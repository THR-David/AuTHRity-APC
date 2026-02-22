// src/state.rs
use tokio::sync::{broadcast, mpsc};
use axum::extract::FromRef;
use crate::opc_worker::{OpcUpdate, OpcCommand};
use crate::config::HistorianConfig;
use std::sync::Arc;
use crate::auth::AuthService;

#[derive(Clone)]
pub struct AppState {
    // We only need the Sender. When a user connects via WebSocket, 
    // we call .subscribe() on this to get their own Receiver.
    pub tx: broadcast::Sender<OpcUpdate>,
    pub cmd_tx: mpsc::Sender<OpcCommand>,
    // You can add other global state here later (e.g., Database connection)
    pub settings: Arc<crate::config::AppSettings>,
    pub historian_config: HistorianConfig,
    pub auth: Arc<AuthService>,
}

impl FromRef<AppState> for Arc<AuthService> {
    fn from_ref(input: &AppState) -> Self {
        input.auth.clone()
    }
}