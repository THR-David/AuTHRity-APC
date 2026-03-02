use axum::{
    extract::{Path, State, Request},
    routing::{get, post},
    Json, Router,
    http::StatusCode,
    response::IntoResponse,
    middleware::{self, Next},
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path as StdPath, PathBuf},
    sync::{Arc, Mutex},
    process::{Command, Stdio},
    fs::File,
};
use tokio::fs;
use tracing::{info, error, warn};
use serde_json::json;
use chrono::Local;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// Directory to store model files
    #[arg(short, long, default_value = "./models")]
    model_dir: String,

    /// Path to apc_engine binary
    #[arg(long)]
    engine_bin: Option<String>,

    /// API Key for authentication
    #[arg(long, env = "HOST_API_KEY", default_value = "secret-deploy-key")]
    api_key: String,

    /// PKI Directory (Authentication)
    #[arg(long, default_value = "./pki")]
    pki_dir: String,

    /// Path to persisted OPC client defaults env file
    #[arg(long, default_value = "./config/opc_client.env")]
    opc_defaults_path: String,
}

#[derive(Clone)]
struct AppState {
    model_dir: PathBuf,
    engine_bin: PathBuf,
    pki_dir: PathBuf,
    opc_defaults_path: PathBuf,
    api_key: String,
    opc_client_defaults: Arc<Mutex<OpcClientDefaults>>,
    // Map of ModelID -> ProcessHandle (Simulator for now, in real life need Child handle management)
    processes: Arc<Mutex<HashMap<String, ProcessStatus>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpcClientDefaults {
    opc_url: String,
    auth_mode: String,
    username: Option<String>,
    password: Option<String>,
    security_policy: String,
    message_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum ProcessState {
    Stopped,
    Running(u32), // PID
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProcessStatus {
    id: String,
    active_model: Option<String>,
    state: ProcessState,
    last_error: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    // Determine Engine Binary Path
    let engine_bin = if let Some(path) = args.engine_bin {
        PathBuf::from(path)
    } else {
        // Smart fallback: Check CWD first (Production), then Dev path
        let exe_name = format!("authrity-apc-engine{}", std::env::consts::EXE_SUFFIX);
        let cwd_engine = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join(&exe_name);
            
        if cwd_engine.exists() {
            info!("Found local engine binary: {:?}", cwd_engine);
            cwd_engine
        } else {
            // Fallback for development
            let dev_path = PathBuf::from("../apc_engine/target/debug").join(&exe_name);
            warn!("Local engine not found. Using dev path: {:?}", dev_path);
            dev_path
        }
    };

    if !engine_bin.exists() {
        error!("❌ APC Engine binary not found at: {:?}. Please invoke with --engine-bin", engine_bin);
        // We continue in case the user plans to upload it later? Or just crash?
        // Better to crash or warn heavily.
    }

    // Ensure model dir exists
    let model_dir = PathBuf::from(&args.model_dir);
    if !model_dir.exists() {
        fs::create_dir_all(&model_dir).await.expect("Failed to create model dir");
    }

    // Determine PKI Directory
    let pki_dir = PathBuf::from(&args.pki_dir);
    let final_pki_dir = if pki_dir.exists() {
        pki_dir
    } else {
        // Fallback for development
        let dev_pki = PathBuf::from("../apc_engine/pki");
        if dev_pki.exists() {
            info!("Found dev PKI: {:?}", dev_pki);
            dev_pki
        } else {
            pki_dir // Keep default
        }
    };
    
    // Canonicalize to absolute path to ensure child process finds it regardless of CWD
    let abs_pki = std::fs::canonicalize(&final_pki_dir).unwrap_or(final_pki_dir);
    let opc_defaults_path = PathBuf::from(&args.opc_defaults_path);
    let initial_defaults = load_opc_client_defaults(&opc_defaults_path).unwrap_or_else(|| OpcClientDefaults {
        opc_url: "opc.tcp://127.0.0.1:4855".to_string(),
        auth_mode: "username".to_string(),
        username: None,
        password: None,
        security_policy: "Basic256Sha256".to_string(),
        message_mode: "SignAndEncrypt".to_string(),
    });

    let shared_state = AppState {
        model_dir,
        engine_bin,
        pki_dir: abs_pki,
        opc_defaults_path,
        api_key: args.api_key.clone(),
        opc_client_defaults: Arc::new(Mutex::new(initial_defaults)),
        processes: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/api/controllers", get(list_controllers))
        .route("/api/opc-client/defaults", get(get_opc_client_defaults).post(set_opc_client_defaults))
        .route("/api/controllers/:id/start", post(start_controller))
        .route("/api/controllers/:id/stop", post(stop_controller))
        .route("/api/controllers/:id/config", get(get_model_config)) // <--- Added: Access to Single Source of Truth for Physics
        .route("/api/controllers/:id/models/:filename", get(get_model_file)) // <--- Added: Fetch specific model file
        .route("/api/deploy", post(deploy_model))
        .layer(middleware::from_fn_with_state(shared_state.clone(), auth_middleware))
        .route("/api/health", get(health_check)) // Health check bypasses auth
        .with_state(shared_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    info!("🚀 Supervisor listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> impl IntoResponse {
    let auth_header = req.headers().get("x-api-key");
    
    match auth_header {
        Some(header) if header == state.api_key.as_str() => {
            next.run(req).await
        }
        _ => {
            warn!("⛔ Unauthorized access attempt");
            (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response()
        }
    }
}

async fn health_check() -> &'static str {
    "OK"
}

// DTOs
#[derive(Serialize)]
struct ControllerSummary {
    id: String,
    models: Vec<String>,
    active_model: Option<String>,
    state: ProcessState,
}

/// List all available model folders and their running state
async fn list_controllers(State(state): State<AppState>) -> Json<Vec<ControllerSummary>> {
    let mut summaries = Vec::new();
    
    // Read root model directory
    if let Ok(mut entries) = fs::read_dir(&state.model_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            // Look for directories (Controller "Slots")
            if path.is_dir() {
                if let Some(folder_name) = path.file_name().and_then(|s| s.to_str()) {
                    let id = folder_name.to_string();
                    
                    // Scan inside for JSON models
                    let mut models = Vec::new();
                    if let Ok(mut sub_entries) = fs::read_dir(&path).await {
                        while let Ok(Some(sub)) = sub_entries.next_entry().await {
                             let sub_path = sub.path();
                             if sub_path.extension().and_then(|s| s.to_str()) == Some("json") {
                                if let Some(fname) = sub_path.file_name().and_then(|s| s.to_str()) {
                                    models.push(fname.to_string());
                                }
                             }
                        }
                    }

                    // Get process status
                    let processes = state.processes.lock().unwrap();
                    let (current_state, active_model) = if let Some(p) = processes.get(&id) {
                        (p.state.clone(), p.active_model.clone())
                    } else {
                        (ProcessState::Stopped, None)
                    };

                    summaries.push(ControllerSummary {
                        id,
                        models,
                        active_model,
                        state: current_state,
                    });
                }
            }
        }
    }
    
    Json(summaries)
}

#[derive(Deserialize)]
struct StartRequest {
    model_filename: String, // REQUIRED now
    opc_url: Option<String>,
    security_policy: Option<String>,
    message_mode: Option<String>,
    auth_mode: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Deserialize)]
struct SetOpcClientDefaultsRequest {
    opc_url: Option<String>,
    security_policy: Option<String>,
    message_mode: Option<String>,
    auth_mode: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Serialize)]
struct OpcClientDefaultsResponse {
    opc_url: String,
    security_policy: String,
    message_mode: String,
    auth_mode: String,
    username: Option<String>,
}

fn validate_auth_payload(auth_mode: &str, username: &Option<String>, password: &Option<String>) -> Result<(), String> {
    let auth_mode_normalized = auth_mode.to_ascii_lowercase();
    if auth_mode_normalized != "username" && auth_mode_normalized != "x509" && auth_mode_normalized != "anonymous" {
        return Err("auth_mode must be 'username', 'x509', or 'anonymous'".to_string());
    }

    if auth_mode_normalized == "username" {
        let user_ok = username.as_deref().map(str::trim).is_some_and(|u| !u.is_empty());
        let pass_ok = password.as_deref().map(str::trim).is_some_and(|p| !p.is_empty());
        if !user_ok || !pass_ok {
            return Err("username auth_mode requires non-empty username and password".to_string());
        }
    }

    Ok(())
}

async fn get_opc_client_defaults(State(state): State<AppState>) -> impl IntoResponse {
    let defaults = state.opc_client_defaults.lock().unwrap().clone();
    Json(OpcClientDefaultsResponse {
        opc_url: defaults.opc_url,
        security_policy: defaults.security_policy,
        message_mode: defaults.message_mode,
        auth_mode: defaults.auth_mode,
        username: defaults.username,
    })
}

async fn set_opc_client_defaults(
    State(state): State<AppState>,
    Json(payload): Json<SetOpcClientDefaultsRequest>,
) -> impl IntoResponse {
    let mut defaults = state.opc_client_defaults.lock().unwrap();

    let next_auth_mode = payload
        .auth_mode
        .clone()
        .unwrap_or_else(|| defaults.auth_mode.clone());
    let next_username = payload.username.clone().or_else(|| defaults.username.clone());
    let next_password = payload.password.clone().or_else(|| defaults.password.clone());

    if let Err(msg) = validate_auth_payload(&next_auth_mode, &next_username, &next_password) {
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }

    if let Some(opc_url) = payload.opc_url {
        if !opc_url.trim().is_empty() {
            defaults.opc_url = opc_url;
        }
    }
    if let Some(policy) = payload.security_policy {
        if !policy.trim().is_empty() {
            defaults.security_policy = policy;
        }
    }
    if let Some(mode) = payload.message_mode {
        if !mode.trim().is_empty() {
            defaults.message_mode = mode;
        }
    }

    defaults.auth_mode = next_auth_mode;
    defaults.username = next_username;
    defaults.password = next_password;

    let snapshot = defaults.clone();
    drop(defaults);

    if let Err(err) = persist_opc_client_defaults(&state.opc_defaults_path, &snapshot) {
        error!("Failed to persist controller_host OPC defaults: {}", err);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to persist OPC defaults: {}", err),
        )
            .into_response();
    }

    info!("Updated controller_host OPC client defaults");
    StatusCode::OK.into_response()
}

async fn start_controller(
    State(state): State<AppState>,
    Path(id): Path<String>, // 'id' is the folder name
    Json(payload): Json<StartRequest>,
) ->  impl IntoResponse {
    // Construct path: models/{id}/{filename}
    let model_path = state.model_dir.join(&id).join(&payload.model_filename);
    
    if !model_path.exists() {
        return (StatusCode::NOT_FOUND, format!("Model file not found inside {}", id)).into_response();
    }

    let processes = state.processes.lock().unwrap();
    
    // Check if THIS specific controller slot is running
    if let Some(status) = processes.get(&id) {
        if matches!(status.state, ProcessState::Running(_)) {
             return (StatusCode::CONFLICT, "Controller is already running").into_response();
        }
    }
    drop(processes);

    info!("Starting controller {} with model {}...", id, payload.model_filename);
    
    // Determine OPC client settings (payload overrides controller_host defaults)
    let mut defaults = state.opc_client_defaults.lock().unwrap().clone();
    if let Some(opc_url) = payload.opc_url.clone() {
        if !opc_url.trim().is_empty() {
            defaults.opc_url = opc_url;
        }
    }
    if let Some(policy) = payload.security_policy.clone() {
        if !policy.trim().is_empty() {
            defaults.security_policy = policy;
        }
    }
    if let Some(mode) = payload.message_mode.clone() {
        if !mode.trim().is_empty() {
            defaults.message_mode = mode;
        }
    }
    if let Some(auth_mode) = payload.auth_mode.clone() {
        defaults.auth_mode = auth_mode;
    }
    if payload.username.is_some() {
        defaults.username = payload.username.clone();
    }
    if payload.password.is_some() {
        defaults.password = payload.password.clone();
    }

    if let Err(msg) = validate_auth_payload(&defaults.auth_mode, &defaults.username, &defaults.password) {
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }

    let opc_url = defaults.opc_url.clone();
    let auth_mode = defaults.auth_mode.clone();
    let security_policy = defaults.security_policy.clone();
    let message_mode = defaults.message_mode.clone();
    let username = defaults.username.clone();
    let password = defaults.password.clone();
    let auth_mode_normalized = auth_mode.to_ascii_lowercase();

    // Persist resolved settings as the new default for subsequent starts
    {
        let mut default_guard = state.opc_client_defaults.lock().unwrap();
        *default_guard = defaults;
        if let Err(err) = persist_opc_client_defaults(&state.opc_defaults_path, &default_guard) {
            warn!("Failed to persist resolved controller defaults after start: {}", err);
        }
    }

    info!("   -> Targeting OPC Server: {}", opc_url);
    info!("   -> OPC Auth Mode: {}", auth_mode);
    info!("   -> OPC Security Policy: {}", security_policy);
    info!("   -> OPC Message Mode: {}", message_mode);
    info!("   -> Using PKI: {:?}", state.pki_dir);

    // Prepare Log File
    let log_dir = PathBuf::from("./logs");
    if !log_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
             error!("Failed to create log dir: {}", e);
             return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create log dir").into_response();
        }
    }

    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let log_filename = format!("{}_{}.log", id, timestamp);
    let log_path = log_dir.join(&log_filename);
    
    info!("   -> Logging to: {:?}", log_path);

    let log_file = match File::create(&log_path) {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to create log file: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create log file").into_response();
        }
    };
    
    // Clone file handle for stderr (stdout and stderr go to same file)
    let log_file_err = match log_file.try_clone() {
        Ok(f) => f,
        Err(e) => {
             error!("Failed to clone log file handle: {}", e);
             return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to setup logging").into_response();
        }
    };

    // Spawn Process
    let mut command = Command::new(&state.engine_bin);
    command
        .arg("--model")
        .arg(&model_path)
        .arg("--opc")
        .arg(&opc_url)
        .arg("--pki")
        .arg(&state.pki_dir)
        .arg("--auth-mode")
        .arg(&auth_mode)
        .arg("--security-policy")
        .arg(&security_policy)
        .arg("--message-mode")
        .arg(&message_mode)
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err));

    let child_res = if auth_mode_normalized == "username" {
        command
            .arg("--username")
            .arg(username.as_deref().unwrap_or_default())
            .arg("--password")
            .arg(password.as_deref().unwrap_or_default())
            .spawn()
    } else {
        command.spawn()
    };

    match child_res {
        Ok(mut child) => {
            match child.try_wait() {
                Ok(Some(status)) => {
                    error!(
                        "Controller process exited immediately with status {}. See log: {:?}",
                        status,
                        log_path
                    );
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!(
                            "Controller failed to start (exit: {}). Check log: {}",
                            status,
                            log_path.display()
                        ),
                    )
                        .into_response();
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("Could not query child process status after spawn: {}", e);
                }
            }

            let pid = child.id();
            info!("Started controller {} with PID {}", id, pid);
            let mut processes = state.processes.lock().unwrap();
            processes.insert(id.clone(), ProcessStatus {
                id,
                active_model: Some(payload.model_filename),
                state: ProcessState::Running(pid),
                last_error: None,
            });
            (StatusCode::OK, Json(json!({"pid": pid}))).into_response()
        }
        Err(e) => {
            error!("Failed to spawn: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

fn load_opc_client_defaults(path: &PathBuf) -> Option<OpcClientDefaults> {
    let content = std::fs::read_to_string(path).ok()?;

    if content.trim_start().starts_with('{') {
        return serde_json::from_str::<OpcClientDefaults>(&content).ok();
    }

    let mut values: HashMap<String, String> = HashMap::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    let opc_url = values.get("OPC_URL")?.clone();
    let auth_mode = values.get("AUTH_MODE")?.clone();
    let security_policy = values.get("SECURITY_POLICY")?.clone();
    let message_mode = values.get("MESSAGE_MODE")?.clone();

    let username = values
        .get("USERNAME")
        .filter(|value| !value.trim().is_empty())
        .cloned();
    let password = values
        .get("PASSWORD")
        .filter(|value| !value.trim().is_empty())
        .cloned();

    Some(OpcClientDefaults {
        opc_url,
        auth_mode,
        username,
        password,
        security_policy,
        message_mode,
    })
}

fn persist_opc_client_defaults(path: &PathBuf, defaults: &OpcClientDefaults) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create parent dir failed: {}", e))?;
    }

    let sanitize = |value: &str| value.replace('\n', "").replace('\r', "");
    let mut payload = String::new();
    payload.push_str("# Controller Host OPC client defaults\n");
    payload.push_str(&format!("OPC_URL={}\n", sanitize(&defaults.opc_url)));
    payload.push_str(&format!("AUTH_MODE={}\n", sanitize(&defaults.auth_mode)));
    payload.push_str(&format!("SECURITY_POLICY={}\n", sanitize(&defaults.security_policy)));
    payload.push_str(&format!("MESSAGE_MODE={}\n", sanitize(&defaults.message_mode)));
    payload.push_str(&format!(
        "USERNAME={}\n",
        defaults
            .username
            .as_deref()
            .map(sanitize)
            .unwrap_or_default()
    ));
    payload.push_str(&format!(
        "PASSWORD={}\n",
        defaults
            .password
            .as_deref()
            .map(sanitize)
            .unwrap_or_default()
    ));

    std::fs::write(path, payload).map_err(|e| format!("write defaults failed: {}", e))?;
    Ok(())
}

async fn stop_controller(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut processes = state.processes.lock().unwrap();

    if let Some(status) = processes.get(&id) {
        if let ProcessState::Running(pid) = status.state {
            info!("Stopping controller {} (PID {})...", id, pid);
            
            // On Windows this kills the process
            #[cfg(target_os = "windows")]
            let _ = Command::new("taskkill").args(["/F", "/PID", &pid.to_string()]).output();
            
            #[cfg(not(target_os = "windows"))]
            let _ = Command::new("kill").arg(pid.to_string()).output();
            
            processes.insert(id.clone(), ProcessStatus {
                id,
                active_model: None,
                state: ProcessState::Stopped,
                last_error: None,
            });
            return (StatusCode::OK, "Stopped").into_response();
        }
    }

    (StatusCode::NOT_FOUND, "Not running").into_response()
}

// Multipart upload handler for /deploy
use axum::extract::Multipart;
use std::io::Write;

fn next_available_filename(folder_path: &StdPath, requested_filename: &str) -> String {
    let requested_path = StdPath::new(requested_filename);
    let stem = requested_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model");
    let ext = requested_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();

    let clean_stem = if stem.is_empty() { "model" } else { stem };
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let initial_candidate = format!("{}_{}{}", clean_stem, timestamp, ext);

    if !folder_path.join(&initial_candidate).exists() {
        return initial_candidate;
    }

    let mut idx: u32 = 1;
    loop {
        let candidate = format!("{}_{}_{}{}", clean_stem, timestamp, idx, ext);
        if !folder_path.join(&candidate).exists() {
            return candidate;
        }
        idx += 1;
    }
}

async fn deploy_model(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap().to_string();
        
        if name == "file" {
            let filename = field
                .file_name()
                .and_then(|f| StdPath::new(f).file_name().and_then(|n| n.to_str()))
                .unwrap_or("model.json")
                .to_string();
            // Read all bytes into memory to parse metadata
            let data = match field.bytes().await {
                Ok(b) => b,
                Err(e) => return (StatusCode::BAD_REQUEST, format!("Failed to read upload: {}", e)).into_response(),
            };
            
            // Extract Folder Name from JSON metadata
            let folder_name = match serde_json::from_slice::<serde_json::Value>(&data) {
                Ok(json) => {
                    json.get("metadata")
                        .and_then(|m| m.get("name"))
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "Unknown".to_string())
                },
                Err(_) => "Unknown".to_string(), // Keep valid behavior for non-json files if any
            };

            // Sanitize
            let safe_folder = folder_name.chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect::<String>();
            
            let final_folder = if safe_folder.is_empty() { "Unknown".to_string() } else { safe_folder };
            
            let folder_path = state.model_dir.join(&final_folder);
            
            // Ensure folder exists
            if !folder_path.exists() {
                if let Err(e) = std::fs::create_dir_all(&folder_path) {
                    error!("Failed to create model folder: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create model directory").into_response();
                }
            }
            
            let safe_filename = filename
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                .collect::<String>();
            let requested_filename = if safe_filename.is_empty() {
                "model.json".to_string()
            } else {
                safe_filename
            };

            let final_filename = next_available_filename(&folder_path, &requested_filename);
            let dest_path = folder_path.join(&final_filename);
            info!("Deploying model to {:?}", dest_path);
            
            if let Ok(mut file) = std::fs::File::create(dest_path) {
                if let Err(e) = file.write_all(&data) {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
                }
            } else {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create file").into_response();
            }

            return (StatusCode::OK, format!("Deployed as {}", final_filename)).into_response();
        }
    }
    
    (StatusCode::OK, "Deployed").into_response()
}

// Single Source of Truth: Retrieve Physics Configuration
async fn get_model_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Sanitize ID (prevent directory traversal)
    let safe_id = std::path::Path::new(&id)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
        
    let model_path = state.model_dir.join(format!("{}.json", safe_id));
    
    if model_path.exists() {
        match fs::read_to_string(model_path).await {
            Ok(content) => (StatusCode::OK, content).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, "Model config not found").into_response()
    }
}
// Get specific model file from controller's models directory
async fn get_model_file(
    State(state): State<AppState>,
    Path((controller_id, model_filename)): Path<(String, String)>,
) -> impl IntoResponse {
    // Sanitize inputs (prevent directory traversal)
    let safe_controller_id = std::path::Path::new(&controller_id)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let safe_filename = std::path::Path::new(&model_filename)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    
    // Build path: models/{controller_id}/{model_filename}
    let model_path = state.model_dir.join(&*safe_controller_id).join(&*safe_filename);
    
    println!("🔍 Looking for model file: {}", model_path.display());
    
    if model_path.exists() {
        match fs::read_to_string(&model_path).await {
            Ok(content) => {
                println!("✅ Serving model file: {}", model_path.display());
                (StatusCode::OK, content).into_response()
            },
            Err(e) => {
                println!("⚠️ Failed to read model file: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
        }
    } else {
        println!("⚠️ Model file not found: {}", model_path.display());
        (StatusCode::NOT_FOUND, format!("Model file not found: {}", safe_filename)).into_response()
    }
}