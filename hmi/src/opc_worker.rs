use anyhow::{Result, anyhow};
use std::time::Duration;
use std::sync::Arc;
use reqwest::Client; // Added
use crate::infrastructure::InfrastructureConfig; // Added
use crate::config::AuthMode;

use opcua::{
    client::{ClientBuilder, IdentityToken, Session, DataChangeCallback},
    crypto::{SecurityPolicy, X509, PrivateKey},
    types::{
        MessageSecurityMode, NodeId, UserTokenPolicy, 
        MonitoredItemCreateRequest, MonitoringParameters, 
        MonitoringMode, ReadValueId, AttributeId, 
        TimestampsToReturn, Variant, NumericRange,
    },
};
use tokio::sync::broadcast;
use serde::{Deserialize, Serialize};

// --- DATA STRUCTURES ---

#[derive(Debug)]
pub enum OpcCommand {
    WriteNumber(String, f64),
    WriteString(String, String),
    WriteBool(String, bool),
    ReadAll,
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
        .pki_dir(std::path::PathBuf::from("."))
        .certificate_path(&settings.paths.cert_path)
        .private_key_path(&settings.paths.key_path)
        .client()
        .map_err(|e| anyhow!("Failed to build client: {:?}", e))?;

    let cert_bytes = std::fs::read(&settings.paths.cert_path)?;
    let x509 = X509::from_der(&cert_bytes)?;
    let key_bytes = std::fs::read(&settings.paths.key_path)?;
    let private_key = PrivateKey::from_pem(&key_bytes)?;

    if settings.auth.mode == AuthMode::Username {
        let user = settings.auth.username.as_deref().unwrap_or("").trim();
        let pass = settings.auth.password.as_deref().unwrap_or("").trim();
        if user.is_empty() || pass.is_empty() {
            return Err(anyhow!("Invalid OPC auth config: username mode requires non-empty auth.username and auth.password"));
        }
    }

    let mut heartbeat_counter = 0;

    loop {
        // Dynamic Config Lookup: Prioritize hosts.json (Infrastructure) over settings.toml
        let mut target_endpoint = settings.opcua.endpoint_url.clone();
        
        if let Ok(infra) = crate::infrastructure::InfrastructureConfig::load("config/hosts.json").await {
             if let Some(opc_srv) = infra.opc_servers.first() {
                 if let Some(ep) = &opc_srv.opc_endpoint {
                     if !ep.is_empty() {
                         target_endpoint = ep.clone();
                     }
                 }
             }
        }

        println!("🔄 HMI Worker: Connecting to {}...", target_endpoint);
        broadcast_sys(&tx, "PlcConnection", 0);

        let (user_token_policy, identity_token) = match settings.auth.mode {
            AuthMode::Username => {
                let username = settings.auth.username.clone().unwrap_or_default();
                let password = settings.auth.password.clone().unwrap_or_default();
                (
                    UserTokenPolicy {
                        token_type: opcua::types::UserTokenType::UserName,
                        ..Default::default()
                    },
                    IdentityToken::UserName(username, password.into()),
                )
            }
            AuthMode::X509 => (
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
                SecurityPolicy::Basic256Sha256.to_str(),
                MessageSecurityMode::SignAndEncrypt,
                user_token_policy,
            ),
            identity_token,
        ).await {
            Ok((s, handle)) => {
                let _ = handle.spawn();
                s
            },
            Err(e) => {
                println!("❌ Connection Error: {}. Retrying in 5s...", e);
                broadcast_sys(&tx, "PlcConnection", 0);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        println!("✅ Connected via OPC UA!");
        broadcast_sys(&tx, "PlcConnection", 1);
        
        tokio::time::sleep(Duration::from_secs(1)).await;

        // --- SUBSCRIBE & COLLECT TAGS ---
        let ns = settings.opcua.namespace_index;
        let mut all_monitored_nodes: Vec<NodeId> = Vec::new(); 
        // Discover nodes from remote OPC management API only.
        if let Ok(infra) = InfrastructureConfig::load("config/hosts.json").await {
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