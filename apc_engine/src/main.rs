mod config;
mod dmc;
mod opc_interface;
mod modelloader;

use anyhow::{Result, anyhow, Context}; 
use clap::Parser;
use config::{UnifiedModel, AppSettings, AuthMode};
use dmc::DmcController;
use modelloader::{load_parametric_model, load_stepresponse_model};
use std::path::PathBuf;
use std::time::Duration;
use serde_json::json;

use opcua::{
    client::{ClientBuilder, IdentityToken, Session},
    crypto::SecurityPolicy,
    crypto::{X509, PrivateKey},
    types::{MessageSecurityMode, NodeId, UserTokenPolicy, UserTokenType},
};

/// APC Engine Worker
/// 
/// A dedicated process for running a single Model Predictive Control loop.
/// Controlled via CLI arguments by the 'controller_host'.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the JSON model file (Physics)
    #[arg(short, long)]
    model: String,

    /// OPC UA Server URL
    #[arg(short, long, default_value = "opc.tcp://127.0.0.1:4855")]
    opc: String,

    /// PKI Root Directory
    #[arg(long, default_value = "pki")]
    pki: String,

    /// OPC UA auth mode: username or x509
    #[arg(long, default_value = "username")]
    auth_mode: String,

    /// Username for OPC UA username/password auth
    #[arg(long)]
    username: Option<String>,

    /// Password for OPC UA username/password auth
    #[arg(long)]
    password: Option<String>,
}

fn parse_auth_mode(mode: &str) -> Result<AuthMode> {
    match mode.to_ascii_lowercase().as_str() {
        "username" => Ok(AuthMode::Username),
        "x509" => Ok(AuthMode::X509),
        _ => Err(anyhow!("Invalid --auth-mode '{}'. Use 'username' or 'x509'", mode)),
    }
}

/// Save current configuration to new JSON file by reading OPC UA values
async fn save_configuration(
    session: &Session,
    model: &UnifiedModel,
    model_dir: &str,
    ns: u16,
    _sys_node_base: &str,
) -> Result<String> {
    use chrono::Local;
    
    // Build timestamped filename
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let new_filename = format!("{}_saved_{}_model.json", model.metadata.name, timestamp);
    let new_path = PathBuf::from(model_dir).join(&new_filename);
    
    println!("💾 Saving configuration to: {}", new_filename);
    
    // Read current values from OPC UA
    let mut updated_model = model.clone();
    
    // Update CV parameters from OPC UA
    for cv in &mut updated_model.variables.cvs {
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:Weight", cv.name))]).await {
            if let Some(&weight) = vals.first() { cv.weight = weight; }
        }
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:Alpha", cv.name))]).await {
            if let Some(&alpha) = vals.first() { cv.alpha = alpha; }
        }
        // Note: ece_factor is typically static (from model config), not tuned live
        // If needed for live tuning, add OPC UA node read here
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:Target", cv.name))]).await {
            if let Some(&target) = vals.first() { cv.limits.target = target; }
        }
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:LowLimit", cv.name))]).await {
            if let Some(&low) = vals.first() { cv.limits.low = low; }
        }
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:HighLimit", cv.name))]).await {
            if let Some(&high) = vals.first() { cv.limits.high = high; }
        }
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:LowLowLimit", cv.name))]).await {
            if let Some(&ll) = vals.first() { cv.limits.low_low = ll; }
        }
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:HighHighLimit", cv.name))]).await {
            if let Some(&hh) = vals.first() { cv.limits.high_high = hh; }
        }
    }
    
    // Update MV parameters from OPC UA
    for mv in &mut updated_model.variables.mvs {
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:Weight", mv.name))]).await {
            if let Some(&weight) = vals.first() { mv.weight_r = weight; }
        }
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:LowLimit", mv.name))]).await {
            if let Some(&low) = vals.first() { mv.limits.low = low; }
        }
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:HighLimit", mv.name))]).await {
            if let Some(&high) = vals.first() { mv.limits.high = high; }
        }
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:LowLowLimit", mv.name))]).await {
            if let Some(&ll) = vals.first() { mv.limits.low_low = ll; }
        }
        if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:HighHighLimit", mv.name))]).await {
            if let Some(&hh) = vals.first() { mv.limits.high_high = hh; }
        }
        // Update economic target if it exists
        if mv.node_ids.target.is_some() {
            if let Ok(vals) = opc_interface::read_bulk(session, &[NodeId::new(ns, format!("{}:Target", mv.name))]).await {
                if let Some(&target) = vals.first() { mv.target = Some(target); }
            }
        }
    }
    
    // Build JSON structure (matching ModelGenerator.tsx format)
    let json_output = json!({
        "metadata": {
            "name": format!("{}_saved_{}", updated_model.metadata.name, timestamp),
            "description": format!("{} (Saved Configuration)", updated_model.metadata.description),
            "version": updated_model.metadata.version,
            "model_type": updated_model.metadata.model_type
        },
        "tuning": {
            "prediction_horizon": updated_model.tuning.prediction_horizon,
            "control_horizon": updated_model.tuning.control_horizon,
            "sample_time": updated_model.tuning.sample_time,
            "solver_tolerance": updated_model.tuning.solver_tolerance,
            "max_iterations": updated_model.tuning.max_iterations
        },
        "variables": {
            "cvs": updated_model.variables.cvs.iter().map(|cv| json!({
                "name": cv.name,
                "description": cv.description,
                "units": cv.units,
                "weight": cv.weight,
                "alpha": cv.alpha,
                "optimization_mode": match &cv.optimization_mode {
                    config::OptimizationMode::Target { value } => json!({ "type": "Target", "value": value }),
                    config::OptimizationMode::Zone => json!({ "type": "Zone" }),
                    config::OptimizationMode::Maximize => json!({ "type": "Maximize" }),
                    config::OptimizationMode::Minimize => json!({ "type": "Minimize" }),
                },
                "limits": {
                    "low_low": cv.limits.low_low,
                    "low": cv.limits.low,
                    "target": cv.limits.target,
                    "high": cv.limits.high,
                    "high_high": cv.limits.high_high
                },
                "node_ids": {
                    "pv": format!("{}:PV", cv.name),
                    "target": format!("{}:Target", cv.name),
                    "prediction": format!("{}:Prediction", cv.name),
                    "limits": {
                        "high": format!("{}:HighLimit", cv.name),
                        "low": format!("{}:LowLimit", cv.name),
                        "hh": format!("{}:HighHighLimit", cv.name),
                        "ll": format!("{}:LowLowLimit", cv.name)
                    }
                }
            })).collect::<Vec<_>>(),
            "mvs": updated_model.variables.mvs.iter().map(|mv| {
                let mut mv_json = json!({
                    "name": mv.name,
                    "description": mv.description,
                    "units": mv.units,
                    "weight_r": mv.weight_r,
                    "max_move": mv.max_move,
                    "optimization_mode": match &mv.optimization_mode {
                        config::MvOptimizationMode::Target { value } => json!({ "type": "Target", "value": value }),
                        config::MvOptimizationMode::Maximize => json!({ "type": "Maximize" }),
                        config::MvOptimizationMode::Minimize => json!({ "type": "Minimize" }),
                    },
                    "limits": {
                        "low_low": mv.limits.low_low,
                        "low": mv.limits.low,
                        "high": mv.limits.high,
                        "high_high": mv.limits.high_high
                    },
                    "node_ids": {
                        "pv": format!("{}:PV", mv.name),
                        "sp": format!("{}:SP", mv.name),
                        "op": format!("{}:OP", mv.name),
                        "mode": format!("{}:Mode", mv.name),
                        "mode_target": format!("{}:ModeTarget", mv.name),
                        "future_plan": format!("{}:FuturePlan", mv.name),
                        "limits": {
                            "high": format!("{}:HighLimit", mv.name),
                            "low": format!("{}:LowLimit", mv.name),
                            "hh": format!("{}:HighHighLimit", mv.name),
                            "ll": format!("{}:LowLowLimit", mv.name)
                        }
                    }
                });
                // Add optional economic target
                if let Some(target) = mv.target {
                    mv_json["target"] = json!(target);
                    if mv.target_weight > 0.0 {
                        mv_json["target_weight"] = json!(mv.target_weight);
                    }
                    mv_json["node_ids"]["target"] = json!(format!("{}:Target", mv.name));
                }
                mv_json
            }).collect::<Vec<_>>(),
            "dvs": updated_model.variables.dvs.iter().map(|dv| json!({
                "name": dv.name,
                "description": dv.description,
                "units": dv.units,
                "limits": {
                    "low": dv.limits.low,
                    "high": dv.limits.high
                },
                "node_ids": {
                    "pv": format!("{}:PV", dv.name)
                }
            })).collect::<Vec<_>>()
        },
        "physics": {
            "gain": updated_model.physics.gain,
            "tau": updated_model.physics.tau,
            "dead_time": updated_model.physics.dead_time,
            "gain_dv": updated_model.physics.gain_dv,
            "tau_dv": updated_model.physics.tau_dv,
            "dead_time_dv": updated_model.physics.dead_time_dv,
            "step_coefficients": updated_model.physics.step_coefficients,
            "dv_coefficients": updated_model.physics.dv_coefficients
        }
    });
    
    // Write to file
    std::fs::write(&new_path, serde_json::to_string_pretty(&json_output)?)?;
    
    println!("✅ Configuration saved to: {}", new_filename);
    Ok(new_filename)
}

/// Load model and initialize controller
fn load_model_and_controller(model_path: &str) -> Result<(UnifiedModel, DmcController)> {
    let model = UnifiedModel::load(model_path)?;
    println!("✅ Model Loaded: {}", model.metadata.name);

    // Load Step Response Coefficients
    let (step_coefficients, dv_coefficients) = match model.metadata.model_type.as_str() {
        "parametric" => {
            println!("📊 Generating step response from FOPDT parameters...");
            let mv_coeffs = load_parametric_model(&model)?;
            let dv_coeffs = Vec::new();
            (mv_coeffs, dv_coeffs)
        }
        "step_response" => {
            println!("📊 Loading direct step response coefficients...");
            load_stepresponse_model(&model)?
        }
        other => {
            return Err(anyhow!("Unknown model_type: '{}'. Use 'parametric' or 'step_response'", other));
        }
    };

    let controller = DmcController::new_from_coefficients(step_coefficients, dv_coefficients, &model);
    Ok((model, controller))
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let args = Args::parse();
    println!("🔐 Starting Industrial DMC (Worker Mode)...");
    let auth_mode = parse_auth_mode(&args.auth_mode)?;
    
    // Construct AppSettings from Args (Migration from settings.toml)
    let pki_path = PathBuf::from(&args.pki);
    let mut settings = AppSettings {
        opcua: config::OpcuaConfig {
            endpoint_url: args.opc.clone(),
            namespace_index: 2,
        },
        identity: config::IdentityConfig {
            app_name: "AuTHRity APC Engine".to_string(),
            app_uri: "urn:authrity:engine:generic".to_string(), // Will be updated after ID determination
            auto_create_keys: true,
            trust_server_certs: true,
        },
        auth: config::AuthConfig {
            mode: auth_mode,
            username: args.username.clone(),
            password: args.password.clone(),
        },
        runtime: config::RuntimeConfig {
            model_path: args.model.clone(),
            reconnect_delay_sec: 5,
        },
        paths: config::PathConfig {
            pki_dir: args.pki.clone(),
            cert_path: pki_path.join("own/cert.der").to_string_lossy().to_string(),
            key_path: pki_path.join("own/private.pem").to_string_lossy().to_string(),
        }
    };

    // Load initial model
    let (model, mut controller) = load_model_and_controller(&settings.runtime.model_path)?;

    // Determine Identity: Strictly from Model Name > Filename
    let engine_id = if !model.metadata.name.is_empty() {
        model.metadata.name.clone()
    } else {
        std::path::Path::new(&settings.runtime.model_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("UnknownEngine") 
            .to_string()
    };

    // Update Identity URI based on Engine ID
    settings.identity.app_uri = format!("urn:authrity:engine:{}", engine_id);

    println!("⚙️ Engine ID: {}", engine_id);
    println!("⚙️ Model Path: {}", settings.runtime.model_path);
    println!("⚙️ Target OPC: {}", settings.opcua.endpoint_url);

    // 3. Security Config
    let _project_root = PathBuf::from("."); // We use "." because the Arg overrides the path, but the PKI lib takes a base dir
    // We need to ensure the PKI dir exists relative to CWD if using defaults
    if !pki_path.exists() {
        // If we are spawning from controller_host without setting CWD, this might fail unless passed absolute path
        // We will assume controller_host sets CWD correctly or passes absolute path.
    }

    if settings.auth.mode == AuthMode::Username {
        let user = settings.auth.username.as_deref().map(str::trim);
        let pass = settings.auth.password.as_deref().map(str::trim);
        if user.is_none() || pass.is_none() || user == Some("") || pass == Some("") {
            return Err(anyhow!("Invalid OPC auth config: username mode requires --username and --password"));
        }
    }

    let mut client = ClientBuilder::new()
        .application_name(&settings.identity.app_name)
        .application_uri(&settings.identity.app_uri)
        .pki_dir(PathBuf::from("."))
        .certificate_path(&settings.paths.cert_path)
        .private_key_path(&settings.paths.key_path)

        .create_sample_keypair(settings.identity.auto_create_keys)
        .trust_server_certs(settings.identity.trust_server_certs)
        .session_retry_limit(3)
        .client()
        .map_err(|e| anyhow!("Failed to build client: {:?}", e))?;

    let x509_identity = if settings.auth.mode == AuthMode::X509 {
        let cert_bytes = std::fs::read(&settings.paths.cert_path)
            .context(format!("Failed to read cert: {}", settings.paths.cert_path))?;
        let x509 = X509::from_der(&cert_bytes).map_err(|e| anyhow!("Parse Cert Error: {:?}", e))?;

        let key_bytes = std::fs::read(&settings.paths.key_path)
            .context(format!("Failed to read key: {}", settings.paths.key_path))?;
        let private_key = PrivateKey::from_pem(&key_bytes).map_err(|e| anyhow!("Parse Key Error: {:?}", e))?;

        Some((x509, private_key))
    } else {
        None
    };

    // --- OUTER LOOP: RECONNECTION MANAGER ---
    loop {
        println!("🔄 Connecting to server...");

        let (token_type, identity_token) = match settings.auth.mode {
            AuthMode::Username => (
                UserTokenType::UserName,
                IdentityToken::UserName(
                    settings.auth.username.clone().unwrap_or_default(),
                    settings.auth.password.clone().unwrap_or_default().into(),
                ),
            ),
            AuthMode::X509 => (
                UserTokenType::Certificate,
                {
                    let (x509, private_key) = x509_identity
                        .as_ref()
                        .ok_or_else(|| anyhow!("Missing x509 identity for x509 auth mode"))?;
                    IdentityToken::X509(Box::new(x509.clone()), Box::new(private_key.clone()))
                },
            ),
        };

        let session = match client.connect_to_matching_endpoint(
            (
                settings.opcua.endpoint_url.as_str(),
                SecurityPolicy::Basic256Sha256.to_str(),
                MessageSecurityMode::SignAndEncrypt,
                UserTokenPolicy {
                    token_type,
                    ..Default::default()
                },
            ),
            identity_token,
        ).await {
            Ok((s, _loop_handle)) => {
                let _ = _loop_handle.spawn(); 
                s
            },
            Err(e) => {
                println!("❌ Connection failed: {}. Retrying in 5s...", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        session.wait_for_connection().await;
        println!("✅ Secure Session Active.");

        // --- PREPARE NODES (Once per connection) ---
        let ns = settings.opcua.namespace_index;

        // Use Engine ID as absolute system node base (single source of truth)
        let sys_node_base = engine_id.clone();
        println!("🔗 Binding to System Node: {}", sys_node_base);

        // Strict validation: fail fast if model metadata/name and OPC node prefix diverge
        let op_mode_probe = NodeId::new(ns, format!("{}:OperatingMode", sys_node_base));
        if let Err(e) = opc_interface::read_bulk(&session, &[op_mode_probe.clone()]).await {
            return Err(anyhow!(
                "System node prefix mismatch. Expected '{}' from model metadata.name, but '{}' is not readable ({}). Ensure model generator writes identical system node prefix in OPC YAML.",
                sys_node_base,
                format!("{}:OperatingMode", sys_node_base),
                e
            ));
        }

        // ✅ NEW: SYSTEM NODES
        let node_op_mode = NodeId::new(ns, format!("{}:OperatingMode", sys_node_base));
        let node_exec_time = NodeId::new(ns, format!("{}:ExecutionTimeMs", sys_node_base));
        let node_next_run = NodeId::new(ns, format!("{}:NextRun", sys_node_base));
        let node_solver_status = NodeId::new(ns, format!("{}:SolverStatus", sys_node_base));
        let node_heartbeat = NodeId::new(ns, format!("{}:Heartbeat", sys_node_base));
        
        // Configuration Management Nodes
        let node_save_config = NodeId::new(ns, format!("{}:SaveConfiguration", sys_node_base));
        let node_config_status = NodeId::new(ns, format!("{}:ConfigurationStatus", sys_node_base));
        let node_objective_function = NodeId::new(ns, format!("{}:ObjectiveFunction", sys_node_base));

        let mut heartbeat_counter = 0;

        // 1. PVs & Targets
        let pv_nodes: Vec<NodeId> = model.variables.cvs.iter()
            .map(|cv| NodeId::new(ns, cv.node_ids.pv.clone()))
            .collect();
        let target_nodes: Vec<NodeId> = model.variables.cvs.iter()
            .map(|cv| NodeId::new(ns, cv.node_ids.target.clone()))
            .collect();

        // 2. MVs (Read/Write to Setpoint)
        let mv_nodes: Vec<NodeId> = model.variables.mvs.iter()
            .map(|mv| NodeId::new(ns, mv.node_ids.sp.clone())) 
            .collect();
        
        // 2a. MV Modes (Read Only - check if PID accepts RCAS)
        let mv_mode_nodes: Vec<NodeId> = model.variables.mvs.iter()
            .map(|mv| NodeId::new(ns, mv.node_ids.mode.clone()))
            .collect();
        let mv_mode_target_nodes: Vec<NodeId> = model.variables.mvs.iter()
            .map(|mv| NodeId::new(ns, mv.node_ids.mode_target.clone()))
            .collect();
        
        // 2b. MV Economic Targets (Optional - only if target node exists)
        let mv_target_nodes: Vec<Option<NodeId>> = model.variables.mvs.iter()
            .map(|mv| mv.node_ids.target.as_ref().map(|t| NodeId::new(ns, t.clone())))
            .collect();
        
        // 2c. DVs (Read Only)
        let dv_nodes: Vec<NodeId> = model.variables.dvs.iter()
            .map(|dv| NodeId::new(ns, dv.node_ids.pv.clone()))
            .collect();
        
        // 2d. DV Limits (Dynamic Read)
        let dv_high_nodes: Vec<NodeId> = model.variables.dvs.iter()
            .map(|dv| NodeId::new(ns, dv.node_ids.limits.high.clone()))
            .collect();
        let dv_low_nodes: Vec<NodeId> = model.variables.dvs.iter()
            .map(|dv| NodeId::new(ns, dv.node_ids.limits.low.clone()))
            .collect();

        // 3. Visualization Nodes (Future Plan & Prediction)
        let mv_plan_nodes: Vec<NodeId> = model.variables.mvs.iter()
            .map(|mv| NodeId::new(ns, mv.node_ids.future_plan.clone())) 
            .collect();
        let mv_steadystate_nodes: Vec<NodeId> = model.variables.mvs.iter()
            .map(|mv| NodeId::new(ns, format!("{}:SteadyState", mv.name)))
            .collect();
        let mv_lastmove_nodes: Vec<NodeId> = model.variables.mvs.iter()
            .map(|mv| NodeId::new(ns, format!("{}:LastMove", mv.name)))
            .collect();
        let cv_pred_nodes: Vec<NodeId> = model.variables.cvs.iter()
            .map(|cv| NodeId::new(ns, cv.node_ids.prediction.clone())) 
            .collect();
        let cv_steadystate_nodes: Vec<NodeId> = model.variables.cvs.iter()
            .map(|cv| NodeId::new(ns, format!("{}:SteadyState", cv.name)))
            .collect();
        let cv_bias_nodes: Vec<NodeId> = model.variables.cvs.iter()
            .map(|cv| NodeId::new(ns, format!("{}:Bias", cv.name)))
            .collect();

        // 4. Limit Nodes (Dynamic Read)
        let mv_high_nodes: Vec<NodeId> = model.variables.mvs.iter()
            .map(|mv| NodeId::new(ns, mv.node_ids.limits.high.clone())) 
            .collect();
        let mv_low_nodes: Vec<NodeId> = model.variables.mvs.iter()
            .map(|mv| NodeId::new(ns, mv.node_ids.limits.low.clone())) 
            .collect();
        let mv_hh_nodes: Vec<NodeId> = model.variables.mvs.iter()
            .map(|mv| NodeId::new(ns, mv.node_ids.limits.hh.clone())) 
            .collect();
        let mv_ll_nodes: Vec<NodeId> = model.variables.mvs.iter()
            .map(|mv| NodeId::new(ns, mv.node_ids.limits.ll.clone())) 
            .collect();        
        // 5. Operating mode 

        // --- PUSH INITIAL MODEL VALUES TO OPC UA ---
        println!("📤 Pushing initial model configuration to OPC UA...");
        
        // CV Weight nodes
        let cv_weight_nodes: Vec<NodeId> = model.variables.cvs.iter()
            .map(|cv| NodeId::new(ns, format!("{}:Weight", cv.name)))
            .collect();
        
        // CV Alpha nodes
        let cv_alpha_nodes: Vec<NodeId> = model.variables.cvs.iter()
            .map(|cv| NodeId::new(ns, format!("{}:Alpha", cv.name)))
            .collect();
        
        // CV Limit nodes
        let cv_high_nodes: Vec<NodeId> = model.variables.cvs.iter()
            .map(|cv| NodeId::new(ns, format!("{}:HighLimit", cv.name)))
            .collect();
        let cv_low_nodes: Vec<NodeId> = model.variables.cvs.iter()
            .map(|cv| NodeId::new(ns, format!("{}:LowLimit", cv.name)))
            .collect();
        let cv_hh_nodes: Vec<NodeId> = model.variables.cvs.iter()
            .map(|cv| NodeId::new(ns, format!("{}:HighHighLimit", cv.name)))
            .collect();
        let cv_ll_nodes: Vec<NodeId> = model.variables.cvs.iter()
            .map(|cv| NodeId::new(ns, format!("{}:LowLowLimit", cv.name)))
            .collect();
        
        // MV Weight nodes
        let mv_weight_nodes: Vec<NodeId> = model.variables.mvs.iter()
            .map(|mv| NodeId::new(ns, format!("{}:Weight", mv.name)))
            .collect();
        
        // Push CV parameters
        for (i, cv) in model.variables.cvs.iter().enumerate() {
            let _ = opc_interface::write_single(&session, &cv_weight_nodes[i], cv.weight).await;
            let _ = opc_interface::write_single(&session, &cv_alpha_nodes[i], cv.alpha).await;
            let _ = opc_interface::write_single(&session, &cv_high_nodes[i], cv.limits.high).await;
            let _ = opc_interface::write_single(&session, &cv_low_nodes[i], cv.limits.low).await;
            let _ = opc_interface::write_single(&session, &cv_hh_nodes[i], cv.limits.high_high).await;
            let _ = opc_interface::write_single(&session, &cv_ll_nodes[i], cv.limits.low_low).await;
            let _ = opc_interface::write_single(&session, &target_nodes[i], cv.limits.target).await;
            println!("   ✅ {}: Weight={:.2}, Alpha={:.2}, Target={:.2}", cv.name, cv.weight, cv.alpha, cv.limits.target);
        }
        
        // Push DV limits
        for (i, dv) in model.variables.dvs.iter().enumerate() {
            let _ = opc_interface::write_single(&session, &dv_high_nodes[i], dv.limits.high).await;
            let _ = opc_interface::write_single(&session, &dv_low_nodes[i], dv.limits.low).await;
            println!("   ✅ {}: Limits=[{:.1}, {:.1}]", dv.name, dv.limits.low, dv.limits.high);
        }
        
        // Push MV parameters
        for (i, mv) in model.variables.mvs.iter().enumerate() {
            let _ = opc_interface::write_single(&session, &mv_weight_nodes[i], mv.weight_r).await;
            let _ = opc_interface::write_single(&session, &mv_high_nodes[i], mv.limits.high).await;
            let _ = opc_interface::write_single(&session, &mv_low_nodes[i], mv.limits.low).await;
            let _ = opc_interface::write_single(&session, &mv_hh_nodes[i], mv.limits.high_high).await;
            let _ = opc_interface::write_single(&session, &mv_ll_nodes[i], mv.limits.low_low).await;
            println!("   ✅ {}: Weight={:.2}, Limits=[{:.1}, {:.1}]", mv.name, mv.weight_r, mv.limits.low, mv.limits.high);
        }
        
        println!("✅ Initial configuration pushed to OPC UA");
        
        match opc_interface::write_string(&session, &node_config_status, "Running").await {
            Ok(_) => println!("✅ Successfully wrote ConfigurationStatus"),
            Err(e) => println!("❌ Failed to write ConfigurationStatus: {}", e),
        }
        
        // --- SOLVER FAILURE TRACKING (Industry Standard Safety) ---
        let mut consecutive_failures: u32 = 0;
        const MAX_FAILURES_BEFORE_MONITOR: u32 = 2;   // Switch to Monitor after 2 failures
        const MAX_FAILURES_BEFORE_IDLE: u32 = 10;     // Critical stop after 10 total failures

        // --- INNER LOOP: CONTROL LOGIC ---
        loop {
            let tick_start = tokio::time::Instant::now();
            println!("--- Tick ---");
            
            // NOTE: Model Hot-Swap Logic Removed (Engine is now process-isolated)
            
            // CHECK FOR SAVE CONFIGURATION TRIGGER
            if let Ok(should_save) = opc_interface::read_bool(&session, &node_save_config).await {
                if should_save {
                    println!("💾 Configuration save requested...");
                    let _ = opc_interface::write_string(&session, &node_config_status, "Saving configuration...").await;
                    let _ = opc_interface::write_bool(&session, &node_save_config, false).await; // Reset trigger
                    
                    let model_dir = std::path::Path::new(&settings.runtime.model_path)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or(".");

                    match save_configuration(&session, &model, model_dir, ns, &sys_node_base).await {
                        Ok(filename) => {
                            let msg = format!("✅ Configuration saved: {}", filename);
                            println!("{}", msg);
                            let _ = opc_interface::write_string(&session, &node_config_status, &msg).await;
                            // Note: We no longer republish AvailableModels as this engine is isolated
                        }
                        Err(e) => {
                            let msg = format!("❌ Failed to save configuration: {}", e);
                            println!("{}", msg);
                            let _ = opc_interface::write_string(&session, &node_config_status, &msg).await;
                        }
                    }
                }
            }

            // A. READ ABSOLUTE TRUTH
            // Includes Operating Mode check
            let mode_res = opc_interface::read_bulk(&session, &[node_op_mode.clone()]).await;
            
            // Standard reads
            let pvs_res = opc_interface::read_bulk(&session, &pv_nodes).await;
            let tgts_res = opc_interface::read_bulk(&session, &target_nodes).await;
            
            // CRITICAL: Read OP (actual output) not SP (setpoint) for feedback control
            // The DMC needs to know what the PIDs are actually outputting, not what we requested
            let mv_op_nodes_for_read: Vec<NodeId> = model.variables.mvs.iter()
                .map(|mv| NodeId::new(ns, mv.node_ids.op.clone()))
                .collect();
            let mvs_res = opc_interface::read_bulk(&session, &mv_op_nodes_for_read).await;
            let mv_modes_res = opc_interface::read_bulk(&session, &mv_mode_nodes).await;
            let mv_mode_targets_res = opc_interface::read_bulk(&session, &mv_mode_target_nodes).await;
            
            // Read MV targets (only for MVs that have target nodes)
            let mut mv_target_values = vec![0.0; model.variables.mvs.len()];
            for (i, target_node_opt) in mv_target_nodes.iter().enumerate() {
                if let Some(target_node) = target_node_opt {
                    if let Ok(result) = opc_interface::read_bulk(&session, &[target_node.clone()]).await {
                        if let Some(val) = result.first() {
                            mv_target_values[i] = *val;
                        }
                    }
                } else {
                    // No target node, use current MV value (no target tracking)
                    mv_target_values[i] = 0.0;  // Will be ignored due to weight = 0
                }
            }
            
            let dvs_res = if !dv_nodes.is_empty() {
                opc_interface::read_bulk(&session, &dv_nodes).await
            } else {
                Ok(Vec::new())
            };
            
            // DV Limits (Dynamic Read)
            let dv_high_res = if !dv_high_nodes.is_empty() {
                opc_interface::read_bulk(&session, &dv_high_nodes).await
            } else {
                Ok(Vec::new())
            };
            let dv_low_res = if !dv_low_nodes.is_empty() {
                opc_interface::read_bulk(&session, &dv_low_nodes).await
            } else {
                Ok(Vec::new())
            };
            
            // Limits (MV)
            let high_res = opc_interface::read_bulk(&session, &mv_high_nodes).await;
            let low_res = opc_interface::read_bulk(&session, &mv_low_nodes).await;
            let hh_res = opc_interface::read_bulk(&session, &mv_hh_nodes).await;
            let ll_res = opc_interface::read_bulk(&session, &mv_ll_nodes).await;
            
            // Limits (CV) - Read dynamically for input validation
            let cv_high_res = opc_interface::read_bulk(&session, &cv_high_nodes).await;
            let cv_low_res = opc_interface::read_bulk(&session, &cv_low_nodes).await;
            let cv_hh_res = opc_interface::read_bulk(&session, &cv_hh_nodes).await;
            let cv_ll_res = opc_interface::read_bulk(&session, &cv_ll_nodes).await;
            
            // Tuning Parameters (CV) - Read dynamically for live tuning
            let cv_weight_res = opc_interface::read_bulk(&session, &cv_weight_nodes).await;
            let cv_alpha_res = opc_interface::read_bulk(&session, &cv_alpha_nodes).await;
            
            // Tuning Parameters (MV) - Read dynamically for live tuning
            let mv_weight_res = opc_interface::read_bulk(&session, &mv_weight_nodes).await;

            // Debug: Check which read failed
            if mode_res.is_err() {
                println!("❌ Failed to read OperatingMode node: {:?}", node_op_mode);
            }
            if pvs_res.is_err() {
                println!("❌ Failed to read PV nodes. First node: {:?}", pv_nodes.first());
                println!("   Expected {} CV PVs", pv_nodes.len());
            }
            if tgts_res.is_err() {
                println!("❌ Failed to read Target nodes. First node: {:?}", target_nodes.first());
                println!("   Expected {} CV Targets", target_nodes.len());
            }
            if mvs_res.is_err() {
                println!("❌ Failed to read MV SP nodes. First node: {:?}", mv_nodes.first());
                println!("   Expected {} MV SPs", mv_nodes.len());
            }
            if mv_modes_res.is_err() {
                println!("❌ Failed to read MV Mode nodes. First node: {:?}", mv_mode_nodes.first());
                println!("   Expected {} MV Modes", mv_mode_nodes.len());
            }
            if mv_mode_targets_res.is_err() {
                println!("❌ Failed to read MV ModeTarget nodes. First node: {:?}", mv_mode_target_nodes.first());
                println!("   Expected {} MV ModeTargets", mv_mode_target_nodes.len());
            }
            if dvs_res.is_err() {
                println!("❌ Failed to read DV PV nodes. First node: {:?}", dv_nodes.first());
                println!("   Expected {} DV PVs", dv_nodes.len());
            }
            if high_res.is_err() {
                println!("❌ Failed to read MV High Limit nodes. First node: {:?}", mv_high_nodes.first());
            }
            if low_res.is_err() {
                println!("❌ Failed to read MV Low Limit nodes. First node: {:?}", mv_low_nodes.first());
            }
            if hh_res.is_err() {
                println!("❌ Failed to read MV HighHigh Limit nodes. First node: {:?}", mv_hh_nodes.first());
            }
            if ll_res.is_err() {
                println!("❌ Failed to read MV LowLow Limit nodes. First node: {:?}", mv_ll_nodes.first());
            }
            if cv_hh_res.is_err() {
                println!("❌ Failed to read CV HighHigh Limit nodes. First node: {:?}", cv_hh_nodes.first());
            }
            if cv_ll_res.is_err() {
                println!("❌ Failed to read CV LowLow Limit nodes. First node: {:?}", cv_ll_nodes.first());
            }
            if cv_high_res.is_err() {
                println!("❌ Failed to read CV High Limit nodes. First node: {:?}", cv_high_nodes.first());
            }
            if cv_low_res.is_err() {
                println!("❌ Failed to read CV Low Limit nodes. First node: {:?}", cv_low_nodes.first());
            }
            if cv_weight_res.is_err() {
                println!("❌ Failed to read CV Weight nodes. First node: {:?}", cv_weight_nodes.first());
            }
            if cv_alpha_res.is_err() {
                println!("❌ Failed to read CV Alpha nodes. First node: {:?}", cv_alpha_nodes.first());
            }
            if mv_weight_res.is_err() {
                println!("❌ Failed to read MV Weight nodes. First node: {:?}", mv_weight_nodes.first());
            }
            if dv_high_res.is_err() && !dv_high_nodes.is_empty() {
                println!("❌ Failed to read DV High Limit nodes. First node: {:?}", dv_high_nodes.first());
            }
            if dv_low_res.is_err() && !dv_low_nodes.is_empty() {
                println!("❌ Failed to read DV Low Limit nodes. First node: {:?}", dv_low_nodes.first());
            }

            match (pvs_res, tgts_res, mvs_res, mv_modes_res, mv_mode_targets_res, dvs_res, high_res, low_res, hh_res, ll_res, cv_high_res, cv_low_res, cv_hh_res, cv_ll_res, cv_weight_res, cv_alpha_res, mv_weight_res, dv_high_res, dv_low_res, mode_res) {
                (Ok(pvs), Ok(targets), Ok(current_mvs), Ok(mv_modes), Ok(mv_mode_targets), Ok(current_dvs), Ok(lim_h), Ok(lim_l), Ok(lim_hh), Ok(lim_ll), Ok(cv_lim_h), Ok(cv_lim_l), Ok(cv_lim_hh), Ok(cv_lim_ll), Ok(cv_weights), Ok(cv_alphas), Ok(mv_weights), Ok(dv_lim_h), Ok(dv_lim_l), Ok(mode_vec)) => {
                    
                    // 1. DETERMINE MODE
                    // 0 = Idle, 1 = Monitor (Calc but don't write), 2 = Engage
                    let operating_mode = mode_vec.first().cloned().unwrap_or(0.0) as i32;

                    // If Idle, skip calculation but keep heartbeat alive
                    if operating_mode == 0 {
                        println!("💤 Idle (Mode 0). Waiting...");
                        let _ = opc_interface::write_single(&session, &node_solver_status, 0.0).await;
                    } else {
                        // === INPUT VALIDATION (Defense in Depth) ===
                        let mut input_valid = true;
                        
                        // 1. Validate CVs (Process Variables)
                        for (i, &pv) in pvs.iter().enumerate() {
                            let cv = &model.variables.cvs[i];
                            let cv_high_high = cv_lim_hh[i];  // ✅ Dynamic limits from OPC UA
                            let cv_low_low = cv_lim_ll[i];    // ✅ Dynamic limits from OPC UA
                            
                            // Check A: Data corruption (NaN, Inf)
                            if !pv.is_finite() {
                                println!("🔥 CRITICAL: CV '{}' has invalid value: {} (NaN/Inf)", cv.name, pv);
                                println!("   Likely cause: OPC UA communication error or DCS math fault");
                                input_valid = false;
                                break;
                            }
                            
                            // Check B: Physical range violation (sensor failure or new operating window)
                            if pv > cv_high_high || pv < cv_low_low {
                                println!("🚨 CRITICAL: CV '{}' outside operating range: {:.2}", cv.name, pv);
                                println!("   Current limits: [{:.2}, {:.2}]", cv_low_low, cv_high_high);
                                println!("   Possible causes: Sensor failure, wiring issue, or operating window changed");
                                println!("   Action: Adjust limits via HMI or verify sensor health");
                                input_valid = false;
                                break;
                            }
                        }
                        
                        // 2. Validate MVs (Manipulated Variables - feedback)
                        if input_valid {
                            for (i, &mv) in current_mvs.iter().enumerate() {
                                let mv_cfg = &model.variables.mvs[i];
                                let mv_high_high = lim_hh[i];  // ✅ Dynamic limits from OPC UA
                                let mv_low_low = lim_ll[i];    // ✅ Dynamic limits from OPC UA
                                
                                if !mv.is_finite() {
                                    println!("🔥 CRITICAL: MV '{}' feedback invalid: {} (NaN/Inf)", mv_cfg.name, mv);
                                    println!("   DCS may be sending corrupted data");
                                    input_valid = false;
                                    break;
                                }
                                
                                if mv > mv_high_high || mv < mv_low_low {
                                    println!("⚠️ WARNING: MV '{}' feedback outside limits: {:.2}", mv_cfg.name, mv);
                                    println!("   Current limits: [{:.2}, {:.2}]", mv_low_low, mv_high_high);
                                    println!("   Continuing - bias correction will handle discrepancy");
                                    // Don't fail - MV feedback can be outside limits temporarily
                                }
                            }
                        }
                        
                        // 3. Validate DVs (Disturbance Variables)
                        if input_valid && !current_dvs.is_empty() {
                            for (i, &dv) in current_dvs.iter().enumerate() {
                                let dv_cfg = &model.variables.dvs[i];
                                let dv_high = dv_lim_h.get(i).copied().unwrap_or(dv_cfg.limits.high);  // ✅ Dynamic or fallback
                                let dv_low = dv_lim_l.get(i).copied().unwrap_or(dv_cfg.limits.low);    // ✅ Dynamic or fallback
                                
                                if !dv.is_finite() {
                                    println!("🔥 CRITICAL: DV '{}' has invalid value: {} (NaN/Inf)", dv_cfg.name, dv);
                                    input_valid = false;
                                    break;
                                }
                                
                                // DVs only have low/high limits (not low_low/high_high)
                                if dv_high > dv_low {
                                    if dv > dv_high || dv < dv_low {
                                        println!("⚠️ WARNING: DV '{}' outside expected range: {:.2}", dv_cfg.name, dv);
                                        println!("   Current range: [{:.2}, {:.2}]", dv_low, dv_high);
                                        println!("   Note: DVs are measured disturbances - warning only, control continues");
                                    }
                                }
                            }
                        }
                        
                        // If any input is invalid, skip this scan to prevent solver crash
                        if !input_valid {
                            println!("🛑 SKIPPING control scan due to invalid inputs.");
                            println!("   Controller will retry in {} seconds.", model.tuning.sample_time);
                            let _ = opc_interface::write_single(&session, &node_solver_status, 99.0).await; // Status 99 = Input Error
                            
                            // Sleep for sample_time before retrying to avoid flooding terminal
                            let sample_time_duration = Duration::from_secs_f64(model.tuning.sample_time);
                            tokio::time::sleep(sample_time_duration).await;
                            continue; // Skip to next iteration
                        }
                        
                        // === INPUTS VALIDATED - PROCEED WITH CONTROL ===
                        
                        // 🔍 DEBUG: Print parameters being used by DMC controller
                        println!("🔍 DMC PARAMETERS:");
                        println!("   CVs:");
                        for (i, cv) in model.variables.cvs.iter().enumerate() {
                            println!("      {}: Weight={:.2}, Alpha={:.2}, Target={:.2}, Limits=[LL:{:.2}, L:{:.2}, H:{:.2}, HH:{:.2}]", 
                                cv.name, cv_weights[i], cv_alphas[i], targets[i],
                                cv_lim_ll[i], cv_lim_l[i], cv_lim_h[i], cv_lim_hh[i]);
                        }
                        println!("   MVs:");
                        for (i, mv) in model.variables.mvs.iter().enumerate() {
                            println!("      {}: Weight={:.2}, Limits=[LL:{:.2}, L:{:.2}, H:{:.2}, HH:{:.2}]", 
                                mv.name, mv_weights[i],
                                lim_ll[i], lim_l[i], lim_h[i], lim_hh[i]);
                        }
                        
                        // B. SOLVE (Always commit predictions - needed for deadtime tracking)
                        // Bias correction handles discrepancies from unmeasured disturbances (operator moves)
                        let solve_start = tokio::time::Instant::now();
                        
                        let dmc_result = controller.next_move(
                            &pvs, 
                            &targets, 
                            &current_mvs,
                            &current_dvs,
                            &mv_target_values,
                            &lim_l,        // MV lower limits
                            &lim_h,        // MV upper limits
                            &cv_lim_l,     // CV lower limits (for soft constraints & rail-riding)
                            &cv_lim_h,     // CV upper limits (for soft constraints & rail-riding)
                            &cv_weights,   // Dynamic CV weights from OPC UA
                            &cv_alphas,    // Dynamic CV alphas from OPC UA
                            &mv_weights,   // Dynamic MV weights from OPC UA (for future use)
                            operating_mode == 2  // Commit only in Mode 2 (Engage)
                        );

                        // Calc Execution Time
                        let exec_ms = solve_start.elapsed().as_secs_f64() * 1000.0;
                        
                        // Write System Stats
                        let _ = opc_interface::write_single(&session, &node_exec_time, exec_ms).await;
                        
                        // Map solver status to numeric code
                        let status_code = match dmc_result.status {
                            clarabel::solver::SolverStatus::Solved => 0.0,
                            clarabel::solver::SolverStatus::AlmostSolved => 1.0,
                            clarabel::solver::SolverStatus::PrimalInfeasible => 2.0,
                            clarabel::solver::SolverStatus::DualInfeasible => 3.0,
                            clarabel::solver::SolverStatus::MaxIterations => 4.0,
                            clarabel::solver::SolverStatus::MaxTime => 5.0,
                            _ => 99.0,
                        };
                        let _ = opc_interface::write_single(&session, &node_solver_status, status_code).await;

                        // ✅ CHECK SOLVER STATUS - Only write if solved successfully
                        if status_code >= 2.0 {
                            // SOLVER FAILED - Increment failure counter
                            consecutive_failures += 1;
                            
                            let status_name = match status_code as i32 {
                                2 => "PrimalInfeasible (constraints conflict)",
                                3 => "DualInfeasible (unbounded)",
                                4 => "MaxIterations (timeout)",
                                5 => "MaxTime (timeout)",
                                _ => "Unknown error",
                            };
                            
                            println!("⚠️ Solver failed: {} (status code {})", status_name, status_code);
                            println!("   Consecutive failures: {}", consecutive_failures);
                            println!("   Action: Skipping all MV writes - last values preserved.");
                            
                            // CRITICAL STOP: Too many failures - switch to IDLE
                            if consecutive_failures >= MAX_FAILURES_BEFORE_IDLE {
                                println!("🔴 CRITICAL: {} consecutive solver failures!", consecutive_failures);
                                println!("   Auto-switching to IDLE mode (Mode 0).");
                                println!("   Reason: Model or constraints likely invalid.");
                                println!("   Operator action required: Review model, constraints, or operating conditions.");
                                let _ = opc_interface::write_single(&session, &node_op_mode, 0.0).await;
                            }
                            // SAFETY MODE: Multiple failures - switch to MONITOR
                            else if consecutive_failures >= MAX_FAILURES_BEFORE_MONITOR && operating_mode == 2 {
                                println!("🚨 {} consecutive failures while in Engage mode.", consecutive_failures);
                                println!("   Auto-switching to MONITOR mode (Mode 1).");
                                println!("   Reason: Allows solver to stabilize without sending commands.");
                                println!("   Solver will continue calculating - operator can re-engage when status recovers.");
                                let _ = opc_interface::write_single(&session, &node_op_mode, 1.0).await;
                            }
                            
                            // Skip visualization and MV writes to preserve last good state
                            continue;
                        } else {
                            // SOLVER SUCCEEDED - Reset failure counter
                            if consecutive_failures > 0 {
                                println!("✅ Solver recovered successfully after {} failure(s).", consecutive_failures);
                                consecutive_failures = 0;
                            }
                            
                            // Print control summary
                            let mode_str = if operating_mode == 2 { "Engage" } else { "Monitor" };
                            println!("✅ Solver OK ({}ms) | Mode: {}", exec_ms as i32, mode_str);
                        }

                        let delta_mvs = dmc_result.next_move;

                        // Read current OP values for bumpless transfer
                        let mv_op_nodes: Vec<NodeId> = model.variables.mvs.iter()
                            .map(|mv| NodeId::new(ns, mv.node_ids.op.clone()))
                            .collect();
                        let current_ops = opc_interface::read_bulk(&session, &mv_op_nodes).await.unwrap_or_else(|_| vec![0.0; model.variables.mvs.len()]);

                        // --- D. WRITE MV FUTURE PLAN & STEADY STATE ---
                        let m_horizon = model.tuning.control_horizon as usize;
                        let num_mv = model.variables.mvs.len();
                        
                        for (i, _mv) in model.variables.mvs.iter().enumerate() {
                            let u_current = current_mvs[i];
                            let mut plan_absolute = Vec::new();
                            let mut running_val = u_current;
                            
                            for step in 0..m_horizon {
                                let idx = (step * num_mv) + i;
                                let step_delta = dmc_result.future_plan[idx];
                                running_val += step_delta;
                                plan_absolute.push(running_val.clamp(lim_l[i], lim_h[i]));
                            }
                            
                            // Write future plan visualization
                            let _ = opc_interface::write_array(&session, &mv_plan_nodes[i], plan_absolute.clone()).await;
                            
                            // Write MV steady state (last value in control horizon)
                            if let Some(&last_mv) = plan_absolute.last() {
                                let _ = opc_interface::write_single(&session, &mv_steadystate_nodes[i], last_mv).await;
                            }
                            
                            // Write last move delta (only in Engage mode)
                            if operating_mode == 2 {
                                let _ = opc_interface::write_single(&session, &mv_lastmove_nodes[i], delta_mvs[i]).await;
                            }
                        }

                        // --- E. WRITE MV SETPOINTS (MODE & LIMIT CHECKING) ---
                        for (i, delta) in delta_mvs.iter().enumerate() {
                            let u_current = current_mvs[i];
                            let u_proposed = u_current + delta;

                            // 1. Check PID Mode - Generic MPC Protocol
                            // DCS adapter modules translate native modes to this protocol:
                            //   0 = Operator Control (MAN/IMAN/OOS)
                            //   1 = Local Auto (PID to local setpoint)
                            //   2 = Cascade (accepts upstream controller SP)
                            //   3 = Remote Cascade (accepts MPC SP) ← MPC writes here
                            let actual_mode = mv_modes[i] as i32;
                            let target_mode = mv_mode_targets[i] as i32;
                            
                            if actual_mode != 3 {
                                // PID not in Remote Cascade yet
                                if target_mode == 3 {
                                    // Operator requested Remote Cascade - send bumpless setpoint
                                    if operating_mode == 2 {
                                        let bumpless_sp = current_ops[i];
                                        let _ = opc_interface::write_single(&session, &mv_nodes[i], bumpless_sp).await;
                                        println!("🔄 {} preparing for Remote Cascade - sending bumpless SP: {:.2}", 
                                            model.variables.mvs[i].name, bumpless_sp);
                                    }
                                } else {
                                    // Operator hasn't requested Remote Cascade - skip
                                    let mode_name = match actual_mode {
                                        0 => "Operator",
                                        1 => "Auto",
                                        2 => "Cascade",
                                        _ => "Unknown",
                                    };
                                    if operating_mode == 2 {
                                        println!("⏸️  {} in {} mode - MPC not controlling (bias correction handles effects)", 
                                            model.variables.mvs[i].name, mode_name);
                                    }
                                }
                                continue;
                            }

                            // 2. Safety Limit Check
                            if u_proposed > lim_hh[i] || u_proposed < lim_ll[i] {
                                println!("🚨 SAFETY TRIP: {} requested {:.2} (Limits: {:.2}/{:.2})", 
                                    model.variables.mvs[i].name, u_proposed, lim_ll[i], lim_hh[i]);
                                continue; 
                            }

                            // 3. WRITE MV (ONLY IN MODE 2 AND RCAS)
                            if operating_mode == 2 {
                                let _ = opc_interface::write_single(&session, &mv_nodes[i], u_proposed).await;
                                println!("   → {}: {:.2} (Δ{:+.3})", model.variables.mvs[i].name, u_proposed, delta);
                            }
                        }

                        // --- F. WRITE CV PREDICTIONS, BIAS & STEADY STATE ---
                        let p_horizon = model.tuning.prediction_horizon as usize;
                        let num_cv = model.variables.cvs.len();

                        for (j, _cv) in model.variables.cvs.iter().enumerate() {
                            let mut cv_trajectory = Vec::with_capacity(p_horizon);
                            for step in 0..p_horizon {
                                let idx = (step * num_cv) + j;
                                cv_trajectory.push(dmc_result.predicted_pvs[idx]);
                            }
                            
                            // Write prediction trajectory
                            let _ = opc_interface::write_array(&session, &cv_pred_nodes[j], cv_trajectory.clone()).await;
                            
                            // Write CV steady state (last value in prediction horizon)
                            if let Some(&last_cv) = cv_trajectory.last() {
                                let _ = opc_interface::write_single(&session, &cv_steadystate_nodes[j], last_cv).await;
                            }
                            
                            // Write CV bias correction value
                            let _ = opc_interface::write_single(&session, &cv_bias_nodes[j], dmc_result.cv_bias[j]).await;
                        }
                        
                        // Write objective function value
                        let _ = opc_interface::write_single(&session, &node_objective_function, dmc_result.objective_value).await;
                    }
                }
                _ => {
                    println!("⚠️ Communication Error: Failed to read tags.");
                    println!("🔌 Network failed. Restarting connection...");
                    break; // Trigger Reconnect
                }
            }

            // ✅ SYSTEM HEARTBEAT & NEXT RUN
            heartbeat_counter += 1;
            let _ = opc_interface::write_single(&session, &node_heartbeat, heartbeat_counter as f64).await;

            // D. SLEEP (Countdown Loop)
            let target_tick = Duration::from_secs(model.tuning.sample_time as u64);
            
            loop {
                let elapsed = tick_start.elapsed();
                if elapsed >= target_tick {
                    break; // Time is up! Start next run.
                }

                let remaining = target_tick - elapsed;
                
                // Write Time Remaining (e.g., 19.0, 18.0... 0.5)
                // We ignore errors here because missing a timer update isn't critical
                let _ = opc_interface::write_single(
                    &session, 
                    &node_next_run, 
                    remaining.as_secs_f64()
                ).await;

                // Logic: Sleep for 1 second, OR the remaining time if < 1s
                let sleep_step = if remaining.as_secs() > 0 {
                    Duration::from_secs(1)
                } else {
                    remaining // Precise sleep for the final milliseconds
                };
                
                tokio::time::sleep(sleep_step).await;
            }
        }

        println!("⏳ Waiting 5s before reconnecting...");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}