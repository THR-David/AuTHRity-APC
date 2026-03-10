mod config;
mod auth;
mod opc_worker;
mod state;
mod web_routes;
mod historian;
mod infrastructure; // Added module

use anyhow::Result;
use axum::{routing::{get, post, put}, Router};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc}; // Import mpsc
use tower_http::services::ServeDir;
use tower_sessions::{cookie::SameSite, Expiry, MemoryStore, SessionManagerLayer};
use time::Duration;
use web_routes::{get_model_config, list_models, get_trends_data};
use crate::config::AppSettings;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load Settings
    println!("⚙️ Loading settings...");
    let settings = AppSettings::load("config/settings.toml")?;
    
    // 2. Create Channels
    // A. Broadcast Channel (One-to-Many): Sends OPC updates to all connected browsers
    let (tx, _rx) = broadcast::channel(5000);

    // B. Command Channel (Many-to-One): Sends commands from Web to OPC Worker
    let (cmd_tx, cmd_rx) = mpsc::channel(100); 

    // 3. Spawn OPC Worker (Background Task)
    let opc_settings = settings.clone();
    let opc_tx = tx.clone();
    
    tokio::spawn(async move {
        println!("🏭 Starting OPC UA Worker...");
        // Now passing 'cmd_rx' to the worker so it can listen for commands
        if let Err(e) = opc_worker::run_opc_worker(opc_settings, cmd_rx, opc_tx).await {
            eprintln!("❌ OPC Worker Failed: {}", e);
        }
    });

    // 3b. Spawn Historian (Background Task)
    let historian_config = settings.historian.clone();
    let historian_rx = tx.subscribe();
    let historian_cmd_tx = cmd_tx.clone();
    
    tokio::spawn(async move {
        if let Err(e) = historian::run_historian(historian_config, historian_rx, historian_cmd_tx).await {
            eprintln!("❌ Historian Failed: {}", e);
        }
    });

    // 4. Shared State for Axum
    let auth_service = auth::AuthService::new(&settings.hmi_auth.database_url).await?;
    let _ = auth_service
        .ensure_bootstrap_admin("admin", "password")
        .await;

    infrastructure::InfrastructureConfig::seed_if_empty(
        &settings.hmi_auth.database_url,
        "config/hosts.json",
    )
    .await?;

    // We store 'cmd_tx' here so the web routes can send commands
    let app_state = AppState {
        tx,
        cmd_tx, // <--- Added this to state
        settings: Arc::new(settings.clone()),
        historian_config: settings.historian.clone(),
        auth: auth::shared(auth_service),
    };

    let session_store = MemoryStore::default();
    let inactivity_minutes = settings.hmi_auth.session_inactivity_minutes.max(5) as i64;
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(true)
        .with_same_site(SameSite::Strict)
        .with_expiry(Expiry::OnInactivity(Duration::minutes(inactivity_minutes)));

    // 5. Frontend Location
    let assets_dir = PathBuf::from("frontend/dist");
    if !assets_dir.exists() {
        println!("⚠️  Frontend folder '{:?}' not found. Serving 404s.", assets_dir);
    }

    // 6. Define Routes
    let app = Router::new()
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/change-password", post(auth::change_password))
        .route("/api/admin/audit", get(web_routes::list_audit_admin))
        .route("/api/admin/users", get(web_routes::list_users_admin).post(web_routes::create_user_admin))
        .route("/api/admin/users/:id/role", post(web_routes::update_user_role_admin))
        .route("/api/admin/users/:id/disable", post(web_routes::set_user_disabled_admin))
        .route("/api/admin/users/:id/password", post(web_routes::reset_user_password_admin))
        .route("/ws", get(web_routes::ws_handler))
        .route("/api/models", get(list_models))
        .route("/api/model", get(get_model_config))
        .route("/api/controller/model", get(web_routes::get_controller_model)) // <--- Get Active Model
        .route("/api/controller/:controller_id/models/:model_filename", get(web_routes::get_controller_model_file)) // <--- Get Specific Model File
        .route("/api/trends", get(get_trends_data))
        .route("/api/stepresponse/data", get(web_routes::get_step_response_data))
        .route("/api/stepresponse/tags", get(web_routes::get_available_tags_api))
        .route("/api/stepresponse/calculate", post(web_routes::calculate_step_response))
        .route("/api/deploy", post(web_routes::deploy_bundle)) // <--- Atomic Deployment Route
        .route("/api/infrastructure", get(web_routes::get_infrastructure).post(web_routes::save_infrastructure)) // <--- Infrastructure Registry
        .route("/api/infrastructure/controller-host-clients", post(web_routes::save_controller_host_clients))
        .route("/api/infrastructure/opc-reconnect", post(web_routes::reconnect_opc_worker))
        .route("/api/prox/opc/security/config", get(web_routes::get_opc_security_config_proxy).put(web_routes::save_opc_security_config_proxy))
        .route("/api/prox/opc/security/tokens", get(web_routes::list_opc_user_tokens_proxy).post(web_routes::upsert_opc_user_token_proxy))
        .route("/api/prox/opc/security/tokens/:id", put(web_routes::upsert_opc_user_token_by_id_proxy).delete(web_routes::delete_opc_user_token_proxy))
        .route("/api/prox/opc/restart", post(web_routes::restart_opc_server_proxy))
        .route("/api/physics/:id", get(web_routes::get_remote_physics)) // <--- Fetch JSON from Supervisor
        // NEW: Controller Management Proxy
        .route("/api/prox/controllers", get(web_routes::list_controllers_proxy))
        .route("/api/prox/controllers/:id/start", post(web_routes::start_controller_proxy))
        .route("/api/prox/controllers/:id/stop", post(web_routes::stop_controller_proxy))
        .route("/api/prox/controllers/:id/logs/tail", get(web_routes::get_controller_logs_proxy))
        .nest_service("/", ServeDir::new(assets_dir))
        .with_state(app_state)
        .layer(session_layer);

    // 7. Start HTTP Server
    let port = 3000;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    println!("🚀 HMI Server running at http://localhost:{}", port);
    axum::serve(listener, app).await?;

    Ok(())
}