use std::sync::Arc;
use std::fs;
use std::path::Path;

use opcua::nodes::AccessLevel;
use log::warn;
use opcua::server::address_space::Variable;
use opcua::server::diagnostics::NamespaceMetadata;
use opcua::server::node_manager::memory::{simple_node_manager, InMemoryNodeManager, SimpleNodeManagerImpl};
use opcua::server::{Server, ServerBuilder, ServerHandle};
use opcua::types::{BuildInfo, DateTime, NodeId, UAString, Variant, LocalizedText};
use serde_yaml::Value as YamlValue;

use crate::config;
use crate::models::NodesFile;

/// Build and configure the OPC UA server
pub fn build_opcua_server() -> (Server, ServerHandle) {
    ServerBuilder::new()
        .with_config_from(config::SERVER_CONF)
        .build_info(BuildInfo {
            product_uri: "urn:ModelPredictiveControlServer".into(),
            manufacturer_name: "David Thor".into(),
            product_name: "Rust OPC-UA".into(),
            software_version: "0.1.0".into(),
            build_number: "1".into(),
            build_date: DateTime::now(),
        })
        .with_node_manager(simple_node_manager(
            NamespaceMetadata { 
                namespace_uri: config::NAMESPACE_URI.to_owned(), 
                ..Default::default() 
            },
            "mpc_manager", 
        ))
        .trust_client_certs(true)
        .diagnostics_enabled(true)
        .build()
        .unwrap()
}

/// Load all YAML model files from the models directory
pub fn load_initial_models(
    ns: u16,
    node_manager: Arc<InMemoryNodeManager<SimpleNodeManagerImpl>>,
) -> usize {
    let mut models_count = 0;

    if let Err(e) = fs::create_dir_all(config::MODELS_DIR) {
        warn!("Could not create directory '{}': {}", config::MODELS_DIR, e);
    }

    if let Ok(entries) = fs::read_dir(config::MODELS_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                let model_name = path.file_stem().unwrap().to_str().unwrap();
                println!("--- Loading Model: {} ---", model_name);
                
                match add_nodes_from_file(ns, node_manager.clone(), &path, model_name) {
                    Ok(_) => {
                        println!("Successfully loaded: {}", model_name);
                        models_count += 1;
                    },
                    Err(e) => warn!("Failed to load model {}: {}", model_name, e),
                }
            }
        }
    }

    models_count
}

/// Load nodes from a YAML file and inject into the address space
pub fn add_nodes_from_file(
    ns: u16,
    manager: Arc<InMemoryNodeManager<SimpleNodeManagerImpl>>,
    path: &Path,
    model_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let s = fs::read_to_string(path)?;
    let nodes_file: NodesFile = serde_yaml::from_str(&s)?;
    inject_nodes(ns, manager, nodes_file, model_name)
}

/// Inject nodes into the OPC UA address space
pub fn inject_nodes(
    ns: u16,
    manager: Arc<InMemoryNodeManager<SimpleNodeManagerImpl>>,
    nodes_file: NodesFile,
    model_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {

    let address_space = manager.address_space();
    let mut address_space = address_space.write();

    // 1. Root Folder for the Model (e.g., "Model5")
    let model_root_id = NodeId::new(ns, model_name.to_string());
    
    // Check if it exists? If strictly "Hot Reloading" means "Update", we might want to clear old nodes?
    // For now, we purely Add/Update. Existing nodes receive new values/defs.
    
    if address_space.find_node(&model_root_id).is_none() {
        address_space.add_folder(&model_root_id, model_name, model_name, &NodeId::objects_folder_id());
        println!("  📁 Created root: ns={};s={}", ns, model_name);
    }

    // This variable tracks where the NEXT node should be placed.
    // It defaults to the model root.
    let mut current_parent_id = model_root_id.clone();
    let mut node_count = 0;

    for spec in nodes_file.nodes {
        // CLEAN NODE ID: Use the YAML string directly. 
        // This prevents the "ns=2;s=model:ns=2;s=tag" nesting error.
        let node_id = NodeId::new(ns, spec.node_id.clone());

        let browse = spec.browse_name.clone().unwrap_or_else(|| spec.node_id.clone());
        let display = spec.display_name.clone().unwrap_or_else(|| browse.clone());
        let class = spec.node_class.unwrap_or_else(|| "Variable".to_string());

        match class.as_str() {
            "Object" => {
                // Objects are always placed in the Model Root (top level of the model)
                address_space.add_folder(&node_id, &browse, &display, &model_root_id);
                
                // Set description attribute if provided
                if let Some(desc) = &spec.description {
                    if let Some(node) = address_space.find_node_mut(&node_id) {
                        node.as_mut_node().set_description(LocalizedText::new("en", desc));
                    }
                }
                
                // UPDATE PARENT: Subsequent variables will now fall into THIS object
                current_parent_id = node_id.clone();
                let desc_info = spec.description.as_deref().unwrap_or("");
                println!("  📁 Object: ns={};s={} [{}]", ns, spec.node_id, desc_info);
                node_count += 1;
            }
            "Variable" | _ => {
                let dt_str = spec.data_type.unwrap_or_else(|| "Double".to_string());
                let is_array = spec.value_rank.unwrap_or(0) == 1;
                let array_len = spec.array_dimensions.as_ref().and_then(|v| v.get(0)).cloned().unwrap_or(0);

                let initial_variant: Variant = match dt_str.to_lowercase().as_str() {
                    "double" | "f64" => {
                        let val = spec.initial_value.as_ref().and_then(|v| v.as_f64()).unwrap_or(0.0);
                        if is_array { vec![val; array_len].into() } else { val.into() }
                    }
                    "doublearray" => {
                        // Parse array from initial_value if it's an array, otherwise empty
                        if let Some(YamlValue::Sequence(seq)) = &spec.initial_value {
                            let vals: Vec<f64> = seq.iter()
                                .filter_map(|v| v.as_f64())
                                .collect();
                            // If empty, use placeholder element for type inference
                            if vals.is_empty() {
                                vec![0.0f64].into()
                            } else {
                                Variant::from(vals)
                            }
                        } else {
                            // Create DoubleArray with single zero element (will be replaced)
                            vec![0.0f64].into()
                        }
                    }
                    "int32" | "i32" => {
                        let val = spec.initial_value.as_ref().and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        if is_array { vec![val; array_len].into() } else { val.into() }
                    }
                    "string" => {
                        let val = spec.initial_value.as_ref().and_then(|v| v.as_str()).unwrap_or("");
                        let ua_str = UAString::from(val);
                        if is_array { vec![ua_str; array_len].into() } else { ua_str.into() }
                    }
                    "stringarray" => {
                        // Parse array from initial_value if it's an array, otherwise empty
                        if let Some(YamlValue::Sequence(seq)) = &spec.initial_value {
                            let vals: Vec<UAString> = seq.iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| UAString::from(s))
                                .collect();
                            // If empty, use placeholder element for type inference
                            if vals.is_empty() {
                                vec![UAString::from("")].into()
                            } else {
                                Variant::from(vals)
                            }
                        } else {
                            // Create StringArray with single empty element (will be replaced)
                            vec![UAString::from("")].into()
                        }
                    }
                    "boolean" | "bool" => {
                        let val = spec.initial_value.as_ref().and_then(|v| v.as_bool()).unwrap_or(false);
                        val.into()
                    }
                    _ => 0.0f64.into(),
                };

                let mut var = Variable::new(&node_id, &browse, &display, initial_variant);
                var.set_writable(true);
                var.set_user_access_level(AccessLevel::CURRENT_READ | AccessLevel::CURRENT_WRITE);
                
                // ADD TO CURRENT PARENT: This places PV/SP inside the TIC101 folder
                let _ = address_space.add_variables(vec![var], &current_parent_id);
                
                // Set description attribute after adding to address space
                if let Some(desc) = &spec.description {
                    if let Some(node) = address_space.find_node_mut(&node_id) {
                        node.as_mut_node().set_description(LocalizedText::new("en", desc));
                    }
                }
                
                let desc_info = spec.description.as_deref().unwrap_or("");
                let unit_info = spec.engineering_units.as_deref().unwrap_or("");
                let info = if !desc_info.is_empty() || !unit_info.is_empty() {
                    format!(" [{}{}]", desc_info, if !unit_info.is_empty() { format!(" {}", unit_info) } else { String::new() })
                } else {
                    String::new()
                };
                println!("  ✅ Variable: ns={};s={} ({}){}", ns, spec.node_id, dt_str, info);
                node_count += 1;
            }
        }
    }
    
    println!("  ✅ Total nodes created: {}", node_count);
    Ok(())
}
