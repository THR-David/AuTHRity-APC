use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::mpsc::Receiver;

use crate::infrastructure::InfrastructureConfig;
use opcua::{
    client::{ClientBuilder, DataChangeCallback, IdentityToken, Session},
    crypto::{PrivateKey, X509},
    types::{
        AttributeId, MessageSecurityMode, MonitoredItemCreateRequest, MonitoringMode, MonitoringParameters,
        NodeId, NumericRange, ReadValueId, TimestampsToReturn, UserTokenPolicy, Variant,
    },
};

// --- DATA STRUCTURES ---

#[derive(Debug)]
pub enum OpcCommand {
    WriteNumber(String, f64),
    WriteString(String, String),
    WriteBool(String, bool),
    ReadAll,
    Reconnect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpcUpdate {
    pub node_id: String,
    pub value: serde_json::Value,
    pub timestamp: String,
    pub status: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NodesFile {
    pub nodes: Vec<NodeSpec>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NodeSpec {
    #[serde(alias = "NodeId", alias = "nodeId")]
    pub node_id: String,
    #[serde(alias = "NodeClass", alias = "nodeClass")]
    pub node_class: Option<String>,
    #[serde(alias = "Description", alias = "description")]
    pub description: Option<String>,
}

// --- HELPER: BROADCAST SYSTEM STATUS ---
fn broadcast_sys(tx: &broadcast::Sender<OpcUpdate>, tag: &str, val: i32) {
    let _ = tx.send(OpcUpdate {
        node_id: format!("System:{}", tag),
        value: serde_json::json!(val),
        timestamp: chrono::Local::now().to_rfc3339(),
        status: 1,
    });
}

fn broadcast_sys_text(tx: &broadcast::Sender<OpcUpdate>, tag: &str, val: &str) {
    let _ = tx.send(OpcUpdate {
        node_id: format!("System:{}", tag),
        value: serde_json::json!(val),
        timestamp: chrono::Local::now().to_rfc3339(),
        status: 1,
    });
}

// --- MAIN WORKER ---

pub async fn run_opc_worker(
    settings: crate::config::AppSettings,
    mut cmd_rx: tokio::sync::mpsc::Receiver<OpcCommand>,
    tx: broadcast::Sender<OpcUpdate>,
) -> Result<()> {
    
    let mut client_builder = ClientBuilder::new()
        .application_name(&settings.identity.app_name)
        .application_uri(&settings.identity.app_uri)
        .trust_server_certs(settings.identity.trust_server_certs)
        .create_sample_keypair(settings.identity.auto_create_keys) 
        .session_retry_limit(3)
        .pki_dir(std::path::PathBuf::from(&settings.paths.pki_dir))
        .certificate_path(&settings.paths.cert_path)
        .private_key_path(&settings.paths.key_path)
        .client()
        .map_err(|e| anyhow!("Failed to build client: {:?}", e))?;

    let cert_bytes = std::fs::read(&settings.paths.cert_path)?;
    let x509 = X509::from_der(&cert_bytes)?;
    let key_bytes = std::fs::read(&settings.paths.key_path)?;
    let private_key = PrivateKey::from_pem(&key_bytes)?;

    let mut heartbeat_counter = 0;
    let mut connect_failures: u32 = 0;
    let base_retry = settings.runtime.reconnect_delay_sec.max(2);

    loop {
        // Dynamic Config Lookup: Prioritize SQLite infrastructure settings over settings.toml
        let mut target_endpoint = settings.opcua.endpoint_url.clone();
        let mut target_security_policy = settings.opcua.security_policy.clone();
        let mut target_security_mode = parse_message_mode(&settings.opcua.message_mode);
        let mut target_auth_mode = settings.auth.mode.as_str().to_string();
        let mut target_username = settings.auth.username.clone().unwrap_or_default();
        let mut target_password = settings.auth.password.clone().unwrap_or_default();
        
        if let Ok(infra) = crate::infrastructure::InfrastructureConfig::load_from_db(&settings.hmi_auth.database_url).await {
             if let Some(opc_srv) = infra.opc_servers.first() {
                 if let Some(ep) = &opc_srv.opc_endpoint {
                     if !ep.is_empty() {
                         target_endpoint = ep.clone();
                     }
                 }
             }

             target_security_policy = infra.hmi_client.security_policy.clone();
             target_security_mode = parse_message_mode(&infra.hmi_client.security_mode);
             target_auth_mode = infra.hmi_client.auth_mode.clone();
               let (resolved_username, resolved_password) = infra.resolve_hmi_username_password();
               target_username = resolved_username.unwrap_or_default();
               target_password = resolved_password.unwrap_or_default();
        }

        println!("🔄 HMI Worker: Connecting to {}...", target_endpoint);
        broadcast_sys(&tx, "PlcConnection", 0);

        let (user_token_policy, identity_token) = match normalize_auth_mode(&target_auth_mode).as_str() {
            "anonymous" => (
                UserTokenPolicy {
                    token_type: opcua::types::UserTokenType::Anonymous,
                    ..Default::default()
                },
                IdentityToken::Anonymous,
            ),
            "username" => {
                if target_username.trim().is_empty() || target_password.trim().is_empty() {
                    connect_failures = connect_failures.saturating_add(1);
                    let delay = compute_retry_delay(base_retry, connect_failures, true);
                    broadcast_sys(&tx, "PlcAuthError", 1);
                    broadcast_sys_text(&tx, "PlcLastError", "Username auth selected but username/password is empty");
                    println!(
                        "❌ Username auth selected but credentials are empty. Retrying in {}s...",
                        delay
                    );
                    let _ = wait_retry_or_reconnect(&mut cmd_rx, delay).await;
                    continue;
                }
                (
                    UserTokenPolicy {
                        token_type: opcua::types::UserTokenType::UserName,
                        ..Default::default()
                    },
                    IdentityToken::UserName(target_username.clone(), target_password.clone().into()),
                )
            }
            _ => (
                    UserTokenPolicy {
                        token_type: opcua::types::UserTokenType::Certificate,
                        ..Default::default()
                    },
                    IdentityToken::X509(Box::new(x509.clone()), Box::new(private_key.clone())),
                ),
        };

        let session = match client_builder.connect_to_matching_endpoint(
            (
                target_endpoint.as_str(),
                target_security_policy.as_str(),
                target_security_mode,
                user_token_policy,
            ),
            identity_token,
        ).await {
            Ok((s, handle)) => {
                let _ = handle.spawn();
                s
            },
            Err(e) => {
                connect_failures = connect_failures.saturating_add(1);
                let error_text = e.to_string();
                let lowered = error_text.to_ascii_lowercase();
                let is_auth_error = lowered.contains("baduseraccessdenied")
                    || lowered.contains("identity")
                    || lowered.contains("user access denied")
                    || lowered.contains("badidentitytoken");
                let delay = compute_retry_delay(base_retry, connect_failures, is_auth_error);
                if is_auth_error {
                    broadcast_sys(&tx, "PlcAuthError", 1);
                }
                broadcast_sys_text(&tx, "PlcLastError", &error_text);
                println!("❌ Connection Error: {}. Retrying in {}s...", e, delay);
                broadcast_sys(&tx, "PlcConnection", 0);
                let _ = wait_retry_or_reconnect(&mut cmd_rx, delay).await;
                continue;
            }
        };

        if let Err(err_text) = validate_session_connection(&session).await {
            connect_failures = connect_failures.saturating_add(1);
            let lowered = err_text.to_ascii_lowercase();
            let is_auth_error = lowered.contains("badidentitytokenrejected")
                || lowered.contains("badidentitytoken")
                || lowered.contains("baduseraccessdenied")
                || lowered.contains("identity")
                || lowered.contains("user access denied")
                || lowered.contains("authenticate");
            let delay = compute_retry_delay(base_retry, connect_failures, is_auth_error);
            if is_auth_error {
                broadcast_sys(&tx, "PlcAuthError", 1);
            }
            broadcast_sys_text(&tx, "PlcLastError", &err_text);
            broadcast_sys(&tx, "PlcConnection", 0);
            println!("❌ Session validation failed: {}. Retrying in {}s...", err_text, delay);
            let _ = wait_retry_or_reconnect(&mut cmd_rx, delay).await;
            continue;
        }

        connect_failures = 0;
        broadcast_sys(&tx, "PlcAuthError", 0);
        broadcast_sys_text(&tx, "PlcLastError", "");

        println!("✅ Connected via OPC UA!");
        broadcast_sys(&tx, "PlcConnection", 1);
        
        tokio::time::sleep(Duration::from_secs(1)).await;

        // --- SUBSCRIBE & COLLECT TAGS ---
        let ns = settings.opcua.namespace_index;
        let mut all_monitored_nodes: Vec<NodeId> = Vec::new(); 
        // Discover nodes from remote OPC management API only.
        if let Ok(infra) = InfrastructureConfig::load_from_db(&settings.hmi_auth.database_url).await {
            if let Some(opc_srv) = infra.opc_servers.first() {
                let mgmt_url = &opc_srv.url;
                println!("🔍 Discovering nodes from: {}", mgmt_url);
                
                let client = Client::new();
                
                // Fetch list of models
                match client.get(&format!("{}/api/nodes", mgmt_url)).send().await {
                    Ok(resp) => {
                        if let Ok(models) = resp.json::<Vec<String>>().await {
                            println!("   -> Found {} remote models.", models.len());
                            
                            for model_id in models {
                                let model_url = format!("{}/api/nodes/{}", mgmt_url, model_id);
                                if let Ok(resp) = client.get(&model_url).send().await {
                                    if let Ok(content) = resp.text().await {
                                        if let Ok(nodes_file) = serde_yaml::from_str::<NodesFile>(&content) {
                                            println!("   -> Subscribing to model: {}", model_id);
                                            match subscribe_to_nodes(&session, ns, nodes_file, tx.clone()).await {
                                                Ok(nodes) => {
                                                    all_monitored_nodes.extend(nodes);
                                                }
                                                Err(e) => println!("   -> Failed to subscribe: {}", e),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Err(e) => println!("⚠️ Remote discovery failed: {}", e),
                }
            }
        }

        println!("✅ Worker Ready. Tracking {} tags.", all_monitored_nodes.len());

        // 5. Main Loop
        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        OpcCommand::WriteNumber(node_id, value) => {
                            println!("📝 Writing number {} to {}", value, node_id);
                            let node_to_write = NodeId::new(ns, node_id.clone()); 
                            let write_value = opcua::types::DataValue::new_now(opcua::types::Variant::Double(value));
                            
                            let write_req = opcua::types::WriteValue {
                                node_id: node_to_write,
                                attribute_id: AttributeId::Value as u32,
                                index_range: NumericRange::None,
                                value: write_value,
                            };

                            if let Err(e) = session.write(&[write_req]).await {
                                println!("❌ Write Transport Error: {}", e);
                            }
                        },
                        
                        OpcCommand::WriteString(node_id, value) => {
                            println!("📝 Writing string '{}' to {}", value, node_id);
                            let node_to_write = NodeId::new(ns, node_id.clone());
                            let write_value = opcua::types::DataValue::new_now(opcua::types::Variant::String(value.into()));
                            
                            let write_req = opcua::types::WriteValue {
                                node_id: node_to_write,
                                attribute_id: AttributeId::Value as u32,
                                index_range: NumericRange::None,
                                value: write_value,
                            };

                            if let Err(e) = session.write(&[write_req]).await {
                                println!("❌ Write Transport Error: {}", e);
                            }
                        },
                        
                        OpcCommand::WriteBool(node_id, value) => {
                            println!("📝 Writing bool {} to {}", value, node_id);
                            let node_to_write = NodeId::new(ns, node_id.clone());
                            let write_value = opcua::types::DataValue::new_now(opcua::types::Variant::Boolean(value));
                            
                            let write_req = opcua::types::WriteValue {
                                node_id: node_to_write,
                                attribute_id: AttributeId::Value as u32,
                                index_range: NumericRange::None,
                                value: write_value,
                            };

                            if let Err(e) = session.write(&[write_req]).await {
                                println!("❌ Write Transport Error: {}", e);
                            }
                        },
                        
                        OpcCommand::ReadAll => {
                            println!("♻️ Refreshing all tags for new client...");
                            if !all_monitored_nodes.is_empty() {
                                let _ = read_and_broadcast(&session, &all_monitored_nodes, &tx).await;
                                broadcast_sys(&tx, "PlcConnection", 1);
                            }
                        },
                        OpcCommand::Reconnect => {
                            println!("🔁 Reconnect requested by settings change.");
                            break;
                        }
                    }
                }

                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    heartbeat_counter += 1;
                    if heartbeat_counter > 999 { heartbeat_counter = 0; }
                    broadcast_sys(&tx, "Heartbeat", heartbeat_counter);
                    broadcast_sys(&tx, "PlcConnection", 1);

                    let ping_node = ReadValueId {
                        node_id: NodeId::new(0, 2259), 
                        attribute_id: AttributeId::Value as u32,
                        index_range: NumericRange::None,
                        data_encoding: opcua::types::QualifiedName::null(),
                    };
                    
                    if let Err(_) = session.read(&[ping_node], TimestampsToReturn::Neither, 0.0).await {
                        println!("⚠️ Heartbeat Failed. Reconnecting...");
                        break; 
                    }
                }
            }
        }
    }
}

// --- HELPER: SUBSCRIBE FROM YAML FLAT LIST ---

async fn subscribe_to_nodes(
    session: &Arc<Session>,
    ns: u16,
    nodes_file: NodesFile,
    tx: broadcast::Sender<OpcUpdate>,
) -> Result<Vec<NodeId>> {
    
    // Filter out Objects, keep variables
    let variables: Vec<NodeId> = nodes_file.nodes.into_iter()
        .filter(|n| n.node_class.as_deref() != Some("Object"))
        .map(|n| NodeId::new(ns, n.node_id))
        .collect();

    if variables.is_empty() { return Ok(vec![]); }

    let tx_cb = tx.clone(); 

    let callback = DataChangeCallback::new(move |dv, item| {
        let node_id_obj = &item.item_to_monitor().node_id;
        let node_id = match &node_id_obj.identifier {
            opcua::types::Identifier::String(s) => s.to_string(),
            opcua::types::Identifier::Numeric(n) => n.to_string(),
            _ => node_id_obj.to_string(), 
        };
        
        let value = if let Some(ref val) = dv.value {
            variant_to_json(val)
        } else {
            serde_json::Value::Null
        };

        let _ = tx_cb.send(OpcUpdate {
            node_id, 
            value,
            timestamp: chrono::Local::now().to_rfc3339(),
            status: dv.status.map(|s| s.bits()).unwrap_or(0),
        });
    });

    let sub_id = session.create_subscription(
        Duration::from_secs(1),
        100, 10, 0, 0, true, callback
    ).await.map_err(|e| anyhow!("Failed to create subscription: {}", e))?;

    let items_req: Vec<MonitoredItemCreateRequest> = variables.iter().enumerate()
        .map(|(i, node_id)| {
            MonitoredItemCreateRequest {
                item_to_monitor: ReadValueId {
                    node_id: node_id.clone(),
                    attribute_id: AttributeId::Value as u32,
                    index_range: NumericRange::None,
                    data_encoding: opcua::types::QualifiedName::null(),
                },
                monitoring_mode: MonitoringMode::Reporting,
                requested_parameters: MonitoringParameters {
                    client_handle: i as u32 + 1,
                    sampling_interval: 250.0,
                    filter: Default::default(),
                    queue_size: 1,
                    discard_oldest: true,
                },
            }
        }).collect();

    let _ = session.create_monitored_items(sub_id, TimestampsToReturn::Both, items_req).await?;

    read_and_broadcast(session, &variables, &tx).await?;

    Ok(variables)
}

async fn read_and_broadcast(
    session: &Arc<Session>,
    nodes: &[NodeId],
    tx: &broadcast::Sender<OpcUpdate>
) -> Result<()> {
    
    let read_ids: Vec<ReadValueId> = nodes.iter().map(|node_id| {
        ReadValueId {
            node_id: node_id.clone(),
            attribute_id: AttributeId::Value as u32,
            index_range: NumericRange::None,
            data_encoding: opcua::types::QualifiedName::null(),
        }
    }).collect();

    if let Ok(results) = session.read(&read_ids, TimestampsToReturn::Both, 0.0).await {
        for (i, val) in results.iter().enumerate() {
            if let Some(status) = val.status {
                if status.is_bad() {
                    // println!("❌ Bad Status for {}: {:?}", nodes[i], status);
                }
            }

            let value = if let Some(ref variant) = val.value {
                variant_to_json(variant)
            } else {
                serde_json::Value::Null
            };

            let node_id_str = match &nodes[i].identifier {
                opcua::types::Identifier::String(s) => s.to_string(),
                opcua::types::Identifier::Numeric(n) => n.to_string(),
                _ => nodes[i].to_string(),
            };

            let _ = tx.send(OpcUpdate {
                node_id: node_id_str,
                value,
                timestamp: chrono::Local::now().to_rfc3339(),
                status: val.status.map(|s| s.bits()).unwrap_or(0),
            });
        }
    }
    Ok(())
}

fn variant_to_json(var: &Variant) -> serde_json::Value {
    match var {
        Variant::Double(d) => serde_json::json!(d),
        Variant::Float(f) => serde_json::json!(f),
        Variant::Int32(i) => serde_json::json!(i),
        Variant::Boolean(b) => serde_json::json!(b),
        Variant::String(s) => serde_json::json!(s.as_ref()),
        Variant::Array(arr) => {
            let json_arr: Vec<serde_json::Value> = arr.values.iter()
                .map(variant_to_json)
                .collect();
            serde_json::Value::Array(json_arr)
        },
        _ => serde_json::json!("Unsupported"),
    }
}

fn parse_message_mode(mode: &str) -> MessageSecurityMode {
    match mode.trim().to_ascii_lowercase().as_str() {
        "none" => MessageSecurityMode::None,
        "sign" => MessageSecurityMode::Sign,
        "signandencrypt" | "sign_and_encrypt" | "sign&encrypt" => MessageSecurityMode::SignAndEncrypt,
        _ => MessageSecurityMode::SignAndEncrypt,
    }
}

fn normalize_auth_mode(mode: &str) -> String {
    match mode.trim().to_ascii_lowercase().as_str() {
        "anonymous" | "anon" => "anonymous".to_string(),
        "credentials" | "credential" | "username/password" | "username" => "username".to_string(),
        "certificate" | "cert" | "x509" => "x509".to_string(),
        _ => "username".to_string(),
    }
}

fn compute_retry_delay(base_retry: u64, failures: u32, auth_related: bool) -> u64 {
    let multiplier = if auth_related { 3 } else { 1 };
    let power = failures.saturating_sub(1).min(3);
    let exponential = 1u64 << power;
    (base_retry * multiplier * exponential).min(60)
}

async fn wait_retry_or_reconnect(cmd_rx: &mut Receiver<OpcCommand>, delay_secs: u64) -> bool {
    let sleeper = tokio::time::sleep(Duration::from_secs(delay_secs));
    tokio::pin!(sleeper);

    loop {
        tokio::select! {
            _ = &mut sleeper => return false,
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(OpcCommand::Reconnect) => return true,
                    Some(_) => continue,
                    None => return false,
                }
            }
        }
    }
}

async fn validate_session_connection(session: &Arc<Session>) -> std::result::Result<(), String> {
    let ping_node = ReadValueId {
        node_id: NodeId::new(0, 2259),
        attribute_id: AttributeId::Value as u32,
        index_range: NumericRange::None,
        data_encoding: opcua::types::QualifiedName::null(),
    };

    let mut last_error = String::new();
    for _ in 0..6 {
        match session.read(&[ping_node.clone()], TimestampsToReturn::Neither, 0.0).await {
            Ok(values) => {
                if let Some(value) = values.first() {
                    if let Some(status) = value.status {
                        if status.is_bad() {
                            let status_text = format!("{:?}", status).to_ascii_lowercase();
                            let is_auth_or_session = status_text.contains("identity")
                                || status_text.contains("access")
                                || status_text.contains("session")
                                || status_text.contains("user");
                            if is_auth_or_session {
                                return Err(format!("OPC read status: {:?}", status));
                            }

                            if status_text.contains("notconnected") {
                                last_error = format!("OPC read status: {:?}", status);
                                tokio::time::sleep(Duration::from_millis(300)).await;
                                continue;
                            }
                        }
                    }
                }
                return Ok(());
            }
            Err(e) => {
                let err_text = e.to_string();
                if err_text.to_ascii_lowercase().contains("badnotconnected") {
                    last_error = err_text;
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    continue;
                }
                return Err(err_text);
            }
        }
    }

    Err(if last_error.is_empty() {
        "OPC session validation failed".to_string()
    } else {
        last_error
    })
}