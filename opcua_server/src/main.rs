use std::net::SocketAddr;

use opcua::server::node_manager::memory::SimpleNodeManager;

mod config;
mod models;
mod bridges;

use models::AppState;

#[tokio::main]
async fn main() {
    env_logger::init();

    // Load configuration
    let cfg = config::Config::load();

    // Build OPC UA server
    let (server, handle) = bridges::opcua::build_opcua_server();
    let node_manager = handle.node_managers().get_of_type::<SimpleNodeManager>().unwrap();
    let ns = handle.get_namespace_index(config::NAMESPACE_URI).unwrap();
    
    // Load initial models from disk
    let models_count = bridges::opcua::load_initial_models(ns, node_manager.clone());
    println!("---------------------------------------");
    println!("Initialization Complete: {} Models Loaded.", models_count);
    
    // Create shared state for API
    let shared_state = AppState {
        node_manager: node_manager.clone(),
        namespace: ns,
    };

    // Create and spawn the hot-reload API server
    let app = bridges::web_api::create_router(shared_state);
    let api_addr = SocketAddr::from(([0, 0, 0, 0], cfg.api.port));
    println!("🚀 Hot-Reload API listening on http://{}", api_addr);

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(api_addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    println!("Server is now running and waiting for connections...");
    println!("---------------------------------------");

    // Spawn shutdown handler
    let handle_c = handle.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        handle_c.cancel();
    });

    // Run OPC UA server
    server.run().await.unwrap();
}