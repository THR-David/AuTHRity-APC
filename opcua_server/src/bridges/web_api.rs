use std::fs;
use std::path::Path;

use axum::{
    extract::{State, Json, Path as AxumPath},
    routing::get,
    Router,
    http::StatusCode,
    response::IntoResponse,
};
use log::{info, error};

use crate::config;
use crate::models::{AppState, NodesFile};
use crate::bridges::opcua;

/// Create the Axum router with all API endpoints
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/api/nodes", get(list_opc_models).post(hot_reload_nodes))
        .route("/api/nodes/:id", get(get_opc_model_content))
        .with_state(state)
}

/// List all available YAML models in the server's models directory
async fn list_opc_models() -> impl IntoResponse {
    let mut models = Vec::new();
    if let Ok(entries) = fs::read_dir(config::MODELS_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    models.push(stem.to_string());
                }
            }
        }
    }
    Json(models)
}

/// Retrieve the raw YAML content of a specific model
async fn get_opc_model_content(
    AxumPath(model_id): AxumPath<String>,
) -> impl IntoResponse {
    // Sanitize input to prevent directory traversal
    let clean_id = Path::new(&model_id).file_name().unwrap_or_default().to_string_lossy();
    let path = format!("{}/{}.yaml", config::MODELS_DIR, clean_id);
    
    match fs::read_to_string(&path) {
        Ok(content) => (StatusCode::OK, content).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Model not found").into_response(),
    }
}

/// Hot-reload nodes from YAML submitted by the HMI
async fn hot_reload_nodes(
    State(state): State<AppState>,
    body: String, // Accept raw YAML string
) -> impl IntoResponse {
    info!("🔥 Hot Reload: Receiving new node configuration...");
    
    // We assume the model name is the "root" key in the YAML or passed via header? 
    // For simplicity, we parse the nodes and look for the first Object to define the "Root".
    // Better yet: The HMI should probably tell us the model name, but let's infer it from the first NodeId.
    
    // Parse YAML to check validity first
    let nodes_file: Result<NodesFile, _> = serde_yaml::from_str(&body);
    match nodes_file {
        Ok(file) => {
            // Infer basic name from first node (usually "ModelName:ControlNodes")
            let model_name = if let Some(first) = file.nodes.first() {
                first.node_id.split(':').next().unwrap_or("UnknownModel").to_string()
            } else {
                return (StatusCode::BAD_REQUEST, "Empty node list").into_response();
            };

            info!("🔥 Hot Reloading Model: {}", model_name);

            // 1. Persist to Disk (Persistence Fix)
            let file_path = format!("{}/{}.yaml", config::MODELS_DIR, model_name);
            match fs::write(&file_path, &body) {
                Ok(_) => info!("💾 Saved model configuration to {}", file_path),
                Err(e) => error!("❌ Failed to save persistence file {}: {}", file_path, e),
            }
            
            // 2. Inject Nodes
            match opcua::inject_nodes(state.namespace, state.node_manager, file, &model_name) {
                Ok(_) => (StatusCode::OK, format!("Deployed & Saved {}", model_name)).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e)).into_response(),
    }
}
