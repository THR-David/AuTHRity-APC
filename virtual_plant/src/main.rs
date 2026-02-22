// src/main.rs

mod config;
mod opc_interface;
mod plant; 

use anyhow::{Result, anyhow, Context};
use std::path::PathBuf;
use std::time::Duration;

use opcua::{
    client::{ClientBuilder, IdentityToken},
    crypto::{SecurityPolicy, X509, PrivateKey},
    types::{MessageSecurityMode, NodeId, UserTokenPolicy, UserTokenType},
};

use plant::debutanizer::{DebutanizerModel, ColumnConfig, ModelInputs, MW, DENSITY};
use plant::cstr::{CSTRModel, CSTRInputs};
use config::{AppSettings, AuthMode};

// --- NODE NAME CONSTANTS ---

// VP_Debutanizer Nodes
const NODE_REFLUX_SP: &str = "Reflux:SP"; 
const NODE_REFLUX_PV: &str = "Reflux:PV";
const NODE_REFLUX_OP: &str = "Reflux:OP";
const NODE_REFLUX_MODE: &str = "Reflux:Mode";
const NODE_STEAM_SP: &str  = "Steam:SP";   
const NODE_STEAM_PV: &str  = "Steam:PV";
const NODE_STEAM_OP: &str  = "Steam:OP";
const NODE_STEAM_MODE: &str = "Steam:Mode";
const NODE_FEED_DV: &str   = "Feed_Flow:PV"; 
const NODE_TOP_C4_CV: &str = "Top_C4:PV";      
const NODE_BTM_TEMP_CV: &str = "Btm_Temp:PV";

// VP_CSTR Nodes
const NODE_CSTR_COOLING_SP: &str = "CSTR_Cooling:SP"; 
const NODE_CSTR_COOLING_PV: &str = "CSTR_Cooling:PV";
const NODE_CSTR_COOLING_OP: &str = "CSTR_Cooling:OP"; 
const NODE_CSTR_COOLING_MODE: &str = "CSTR_Cooling:Mode";
const NODE_CSTR_FEED_DV: &str    = "CSTR_Feed:PV";    
const NODE_CSTR_TEMP_CV: &str    = "CSTR_Temp:PV";    
const NODE_CSTR_CONC_CV: &str    = "CSTR_Conc:PV"; 

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    println!("Starting AuTHRity: Combined Plant Simulator...");

    // 1. Load Configuration
    let settings_path = "config/settings.toml";
    let settings = AppSettings::load(settings_path)
        .with_context(|| format!("Failed to load config from {}", settings_path))?;
    
    println!("Configuration loaded:");
    println!("   - OPC UA: {}", settings.opcua.endpoint_url);
    println!("   - App: {} ({})", settings.identity.app_name, settings.identity.app_uri);
    println!("   - Speed: {}x real-time", settings.runtime.speed_multiplier);

    // 2. Setup Paths & Certificates
    let project_root = PathBuf::from(".");
    let cert_path = project_root.join(&settings.paths.cert_path);
    let key_path = project_root.join(&settings.paths.key_path);

    // 3. Configure Client
    let mut client_builder = ClientBuilder::new()
        .application_name(&settings.identity.app_name)
        .application_uri(&settings.identity.app_uri)
        .pki_dir(project_root)
        .certificate_path(&cert_path)
        .private_key_path(&key_path)
        .create_sample_keypair(settings.identity.auto_create_keys)
        .trust_server_certs(settings.identity.trust_server_certs)
        .client()
        .map_err(|e| anyhow!("Config error: {:?}", e))?;

    let cert_bytes = std::fs::read(&cert_path).context("Read cert.der")?;
    let x509 = X509::from_der(&cert_bytes).map_err(|e| anyhow!("Parse Cert: {:?}", e))?;
    let key_bytes = std::fs::read(&key_path).context("Read private.pem")?;
    let private_key = PrivateKey::from_pem(&key_bytes).map_err(|e| anyhow!("Parse Key: {:?}", e))?;

    if settings.auth.mode == AuthMode::Username {
        let user = settings.auth.username.as_deref().map(str::trim);
        let pass = settings.auth.password.as_deref().map(str::trim);
        if user.is_none() || pass.is_none() || user == Some("") || pass == Some("") {
            return Err(anyhow!("Invalid OPC auth config: username mode requires non-empty auth.username and auth.password"));
        }
    }

    if settings.auth.mode == AuthMode::Username {
        let user = settings.auth.username.as_deref().unwrap_or("").trim();
        let pass = settings.auth.password.as_deref().unwrap_or("").trim();
        if user.is_empty() || pass.is_empty() {
            return Err(anyhow!("Invalid OPC auth config: username mode requires non-empty auth.username and auth.password"));
        }
    }

    // 4. Define Nodes
    let namespace = settings.opcua.namespace_index;
    
    let read_nodes = vec![
        NodeId::new(namespace, NODE_REFLUX_SP),
        NodeId::new(namespace, NODE_STEAM_SP),
        NodeId::new(namespace, NODE_FEED_DV),
        NodeId::new(namespace, NODE_CSTR_COOLING_SP),
        NodeId::new(namespace, NODE_CSTR_FEED_DV),
    ];

    let write_deb_top = NodeId::new(namespace, NODE_TOP_C4_CV);
    let write_deb_btm = NodeId::new(namespace, NODE_BTM_TEMP_CV);
    let write_reflux_pv = NodeId::new(namespace, NODE_REFLUX_PV);
    let write_reflux_op = NodeId::new(namespace, NODE_REFLUX_OP);
    let write_steam_pv = NodeId::new(namespace, NODE_STEAM_PV);
    let write_steam_op = NodeId::new(namespace, NODE_STEAM_OP);
    
    let write_cstr_temp = NodeId::new(namespace, NODE_CSTR_TEMP_CV);
    let write_cstr_conc = NodeId::new(namespace, NODE_CSTR_CONC_CV);
    let write_cstr_cool_pv = NodeId::new(namespace, NODE_CSTR_COOLING_PV);
    let write_cstr_cool_op = NodeId::new(namespace, NODE_CSTR_COOLING_OP);

    // 5. Initialize Physics Engines
    let mut plant_deb = DebutanizerModel::new(ColumnConfig {
        num_stages: settings.debutanizer.num_stages,
        feed_stage: settings.debutanizer.feed_stage,
        relative_volatility: settings.debutanizer.relative_volatility,
        hold_up_molar: settings.debutanizer.hold_up_molar,       
        dt_seconds: settings.debutanizer.dt_seconds,          
    });

    let mut plant_cstr = CSTRModel::new();

    println!("Physics Engines Loaded. Waiting for OPC UA...");

    // Parse security settings
    let security_policy = match settings.opcua.security_policy.as_str() {
        "None" => SecurityPolicy::None,
        "Basic128Rsa15" => SecurityPolicy::Basic128Rsa15,
        "Basic256" => SecurityPolicy::Basic256,
        "Basic256Sha256" => SecurityPolicy::Basic256Sha256,
        "Aes128Sha256RsaOaep" => SecurityPolicy::Aes128Sha256RsaOaep,
        "Aes256Sha256RsaPss" => SecurityPolicy::Aes256Sha256RsaPss,
        _ => {
            println!("Unknown security policy, defaulting to Basic256Sha256");
            SecurityPolicy::Basic256Sha256
        }
    };
    
    let message_mode = match settings.opcua.message_mode.as_str() {
        "None" => MessageSecurityMode::None,
        "Sign" => MessageSecurityMode::Sign,
        "SignAndEncrypt" => MessageSecurityMode::SignAndEncrypt,
        _ => {
            println!("Unknown message mode, defaulting to SignAndEncrypt");
            MessageSecurityMode::SignAndEncrypt
        }
    };

    // 6. Connection Loop
    loop {
        println!("Connecting to {}...", settings.opcua.endpoint_url);
        let (identity, token_type) = match settings.auth.mode {
            AuthMode::Username => (
                IdentityToken::UserName(
                    settings.auth.username.clone().unwrap_or_default(),
                    settings.auth.password.clone().unwrap_or_default().into(),
                ),
                UserTokenType::UserName,
            ),
            AuthMode::X509 => (
                IdentityToken::X509(Box::new(x509.clone()), Box::new(private_key.clone())),
                UserTokenType::Certificate,
            ),
        };
        
        let connect_result = tokio::time::timeout(
            Duration::from_secs(15),
            client_builder.connect_to_matching_endpoint(
                (
                    settings.opcua.endpoint_url.as_str(),
                    security_policy.to_str(),
                    message_mode,
                    UserTokenPolicy { token_type, ..Default::default() },
                ),
                identity,
            ),
        ).await;

        let session = match connect_result {
            Ok(Ok((s, _h))) => {
                let _ = _h.spawn();
                s
            }
            Ok(Err(e)) => {
                println!("Connect failed: {}. Retrying in {}s...", e, settings.runtime.reconnect_delay_sec);
                tokio::time::sleep(Duration::from_secs(settings.runtime.reconnect_delay_sec)).await;
                continue;
            }
            Err(_) => {
                println!("Connect timed out after 15s. Retrying in {}s...", settings.runtime.reconnect_delay_sec);
                tokio::time::sleep(Duration::from_secs(settings.runtime.reconnect_delay_sec)).await;
                continue;
            }
        };

        let connected = tokio::time::timeout(Duration::from_secs(10), session.wait_for_connection())
            .await
            .unwrap_or(false);
        if !connected {
            println!("Connection dropped before session activation. Retrying in {}s...", settings.runtime.reconnect_delay_sec);
            tokio::time::sleep(Duration::from_secs(settings.runtime.reconnect_delay_sec)).await;
            continue;
        }

        println!("Connected! Running Simulation.");
        
        // Wait for session to be fully ready for writes
        println!("Waiting for session to stabilize...");
        tokio::time::sleep(Duration::from_secs(3)).await;
        
        // Initialize OPC UA nodes with startup values
        println!("Sending initial values to OPC UA server...");
        let init_reflux = 75.0;
        let init_steam = 150.0;
        let init_feed = 150.0;
        let init_mode = 3.0;  // Mode 3 for immediate control
        
        if let Err(e) = opc_interface::write_single(&session, &write_reflux_pv, init_reflux).await {
            println!("  Warning: Failed to write Reflux:PV - {}", e);
        }
        if let Err(e) = opc_interface::write_single(&session, &write_reflux_op, init_reflux).await {
            println!("  Warning: Failed to write Reflux:OP - {}", e);
        }
        if let Err(e) = opc_interface::write_single(&session, &NodeId::new(namespace, NODE_REFLUX_SP), init_reflux).await {
            println!("  Warning: Failed to write Reflux:SP - {}", e);
        }
        if let Err(e) = opc_interface::write_single(&session, &NodeId::new(namespace, NODE_REFLUX_MODE), init_mode).await {
            println!("  Warning: Failed to write Reflux:Mode - {}", e);
        }
        if let Err(e) = opc_interface::write_single(&session, &NodeId::new(namespace, "Reflux:ModeTarget"), init_mode).await {
            println!("  Warning: Failed to write Reflux:ModeTarget - {}", e);
        }
        
        if let Err(e) = opc_interface::write_single(&session, &write_steam_pv, init_steam).await {
            println!("  Warning: Failed to write Steam:PV - {}", e);
        }
        if let Err(e) = opc_interface::write_single(&session, &write_steam_op, init_steam).await {
            println!("  Warning: Failed to write Steam:OP - {}", e);
        }
        if let Err(e) = opc_interface::write_single(&session, &NodeId::new(namespace, NODE_STEAM_SP), init_steam).await {
            println!("  Warning: Failed to write Steam:SP - {}", e);
        }
        if let Err(e) = opc_interface::write_single(&session, &NodeId::new(namespace, NODE_STEAM_MODE), init_mode).await {
            println!("  Warning: Failed to write Steam:Mode - {}", e);
        }
        if let Err(e) = opc_interface::write_single(&session, &NodeId::new(namespace, "Steam:ModeTarget"), init_mode).await {
            println!("  Warning: Failed to write Steam:ModeTarget - {}", e);
        }
        
        if let Err(e) = opc_interface::write_single(&session, &NodeId::new(namespace, NODE_FEED_DV), init_feed).await {
            println!("  Warning: Failed to write Feed_Flow:PV - {}", e);
        }
        
        println!("Initial values set: Reflux={}m³/h, Steam={}m³/h, Feed={}m³/h, Mode={}", 
                 init_reflux, init_steam, init_feed, init_mode as i32);
        
        // Give writes time to complete
        tokio::time::sleep(Duration::from_secs(2)).await;

        // 7. Simulation Loop
        loop {
            match opc_interface::read_bulk(&session, &read_nodes).await {
                Ok(vals) => {
                    // Parse inputs
                    let reflux_m3h = vals.get(0).unwrap_or(&100.0).max(0.0);
                    let steam_m3h  = vals.get(1).unwrap_or(&135.0).max(0.0);
                    let feed_m3h   = vals.get(2).unwrap_or(&150.0).max(0.0);
                    let cool_temp = vals.get(3).unwrap_or(&300.0).max(250.0);
                    let feed_flow = vals.get(4).unwrap_or(&6.0).max(0.0);

                    // Prepare inputs
                    let deb_inputs = ModelInputs {
                        reflux_l: (reflux_m3h * DENSITY) / MW,
                        boilup_v: (steam_m3h  * DENSITY) / MW, 
                        feed_f:   (feed_m3h   * DENSITY) / MW,
                        feed_z: 0.5, 
                    };

                    let cstr_inputs = CSTRInputs {
                        flow_f: feed_flow,
                        cool_temp: cool_temp,
                        ca_feed: 10.0, 
                        t_feed: 300.0, 
                    };

                    // Run simulation steps (speedup from config)
                    let mut deb_out = plant_deb.step(&deb_inputs);
                    let mut cstr_out = plant_cstr.step(&cstr_inputs);

                    for _ in 0..(settings.runtime.speed_multiplier - 1) {
                        deb_out = plant_deb.step(&deb_inputs);
                        cstr_out = plant_cstr.step(&cstr_inputs);
                    }

                    // Write outputs
                    let _ = opc_interface::write_single(&session, &write_deb_top, deb_out.top_c4_frac).await;
                    let _ = opc_interface::write_single(&session, &write_deb_btm, deb_out.bottom_temp_deg_c).await;
                    let _ = opc_interface::write_single(&session, &write_reflux_pv, reflux_m3h).await;
                    let _ = opc_interface::write_single(&session, &write_reflux_op, reflux_m3h).await;
                    let _ = opc_interface::write_single(&session, &write_steam_pv, steam_m3h).await;
                    let _ = opc_interface::write_single(&session, &write_steam_op, steam_m3h).await;

                    let _ = opc_interface::write_single(&session, &write_cstr_temp, cstr_out.t).await;
                    let _ = opc_interface::write_single(&session, &write_cstr_conc, cstr_out.ca).await;
                    let _ = opc_interface::write_single(&session, &write_cstr_cool_pv, cool_temp).await;
                    let _ = opc_interface::write_single(&session, &write_cstr_cool_op, cool_temp).await;

                    // Log status
                    println!(
                        "Tick (+{}s): [DEB] C4:{:.2}% BtmT:{:.1}C | [CSTR] T:{:.1}K Ca:{:.2}kmol/m3", 
                        settings.runtime.speed_multiplier,
                        deb_out.top_c4_frac * 100.0, deb_out.bottom_temp_deg_c, cstr_out.t, cstr_out.ca
                    );
                }
                Err(e) => {
                    println!("Read Error: {}. Reconnecting...", e);
                    break; 
                }
            }
            tokio::time::sleep(Duration::from_millis(settings.runtime.cycle_time_ms)).await;
        }
        tokio::time::sleep(Duration::from_secs(settings.runtime.reconnect_delay_sec)).await;
    }
}
