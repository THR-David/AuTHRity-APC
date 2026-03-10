use std::fs;
use std::path::Path;
use std::time::Duration;

use axum::{
    extract::{State, Json, Path as AxumPath},
    routing::{get, post, put},
    Router,
    http::StatusCode,
    response::IntoResponse,
};
use log::{info, error};
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use crate::config;
use crate::models::{AppState, NodesFile};
use crate::bridges::opcua;

const BUILTIN_ANONYMOUS_TOKEN_ID: &str = "ANONYMOUS";

/// Create the Axum router with all API endpoints
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/api/nodes", get(list_opc_models).post(hot_reload_nodes))
        .route("/api/nodes/:id", get(get_opc_model_content))
    .route("/api/admin/restart", post(restart_opc_server))
        .route("/api/security/config", get(get_security_config).put(update_security_config))
        .route("/api/security/tokens", get(list_user_tokens).post(upsert_user_token))
        .route("/api/security/tokens/:id", put(upsert_user_token_with_id).delete(delete_user_token))
        .with_state(state)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SecurityEndpointConfig {
    id: String,
    path: String,
    security_policy: String,
    security_mode: String,
    security_level: u16,
    user_token_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SecurityConfigPayload {
    default_endpoint: String,
    endpoints: Vec<SecurityEndpointConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UserTokenSummary {
    id: String,
    user: String,
    has_password: bool,
    x509: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UpsertUserTokenRequest {
    id: Option<String>,
    user: String,
    pass: Option<String>,
    x509: Option<String>,
}

fn slugify_token_id(input: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = false;
    for ch in input.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }

    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "token".to_string()
    } else {
        out
    }
}

fn resolve_token_id(token_map: &Mapping, requested_id: Option<&str>, username: &str) -> String {
    let requested = requested_id.unwrap_or("").trim();
    if !requested.is_empty() {
        return requested.to_string();
    }

    let base = slugify_token_id(username);
    let mut candidate = base.clone();
    let mut suffix = 2u32;
    while token_map.contains_key(key(&candidate)) {
        candidate = format!("{}_{}", base, suffix);
        suffix += 1;
    }
    candidate
}

fn load_server_config_value() -> Result<Value, (StatusCode, String)> {
    let content = fs::read_to_string(config::SERVER_CONF)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read server config: {}", e)))?;
    serde_yaml::from_str::<Value>(&content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Invalid server config YAML: {}", e)))
}

fn save_server_config_value(value: &Value) -> Result<(), (StatusCode, String)> {
    let serialized = serde_yaml::to_string(value)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serialize server config: {}", e)))?;
    fs::write(config::SERVER_CONF, serialized)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write server config: {}", e)))
}

fn key(name: &str) -> Value {
    Value::String(name.to_string())
}

fn get_root_mapping_mut(doc: &mut Value) -> Result<&mut Mapping, (StatusCode, String)> {
    doc.as_mapping_mut()
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "Server config root must be a YAML map".to_string()))
}

fn get_root_mapping(doc: &Value) -> Result<&Mapping, (StatusCode, String)> {
    doc.as_mapping()
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "Server config root must be a YAML map".to_string()))
}

fn get_security_payload(doc: &Value) -> Result<SecurityConfigPayload, (StatusCode, String)> {
    let root = get_root_mapping(doc)?;
    let default_endpoint = root
        .get(key("default_endpoint"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let endpoints_map = root
        .get(key("endpoints"))
        .and_then(Value::as_mapping)
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "Missing endpoints map in server config".to_string()))?;

    let mut endpoints = Vec::new();
    for (id_value, endpoint_value) in endpoints_map {
        let Some(id) = id_value.as_str() else { continue; };
        let Some(endpoint_map) = endpoint_value.as_mapping() else { continue; };

        let user_token_ids = endpoint_map
            .get(key("user_token_ids"))
            .and_then(Value::as_sequence)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        endpoints.push(SecurityEndpointConfig {
            id: id.to_string(),
            path: endpoint_map
                .get(key("path"))
                .and_then(Value::as_str)
                .unwrap_or("/")
                .to_string(),
            security_policy: endpoint_map
                .get(key("security_policy"))
                .and_then(Value::as_str)
                .unwrap_or("None")
                .to_string(),
            security_mode: endpoint_map
                .get(key("security_mode"))
                .and_then(Value::as_str)
                .unwrap_or("None")
                .to_string(),
            security_level: endpoint_map
                .get(key("security_level"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
            user_token_ids,
        });
    }

    Ok(SecurityConfigPayload {
        default_endpoint,
        endpoints,
    })
}

fn set_security_payload(doc: &mut Value, payload: &SecurityConfigPayload) -> Result<(), (StatusCode, String)> {
    if payload.endpoints.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "At least one endpoint is required".to_string()));
    }
    if !payload.endpoints.iter().any(|ep| ep.id == payload.default_endpoint) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("default_endpoint '{}' is not present in endpoints", payload.default_endpoint),
        ));
    }

    let root = get_root_mapping_mut(doc)?;
    root.insert(key("default_endpoint"), Value::String(payload.default_endpoint.clone()));

    let mut endpoints_map = Mapping::new();
    for endpoint in &payload.endpoints {
        let mut endpoint_map = Mapping::new();
        endpoint_map.insert(key("path"), Value::String(endpoint.path.clone()));
        endpoint_map.insert(key("security_policy"), Value::String(endpoint.security_policy.clone()));
        endpoint_map.insert(key("security_mode"), Value::String(endpoint.security_mode.clone()));
        endpoint_map.insert(key("security_level"), Value::Number((endpoint.security_level as u64).into()));
        endpoint_map.insert(
            key("user_token_ids"),
            Value::Sequence(
                endpoint
                    .user_token_ids
                    .iter()
                    .map(|token_id| Value::String(token_id.clone()))
                    .collect(),
            ),
        );
        endpoints_map.insert(Value::String(endpoint.id.clone()), Value::Mapping(endpoint_map));
    }

    root.insert(key("endpoints"), Value::Mapping(endpoints_map));
    Ok(())
}

fn get_tokens(doc: &Value) -> Vec<UserTokenSummary> {
    let mut out = Vec::new();
    out.push(UserTokenSummary {
        id: BUILTIN_ANONYMOUS_TOKEN_ID.to_string(),
        user: "(anonymous)".to_string(),
        has_password: false,
        x509: None,
    });

    if let Some(root) = doc.as_mapping() {
        if let Some(token_map) = root.get(key("user_tokens")).and_then(Value::as_mapping) {
            for (id_value, token_value) in token_map {
                let Some(id) = id_value.as_str() else { continue; };
                let Some(entry) = token_value.as_mapping() else { continue; };
                let user = entry.get(key("user")).and_then(Value::as_str).unwrap_or("").to_string();
                let has_password = entry.get(key("pass")).and_then(Value::as_str).is_some();
                let x509 = entry
                    .get(key("x509"))
                    .and_then(Value::as_str)
                    .map(|v| v.to_string());

                out.push(UserTokenSummary {
                    id: id.to_string(),
                    user,
                    has_password,
                    x509,
                });
            }
        }
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
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

async fn get_security_config() -> impl IntoResponse {
    match load_server_config_value().and_then(|doc| get_security_payload(&doc)) {
        Ok(payload) => Json(payload).into_response(),
        Err((status, msg)) => (status, msg).into_response(),
    }
}

async fn update_security_config(Json(payload): Json<SecurityConfigPayload>) -> impl IntoResponse {
    match load_server_config_value() {
        Ok(mut doc) => {
            if let Err((status, msg)) = set_security_payload(&mut doc, &payload) {
                return (status, msg).into_response();
            }
            match save_server_config_value(&doc) {
                Ok(_) => (StatusCode::OK, "Security config updated. Restart OPC UA server to apply endpoint changes.").into_response(),
                Err((status, msg)) => (status, msg).into_response(),
            }
        }
        Err((status, msg)) => (status, msg).into_response(),
    }
}

async fn list_user_tokens() -> impl IntoResponse {
    match load_server_config_value() {
        Ok(doc) => Json(get_tokens(&doc)).into_response(),
        Err((status, msg)) => (status, msg).into_response(),
    }
}

async fn upsert_user_token(Json(payload): Json<UpsertUserTokenRequest>) -> impl IntoResponse {
    upsert_user_token_inner(payload.id.clone(), payload)
}

async fn upsert_user_token_with_id(
    AxumPath(id): AxumPath<String>,
    Json(payload): Json<UpsertUserTokenRequest>,
) -> impl IntoResponse {
    upsert_user_token_inner(Some(id), payload)
}

fn upsert_user_token_inner(id: Option<String>, payload: UpsertUserTokenRequest) -> axum::response::Response {
    if payload.user.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "Token user is required").into_response();
    }

    let requested_id = id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .or_else(|| payload.id.as_deref().map(str::trim).filter(|v| !v.is_empty()));

    if let Some(id_candidate) = requested_id {
        if id_candidate.eq_ignore_ascii_case(BUILTIN_ANONYMOUS_TOKEN_ID) {
            return (
                StatusCode::BAD_REQUEST,
                "ANONYMOUS is a built-in token policy id and does not need to be created or edited",
            )
                .into_response();
        }
    }

    if payload.pass.as_ref().is_none() && payload.x509.as_ref().is_none() {
        return (
            StatusCode::BAD_REQUEST,
            "At least one credential is required: pass or x509",
        )
            .into_response();
    }

    let mut doc = match load_server_config_value() {
        Ok(doc) => doc,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    let root = match get_root_mapping_mut(&mut doc) {
        Ok(root) => root,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    let token_map_value = root
        .entry(key("user_tokens"))
        .or_insert_with(|| Value::Mapping(Mapping::new()));

    let Some(token_map) = token_map_value.as_mapping_mut() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "user_tokens must be a YAML map").into_response();
    };

    let final_id = resolve_token_id(token_map, requested_id, payload.user.trim());

    let mut token_entry = Mapping::new();
    token_entry.insert(key("user"), Value::String(payload.user.trim().to_string()));
    if let Some(pass) = payload.pass {
        if !pass.trim().is_empty() {
            token_entry.insert(key("pass"), Value::String(pass));
        }
    }
    if let Some(x509) = payload.x509 {
        if !x509.trim().is_empty() {
            token_entry.insert(key("x509"), Value::String(x509));
        }
    }

    token_map.insert(Value::String(final_id.clone()), Value::Mapping(token_entry));

    match save_server_config_value(&doc) {
        Ok(_) => (
            StatusCode::OK,
            format!("Token '{}' saved. Restart OPC UA server to apply token changes.", final_id),
        )
            .into_response(),
        Err((status, msg)) => (status, msg).into_response(),
    }
}

async fn delete_user_token(AxumPath(id): AxumPath<String>) -> impl IntoResponse {
    if id.eq_ignore_ascii_case(BUILTIN_ANONYMOUS_TOKEN_ID) {
        return (
            StatusCode::BAD_REQUEST,
            "ANONYMOUS is a built-in token policy id and cannot be removed",
        )
            .into_response();
    }

    let mut doc = match load_server_config_value() {
        Ok(doc) => doc,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    let root = match get_root_mapping_mut(&mut doc) {
        Ok(root) => root,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    if let Some(tokens) = root.get_mut(key("user_tokens")).and_then(Value::as_mapping_mut) {
        tokens.remove(key(&id));
    }

    if let Some(endpoints) = root.get_mut(key("endpoints")).and_then(Value::as_mapping_mut) {
        for endpoint in endpoints.values_mut() {
            if let Some(endpoint_map) = endpoint.as_mapping_mut() {
                if let Some(user_token_ids) = endpoint_map.get_mut(key("user_token_ids")).and_then(Value::as_sequence_mut) {
                    user_token_ids.retain(|token_id| token_id.as_str() != Some(id.as_str()));
                }
            }
        }
    }

    match save_server_config_value(&doc) {
        Ok(_) => (StatusCode::OK, "Token removed. Restart OPC UA server to apply token changes.").into_response(),
        Err((status, msg)) => (status, msg).into_response(),
    }
}

async fn restart_opc_server() -> impl IntoResponse {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to resolve current executable: {}", e)).into_response();
        }
    };
    let args: Vec<String> = std::env::args().skip(1).collect();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;

        let spawn_result = std::process::Command::new(&exe)
            .args(&args)
            .spawn();

        if let Err(err) = spawn_result {
            error!("❌ Failed to spawn OPC UA server restart process: {}", err);
            return;
        }

        info!("♻️ OPC UA server restart requested. Exiting current process.");
        std::process::exit(0);
    });

    (
        StatusCode::ACCEPTED,
        "OPC UA server restart requested. Connections will drop briefly.",
    )
        .into_response()
}
