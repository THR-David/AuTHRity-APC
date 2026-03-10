use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State, Query, Multipart, Path as AxumPath},
    response::IntoResponse,
    Json,
    http::StatusCode,
};
use crate::state::AppState;
use crate::opc_worker::{NodesFile, OpcCommand}; 
use futures::{sink::SinkExt, stream::StreamExt}; 
use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::path::Path;
use std::collections::HashSet;
use crate::infrastructure::InfrastructureConfig; // Added Import
use crate::auth::{
    permission_audit_label, required_runtime_write_permission, CsrfVerified, CurrentUser, Permission, Role,
};

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: Role,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub role: Role,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct SetDisabledRequest {
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<i64>,
    pub action: Option<String>,
    pub result: Option<String>,
    pub actor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ControllerModelQuery {
    pub controller_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveControllerHostClientsRequest {
    pub supervisors: Vec<crate::infrastructure::ServiceConfig>,
    pub controller_host_clients: Vec<crate::infrastructure::ControllerHostClientSettings>,
}

#[derive(Debug, Deserialize)]
pub struct OpcServerSelectorQuery {
    pub opc_server_id: Option<String>,
}

fn normalize_service_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn validate_unique_opc_servers(infra: &InfrastructureConfig) -> Result<(), String> {
    let mut seen_admin_urls: HashSet<String> = HashSet::new();
    let mut seen_opc_endpoints: HashSet<String> = HashSet::new();

    for opc in &infra.opc_servers {
        let admin_url = normalize_service_url(&opc.url);
        if admin_url.is_empty() {
            return Err(format!("OPC server '{}' has an empty admin URL", opc.name));
        }
        if !seen_admin_urls.insert(admin_url.clone()) {
            return Err(format!("Duplicate OPC server admin URL is not allowed: {}", opc.url));
        }

        if let Some(endpoint) = opc.opc_endpoint.as_ref() {
            let endpoint_norm = normalize_service_url(endpoint);
            if !endpoint_norm.is_empty() && !seen_opc_endpoints.insert(endpoint_norm) {
                return Err(format!("Duplicate OPC endpoint is not allowed: {}", endpoint));
            }
        }
    }

    Ok(())
}

pub async fn list_audit_admin(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<AuditQuery>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::AuditRead) {
        return e.into_response();
    }

    let limit = query.limit.unwrap_or(200);
    match state
        .auth
        .list_audit_events(
            limit,
            query.action.as_deref(),
            query.result.as_deref(),
            query.actor.as_deref(),
        )
        .await
    {
        Ok(events) => Json(events).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn list_users_admin(
    State(state): State<AppState>,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::UserManage) {
        return e.into_response();
    }

    match state.auth.list_users().await {
        Ok(users) => Json(users).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn create_user_admin(
    State(state): State<AppState>,
    _: CsrfVerified,
    user: CurrentUser,
    Json(payload): Json<CreateUserRequest>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::UserManage) {
        return e.into_response();
    }

    match state
        .auth
        .create_user(payload.username.trim(), &payload.password, payload.role)
        .await
    {
        Ok(_) => {
            state
                .auth
                .audit(Some(&user), "user.create", Some(payload.username.trim()), "success", None)
                .await;
            StatusCode::CREATED.into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn update_user_role_admin(
    State(state): State<AppState>,
    _: CsrfVerified,
    user: CurrentUser,
    AxumPath(id): AxumPath<i64>,
    Json(payload): Json<UpdateRoleRequest>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::UserManage) {
        return e.into_response();
    }

    match state.auth.update_user_role(id, payload.role).await {
        Ok(_) => {
            state
                .auth
                .audit(Some(&user), "user.update_role", Some(&id.to_string()), "success", None)
                .await;
            StatusCode::OK.into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn set_user_disabled_admin(
    State(state): State<AppState>,
    _: CsrfVerified,
    user: CurrentUser,
    AxumPath(id): AxumPath<i64>,
    Json(payload): Json<SetDisabledRequest>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::UserManage) {
        return e.into_response();
    }

    if id == user.id && payload.disabled {
        return (StatusCode::BAD_REQUEST, "Cannot disable current user").into_response();
    }

    match state.auth.set_user_disabled(id, payload.disabled).await {
        Ok(_) => {
            state
                .auth
                .audit(Some(&user), "user.set_disabled", Some(&id.to_string()), "success", Some(if payload.disabled { "disabled" } else { "enabled" }))
                .await;
            StatusCode::OK.into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn reset_user_password_admin(
    State(state): State<AppState>,
    _: CsrfVerified,
    user: CurrentUser,
    AxumPath(id): AxumPath<i64>,
    Json(payload): Json<ResetPasswordRequest>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::UserManage) {
        return e.into_response();
    }

    match state.auth.reset_user_password(id, &payload.new_password).await {
        Ok(_) => {
            state
                .auth
                .audit(Some(&user), "user.reset_password", Some(&id.to_string()), "success", None)
                .await;
            StatusCode::OK.into_response()
        }
        Err(e) => e.into_response(),
    }
}

// --- INFRASTRUCTURE HANDLERS ---
pub async fn get_infrastructure(
    State(state): State<AppState>,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::InfraWrite) {
        return e.into_response();
    }

    let config = match InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        Ok(config) => config,
        Err(_) => {
            let fallback = InfrastructureConfig::load("config/hosts.json").await.unwrap_or_default();
            if let Err(e) = fallback.save_to_db(&state.settings.hmi_auth.database_url).await {
                return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to initialize infrastructure settings: {}", e)).into_response();
            }
            fallback
        }
    };

    state
        .auth
        .audit(Some(&user), "infra.read", Some("sqlite:infra_settings"), "success", None)
        .await;

    Json(config.redacted_for_api()).into_response()
}

pub async fn save_infrastructure(
    State(state): State<AppState>,
    _: CsrfVerified,
    user: CurrentUser,
    Json(payload): Json<InfrastructureConfig>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::InfraWrite) {
        return e.into_response();
    }

    if let Err(detail) = validate_unique_opc_servers(&payload) {
        return (StatusCode::BAD_REQUEST, detail).into_response();
    }

    let existing = InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url)
        .await
        .ok();
    let merged_payload = payload.with_secret_refs(existing.as_ref());

    match merged_payload.save_to_db(&state.settings.hmi_auth.database_url).await {
        Ok(_) => {
            if let Err(sync_err) = sync_controller_host_opc_defaults(
                &merged_payload,
                &state.settings.services.supervisor_api_key,
            )
            .await
            {
                state
                    .auth
                    .audit(
                        Some(&user),
                        "infra.write",
                        Some("sqlite:infra_settings"),
                        "partial",
                        Some(&format!("saved but sync failed: {}", sync_err)),
                    )
                    .await;
                return (
                    StatusCode::BAD_GATEWAY,
                    format!("Infrastructure saved, but Controller Host sync failed: {}", sync_err),
                )
                    .into_response();
            }

            state
                .auth
                .audit(Some(&user), "infra.write", Some("sqlite:infra_settings"), "success", None)
                .await;
            StatusCode::OK.into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save infrastructure to DB: {}", e)).into_response(),
    }
}

pub async fn save_controller_host_clients(
    State(state): State<AppState>,
    _: CsrfVerified,
    user: CurrentUser,
    Json(payload): Json<SaveControllerHostClientsRequest>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::InfraWrite) {
        return e.into_response();
    }

    let mut config = match InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        Ok(current) => current,
        Err(_) => InfrastructureConfig::load("config/hosts.json").await.unwrap_or_default(),
    };

    config.supervisors = payload.supervisors;
    config.controller_host_clients = payload.controller_host_clients;

    let existing = InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url)
        .await
        .ok();
    let merged_payload = config.with_secret_refs(existing.as_ref());

    match merged_payload.save_to_db(&state.settings.hmi_auth.database_url).await {
        Ok(_) => {
            if let Err(sync_err) = sync_controller_host_opc_defaults(
                &merged_payload,
                &state.settings.services.supervisor_api_key,
            )
            .await
            {
                state
                    .auth
                    .audit(
                        Some(&user),
                        "infra.controller_host_clients.write",
                        Some("sqlite:infra_settings"),
                        "partial",
                        Some(&format!("saved but sync failed: {}", sync_err)),
                    )
                    .await;
                return (
                    StatusCode::BAD_GATEWAY,
                    format!("Controller Host clients saved, but sync failed: {}", sync_err),
                )
                    .into_response();
            }

            state
                .auth
                .audit(
                    Some(&user),
                    "infra.controller_host_clients.write",
                    Some("sqlite:infra_settings"),
                    "success",
                    None,
                )
                .await;

            StatusCode::OK.into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save controller host clients to DB: {}", e),
        )
            .into_response(),
    }
}

async fn sync_controller_host_opc_defaults(
    infra: &InfrastructureConfig,
    api_key: &str,
) -> std::result::Result<(), String> {
    let client = Client::new();
    let mut failures: Vec<String> = Vec::new();

    let opc_url = infra
        .opc_servers
        .first()
        .and_then(|opc| opc.opc_endpoint.clone())
        .filter(|value| !value.trim().is_empty());

    for supervisor in &infra.supervisors {
        let Some(host_client) = infra
            .controller_host_clients
            .iter()
            .find(|candidate| candidate.supervisor_id == supervisor.id)
        else {
            continue;
        };

        let (username, password) = infra.resolve_controller_host_username_password(&supervisor.id);
        let url = format!("{}/api/opc-client/defaults", supervisor.url.trim_end_matches('/'));

        let body = serde_json::json!({
            "opc_url": opc_url.clone(),
            "security_policy": host_client.security_policy,
            "message_mode": host_client.security_mode,
            "auth_mode": normalize_auth_mode_payload(&host_client.auth_mode),
            "username": username,
            "password": password,
        });

        match client
            .post(&url)
            .header("x-api-key", api_key)
            .json(&body)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => {
                let status = response.status();
                let detail = response.text().await.unwrap_or_else(|_| "no response body".to_string());
                failures.push(format!("{} -> {}: {}", supervisor.name, status, detail));
            }
            Err(err) => failures.push(format!("{} -> request error: {}", supervisor.name, err)),
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

pub async fn reconnect_opc_worker(
    State(state): State<AppState>,
    _: CsrfVerified,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::InfraWrite) {
        return e.into_response();
    }

    match state.cmd_tx.send(OpcCommand::Reconnect).await {
        Ok(_) => {
            state
                .auth
                .audit(Some(&user), "infra.opc_reconnect", Some("opc_worker"), "success", None)
                .await;
            StatusCode::OK.into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to request OPC reconnect: {}", e)).into_response(),
    }
}

fn select_opc_admin_url(infra: &InfrastructureConfig, opc_server_id: Option<&str>) -> Option<String> {
    let selected = if let Some(id) = opc_server_id.map(str::trim).filter(|id| !id.is_empty()) {
        infra.opc_servers.iter().find(|opc| opc.id == id)
    } else {
        infra.opc_servers.first()
    };

    selected
        .map(|opc| opc.url.trim().trim_end_matches('/').to_string())
        .filter(|url| !url.is_empty())
}

pub async fn get_opc_security_config_proxy(
    State(state): State<AppState>,
    Query(query): Query<OpcServerSelectorQuery>,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::InfraWrite) {
        return e.into_response();
    }

    let infra = match InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to load infrastructure config: {}", e))
                .into_response();
        }
    };

    let Some(opc_admin_url) = select_opc_admin_url(&infra, query.opc_server_id.as_deref()) else {
        return (StatusCode::NOT_FOUND, "No OPC server configured in infrastructure settings").into_response();
    };

    let client = Client::new();
    let url = format!("{}/api/security/config", opc_admin_url);
    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(body) => (status, body).into_response(),
                Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed to read OPC response: {}", e)).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("OPC server unreachable: {}", e)).into_response(),
    }
}

pub async fn save_opc_security_config_proxy(
    State(state): State<AppState>,
    Query(query): Query<OpcServerSelectorQuery>,
    _: CsrfVerified,
    user: CurrentUser,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::InfraWrite) {
        return e.into_response();
    }

    let infra = match InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to load infrastructure config: {}", e))
                .into_response();
        }
    };

    let Some(opc_admin_url) = select_opc_admin_url(&infra, query.opc_server_id.as_deref()) else {
        return (StatusCode::NOT_FOUND, "No OPC server configured in infrastructure settings").into_response();
    };

    let client = Client::new();
    let url = format!("{}/api/security/config", opc_admin_url);
    match client.put(&url).json(&payload).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                state
                    .auth
                    .audit(Some(&user), "infra.opc_security.write", Some(&opc_admin_url), "success", None)
                    .await;
            }
            match resp.text().await {
                Ok(body) => (status, body).into_response(),
                Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed to read OPC response: {}", e)).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("OPC server unreachable: {}", e)).into_response(),
    }
}

pub async fn list_opc_user_tokens_proxy(
    State(state): State<AppState>,
    Query(query): Query<OpcServerSelectorQuery>,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::InfraWrite) {
        return e.into_response();
    }

    let infra = match InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to load infrastructure config: {}", e))
                .into_response();
        }
    };

    let Some(opc_admin_url) = select_opc_admin_url(&infra, query.opc_server_id.as_deref()) else {
        return (StatusCode::NOT_FOUND, "No OPC server configured in infrastructure settings").into_response();
    };

    let client = Client::new();
    let url = format!("{}/api/security/tokens", opc_admin_url);
    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(body) => (status, body).into_response(),
                Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed to read OPC response: {}", e)).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("OPC server unreachable: {}", e)).into_response(),
    }
}

pub async fn upsert_opc_user_token_proxy(
    State(state): State<AppState>,
    Query(query): Query<OpcServerSelectorQuery>,
    _: CsrfVerified,
    user: CurrentUser,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::InfraWrite) {
        return e.into_response();
    }

    let infra = match InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to load infrastructure config: {}", e))
                .into_response();
        }
    };

    let Some(opc_admin_url) = select_opc_admin_url(&infra, query.opc_server_id.as_deref()) else {
        return (StatusCode::NOT_FOUND, "No OPC server configured in infrastructure settings").into_response();
    };

    let client = Client::new();
    let url = format!("{}/api/security/tokens", opc_admin_url);
    match client.post(&url).json(&payload).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                let target = payload.get("id").and_then(|v| v.as_str()).unwrap_or("<generated>");
                state
                    .auth
                    .audit(Some(&user), "infra.opc_token.upsert", Some(target), "success", None)
                    .await;
            }
            match resp.text().await {
                Ok(body) => (status, body).into_response(),
                Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed to read OPC response: {}", e)).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("OPC server unreachable: {}", e)).into_response(),
    }
}

pub async fn upsert_opc_user_token_by_id_proxy(
    AxumPath(id): AxumPath<String>,
    State(state): State<AppState>,
    Query(query): Query<OpcServerSelectorQuery>,
    _: CsrfVerified,
    user: CurrentUser,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::InfraWrite) {
        return e.into_response();
    }

    let infra = match InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to load infrastructure config: {}", e))
                .into_response();
        }
    };

    let Some(opc_admin_url) = select_opc_admin_url(&infra, query.opc_server_id.as_deref()) else {
        return (StatusCode::NOT_FOUND, "No OPC server configured in infrastructure settings").into_response();
    };

    let client = Client::new();
    let url = format!("{}/api/security/tokens/{}", opc_admin_url, id);
    match client.put(&url).json(&payload).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                state
                    .auth
                    .audit(Some(&user), "infra.opc_token.upsert", Some(&id), "success", None)
                    .await;
            }
            match resp.text().await {
                Ok(body) => (status, body).into_response(),
                Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed to read OPC response: {}", e)).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("OPC server unreachable: {}", e)).into_response(),
    }
}

pub async fn delete_opc_user_token_proxy(
    AxumPath(id): AxumPath<String>,
    State(state): State<AppState>,
    Query(query): Query<OpcServerSelectorQuery>,
    _: CsrfVerified,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::InfraWrite) {
        return e.into_response();
    }

    let infra = match InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to load infrastructure config: {}", e))
                .into_response();
        }
    };

    let Some(opc_admin_url) = select_opc_admin_url(&infra, query.opc_server_id.as_deref()) else {
        return (StatusCode::NOT_FOUND, "No OPC server configured in infrastructure settings").into_response();
    };

    let client = Client::new();
    let url = format!("{}/api/security/tokens/{}", opc_admin_url, id);
    match client.delete(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                state
                    .auth
                    .audit(Some(&user), "infra.opc_token.delete", Some(&id), "success", None)
                    .await;
            }
            match resp.text().await {
                Ok(body) => (status, body).into_response(),
                Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed to read OPC response: {}", e)).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("OPC server unreachable: {}", e)).into_response(),
    }
}

pub async fn restart_opc_server_proxy(
    State(state): State<AppState>,
    Query(query): Query<OpcServerSelectorQuery>,
    _: CsrfVerified,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::InfraWrite) {
        return e.into_response();
    }

    let infra = match InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to load infrastructure config: {}", e))
                .into_response();
        }
    };

    let Some(opc_admin_url) = select_opc_admin_url(&infra, query.opc_server_id.as_deref()) else {
        return (StatusCode::NOT_FOUND, "No OPC server configured in infrastructure settings").into_response();
    };

    let client = Client::new();
    let url = format!("{}/api/admin/restart", opc_admin_url);
    match client.post(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                state
                    .auth
                    .audit(Some(&user), "infra.opc_server.restart", Some(&opc_admin_url), "success", None)
                    .await;
            }
            match resp.text().await {
                Ok(body) => (status, body).into_response(),
                Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed to read OPC response: {}", e)).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("OPC server unreachable: {}", e)).into_response(),
    }
}

// --- DEPLOYMENT HANDLER ---
pub async fn deploy_bundle(
    State(state): State<AppState>,
    _: CsrfVerified,
    user: CurrentUser,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ModelDeploy) {
        return e.into_response();
    }

    println!("📦 HMI: Receiving Deployment Bundle...");
    
    let mut json_content: Option<Vec<u8>> = None;
    let mut yaml_content: Option<Vec<u8>> = None;
    let mut model_name = "Unknown".to_string();
    
    // Default URLs from config
    let mut target_supervisor = state.settings.services.supervisor_url.clone();
    let mut target_opc = state.settings.services.opc_hot_reload_url.clone();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap().to_string();
        
        if name == "model_json" {
            if let Some(fname) = field.file_name() {
                // If model name is unknown, define it from JSON filename
                if model_name == "Unknown" {
                    model_name = Path::new(fname).file_stem().unwrap().to_string_lossy().to_string();
                }
            }
            json_content = Some(field.bytes().await.unwrap().to_vec());
        } else if name == "nodes_yaml" {
            // Also try to get model name from YAML if JSON wasn't provided first
            if let Some(fname) = field.file_name() {
                 if model_name == "Unknown" {
                    model_name = Path::new(fname).file_stem().unwrap().to_string_lossy().to_string();
                }
            }
            yaml_content = Some(field.bytes().await.unwrap().to_vec());
        } else if name == "target_supervisor" {
             let val = field.text().await.unwrap();
             if !val.trim().is_empty() { target_supervisor = val; }
        } else if name == "target_opc" {
             let val = field.text().await.unwrap();
             if !val.trim().is_empty() { target_opc = val; }
        }
    }

    if json_content.is_none() && yaml_content.is_none() {
        return (StatusCode::BAD_REQUEST, "Must provide at least model_json OR nodes_yaml").into_response();
    }

    let client = Client::new();
    println!("🚀 Processing Deployment for '{}'...", model_name);

    // 1. Push JSON to Supervisor (controller_host)
    if let Some(json_bytes) = json_content {
        println!("   -> Deploying JSON to Supervisor: {}", target_supervisor);
        let json_part = reqwest::multipart::Part::bytes(json_bytes)
            .file_name(format!("{}.json", model_name))
            .mime_str("application/json").unwrap();
            
        let supervisor_form = reqwest::multipart::Form::new().part("file", json_part);
        let deploy_url = format!("{}/api/deploy", target_supervisor);

        match client.post(&deploy_url)
            .header("x-api-key", &state.settings.services.supervisor_api_key)
            .multipart(supervisor_form)
            .send()
            .await 
        {
            Ok(res) if res.status().is_success() => println!("      ✅ Supervisor Deployment Success"),
            Ok(res) => return (StatusCode::BAD_GATEWAY, format!("Supervisor rejected: {}", res.status())).into_response(),
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("Supervisor unreachable: {}", e)).into_response(),
        }
    }

    // 2. Push YAML to Connectivity (opcua_server)
    if let Some(yaml_bytes) = yaml_content.clone() {
        println!("   -> Hot-Swapping OPC UA Nodes on: {}", target_opc);
        let hot_reload_url = format!("{}/api/nodes", target_opc);

        match client.post(&hot_reload_url)
            .body(yaml_bytes.clone()) 
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => println!("      ✅ OPC UA Hot-Reload Success"),
            Ok(res) => return (StatusCode::BAD_GATEWAY, format!("OPC Server rejected: {}", res.status())).into_response(),
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("OPC Server unreachable: {}", e)).into_response(),
        }
    }
    
    state
        .auth
        .audit(Some(&user), "model.deploy", Some(&model_name), "success", None)
        .await;

    (StatusCode::OK, format!("Partial/Full Deployment of '{}' Complete", model_name)).into_response()
}

// --- WEBSOCKET HANDLER ---
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ReadView) {
        return e.into_response();
    }

    ws.on_upgrade(|socket| handle_socket(socket, state, user))
}

async fn handle_socket(socket: WebSocket, state: AppState, user: CurrentUser) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    let cmd_tx = state.cmd_tx.clone();
    let auth = state.auth.clone();
    
    // 1. Receive Commands (Browser -> Rust)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if json["type"] == "WRITE" {
                        if let Some(node_id) = json["nodeId"].as_str() {
                            // Handle different value types
                            if let Some(val) = json["value"].as_f64() {
                                let required_permission = required_runtime_write_permission(node_id);
                                if user.require(required_permission).is_err() {
                                    let denied_detail = format!("requires {}", permission_audit_label(required_permission));
                                    auth.audit(Some(&user), "runtime.write", Some(node_id), "denied", Some(&denied_detail)).await;
                                    continue;
                                }
                                println!("📩 Web Command: Write number {} to {}", val, node_id);
                                let _ = cmd_tx.send(OpcCommand::WriteNumber(node_id.to_string(), val)).await;
                                auth.audit(Some(&user), "runtime.write", Some(node_id), "success", None).await;
                            } else if let Some(val) = json["value"].as_str() {
                                if user.require(Permission::RuntimeWriteBasic).is_err() {
                                    auth.audit(Some(&user), "runtime.write", Some(node_id), "denied", Some("requires runtime:write_basic")).await;
                                    continue;
                                }
                                println!("📩 Web Command: Write string '{}' to {}", val, node_id);
                                let _ = cmd_tx.send(OpcCommand::WriteString(node_id.to_string(), val.to_string())).await;
                                auth.audit(Some(&user), "runtime.write", Some(node_id), "success", None).await;
                            } else if let Some(val) = json["value"].as_bool() {
                                if user.require(Permission::RuntimeWriteBasic).is_err() {
                                    auth.audit(Some(&user), "runtime.write", Some(node_id), "denied", Some("requires runtime:write_basic")).await;
                                    continue;
                                }
                                println!("📩 Web Command: Write bool {} to {}", val, node_id);
                                let _ = cmd_tx.send(OpcCommand::WriteBool(node_id.to_string(), val)).await;
                                auth.audit(Some(&user), "runtime.write", Some(node_id), "success", None).await;
                            }
                        }
                    } else if json["type"] == "REFRESH" {
                        println!("🔄 Web Command: Client requested data refresh");
                        let _ = cmd_tx.send(OpcCommand::ReadAll).await;
                    }
                }
            }
        }
    });

    // 2. Send Updates (Rust -> Browser)
    let mut send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if let Ok(json_text) = serde_json::to_string(&msg) {
                        if sender.send(Message::Text(json_text)).await.is_err() { break; }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    tokio::select! { _ = &mut recv_task => {}, _ = &mut send_task => {} }
}

// --- LIST ALL YAML MODELS ---
pub async fn list_models(
    State(state): State<AppState>,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ReadView) {
        return e.into_response();
    }

    // Load registry to find primary OPC server and list models from remote source only.
    if let Ok(infra) = InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        if let Some(opc) = infra.opc_servers.first() {
            let client = Client::builder()
                .timeout(std::time::Duration::from_millis(500))
                .build()
                .unwrap_or_default();
            println!("🔍 Fetching models from OPC Server: {}", opc.url);
            
            match client.get(&format!("{}/api/nodes", opc.url)).send().await {
                Ok(resp) => {
                    if let Ok(models) = resp.json::<Vec<String>>().await {
                        return Json(models).into_response();
                    }
                    return (StatusCode::BAD_GATEWAY, "Failed to parse model list from OPC server").into_response();
                },
                Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed to fetch models from OPC server: {}", e)).into_response(),
            }
        }
    }

    (StatusCode::NOT_FOUND, "No OPC server configured in infrastructure settings").into_response()
}

// --- DYNAMIC CONFIG LOADER ---
#[derive(Deserialize)]
pub struct ModelParams {
    file: Option<String>,
}

pub async fn get_model_config(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<ModelParams>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ReadView) {
        return e.into_response();
    }

    let filename = match params.file {
        Some(value) if !value.trim().is_empty() => value,
        _ => return (StatusCode::BAD_REQUEST, "Missing required query parameter: file").into_response(),
    };
    
    // 1. Try fetching from Remote OPC Server
    if let Ok(infra) = InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        if let Some(opc) = infra.opc_servers.first() {
            let client = Client::builder()
                .timeout(std::time::Duration::from_millis(500))
                .build()
                .unwrap_or_default();
            let url = format!("{}/api/nodes/{}", opc.url, filename);
             println!("🔍 Fetching node map from OPC Server: {}", url);
            
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(content) = resp.text().await {
                        if let Ok(nodes_file) = serde_yaml::from_str::<NodesFile>(&content) {
                            return Json(nodes_file).into_response();
                        }
                    }
                },
                Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed to fetch node map from OPC server: {}", e)).into_response(),
                Ok(r) => return (StatusCode::BAD_GATEWAY, format!("Remote node map fetch failed: {}", r.status())).into_response(),
            }
        }
    }

    (StatusCode::NOT_FOUND, "No OPC server configured in infrastructure settings").into_response()
}

// --- FETCH PHYSICS FROM SUPERVISOR ---
pub async fn get_remote_physics(
    State(state): State<AppState>,
    user: CurrentUser,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ReadView) {
        return e.into_response();
    }

    // 1. Get primary supervisor from registry
    if let Ok(infra) = InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        if let Some(sup) = infra.supervisors.first() {
            let client = Client::new();
            let url = format!("{}/api/controllers/{}/config", sup.url, id);
            println!("🔍 Fetching Physics JSON from Supervisor: {}", url);
            
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(content) = resp.text().await {
                        // Return raw JSON (browser will parse it)
                        return (
                            [(axum::http::header::CONTENT_TYPE, "application/json")],
                            content
                        ).into_response();
                    }
                },
                Err(e) => println!("⚠️ Failed to fetch physics from remote Supervisor: {}", e),
                Ok(r) => println!("⚠️ Remote fetch failed: {}", r.status()),
            }
        }
    }
    
    (StatusCode::NOT_FOUND, "Physics config not found on supervisor").into_response()
}

// --- GET ACTIVE CONTROLLER MODEL ---
pub async fn get_controller_model(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<ControllerModelQuery>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ReadView) {
        return e.into_response();
    }

    // Get the active controller's model from supervisor
    let target_sup = state.settings.services.supervisor_url.clone();
    let api_key = state.settings.services.supervisor_api_key.clone();

    let client = Client::new();
    
    // First, list controllers to find a running target
    let list_url = format!("{}/api/controllers", target_sup);
    
    match client.get(&list_url).header("x-api-key", &api_key).send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(controllers) = resp.json::<Vec<serde_json::Value>>().await {
                let requested_id = query.controller_id.as_deref().map(str::trim).filter(|s| !s.is_empty());

                let is_running = |c: &&serde_json::Value| {
                    c.get("state").map(|s| s.is_object()).unwrap_or(false)
                };

                let target = if let Some(controller_id) = requested_id {
                    controllers.iter().find(|c| c.get("id").and_then(|i| i.as_str()) == Some(controller_id))
                } else {
                    controllers.iter().find(is_running)
                };

                let Some(controller) = target else {
                    if let Some(controller_id) = requested_id {
                        return (StatusCode::NOT_FOUND, format!("Controller '{}' not found", controller_id)).into_response();
                    }
                    return (StatusCode::NOT_FOUND, "No active controller found").into_response();
                };

                if !is_running(&controller) {
                    let controller_id = controller.get("id").and_then(|i| i.as_str()).unwrap_or("unknown");
                    return (StatusCode::NOT_FOUND, format!("Controller '{}' is not running", controller_id)).into_response();
                }

                if let Some(id) = controller.get("id").and_then(|i| i.as_str()) {
                    if let Some(active_model) = controller.get("active_model").and_then(|m| m.as_str()) {
                        // Use the models endpoint which handles subdirectories correctly
                        let model_url = format!("{}/api/controllers/{}/models/{}", target_sup, id, active_model);
                        println!("🔍 Fetching active model from: {}", model_url);

                        match client.get(&model_url).header("x-api-key", &api_key).send().await {
                            Ok(model_resp) if model_resp.status().is_success() => {
                                if let Ok(model_json) = model_resp.text().await {
                                    return (
                                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                                        model_json
                                    ).into_response();
                                }
                            }
                            Err(e) => {
                                println!("⚠️ Failed to fetch model: {}", e);
                                return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch model: {}", e)).into_response();
                            }
                            Ok(r) => {
                                println!("⚠️ Model fetch failed: {}", r.status());
                                return (StatusCode::BAD_GATEWAY, format!("Failed to fetch model from supervisor: {}", r.status())).into_response();
                            }
                        }
                    }
                }
                
                return (StatusCode::NOT_FOUND, "No active model found for controller").into_response();
            }
        }
        Err(e) => {
            println!("⚠️ Failed to list controllers: {}", e);
            return (StatusCode::BAD_GATEWAY, format!("Supervisor unreachable: {}", e)).into_response();
        }
        Ok(r) => {
            println!("⚠️ Controller list failed: {}", r.status());
            return (StatusCode::BAD_GATEWAY, "Failed to list controllers").into_response();
        }
    }
    
    (StatusCode::INTERNAL_SERVER_ERROR, "Unexpected error").into_response()
}

// --- GET SPECIFIC MODEL FILE FROM CONTROLLER ---
pub async fn get_controller_model_file(
    State(state): State<AppState>,
    user: CurrentUser,
    AxumPath((controller_id, model_filename)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ReadView) {
        return e.into_response();
    }

    // Fetch a specific model file from a controller's models directory
    let target_sup = state.settings.services.supervisor_url.clone();
    let api_key = state.settings.services.supervisor_api_key.clone();
    let client = Client::new();
    
    // The supervisor's controller_host stores models in controller_host/models/{controller_id}/{model_filename}
    // We need to fetch the file content
    let model_url = format!("{}/api/controllers/{}/models/{}", target_sup, controller_id, model_filename);
    println!("🔍 Fetching model file: {}", model_url);
    
    match client.get(&model_url).header("x-api-key", &api_key).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.text().await {
                Ok(model_json) => {
                    return (
                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                        model_json
                    ).into_response();
                }
                Err(e) => {
                    println!("⚠️ Failed to read model file: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read model: {}", e)).into_response();
                }
            }
        }
        Err(e) => {
            println!("⚠️ Failed to fetch model file: {}", e);
            return (StatusCode::BAD_GATEWAY, format!("Failed to fetch model: {}", e)).into_response();
        }
        Ok(r) => {
            println!("⚠️ Model file fetch failed: {} - {}", r.status(), model_url);
            return (StatusCode::NOT_FOUND, format!("Model file not found: {}", model_filename)).into_response();
        }
    }
}

// --- CONTROLLER PROXY HANDLERS ---
pub async fn list_controllers_proxy(
    State(state): State<AppState>,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ReadView) {
        return e.into_response();
    }

    // 1. Get primary supervisor
    let target_sup = state.settings.services.supervisor_url.clone();
    let api_key = state.settings.services.supervisor_api_key.clone();

    let client = Client::new();
    let url = format!("{}/api/controllers", target_sup);
    
    match client.get(&url).header("x-api-key", api_key).send().await {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(body) => (status, body).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Supervisor unreachable: {}", e)).into_response(),
    }
}

pub async fn start_controller_proxy(
    AxumPath(id): AxumPath<String>,
    State(state): State<AppState>,
    _: CsrfVerified,
    user: CurrentUser,
    Json(mut payload): Json<serde_json::Value>, // Read the incoming JSON body
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ControllerLifecycle) {
        return e.into_response();
    }

    let mut target_sup = state.settings.services.supervisor_url.clone();
    let api_key = state.settings.services.supervisor_api_key.clone();
    let mut target_opc_tcp = state.settings.opcua.endpoint_url.clone();
    let mut auth_mode = state.settings.auth.mode.as_str().to_string();
    let mut auth_username = state.settings.auth.username.clone();
    let mut auth_password = state.settings.auth.password.clone();
    let mut security_policy = state.settings.opcua.security_policy.clone();
    let mut message_mode = state.settings.opcua.message_mode.clone();

    let target_sup_from_payload = payload
        .get("target_supervisor")
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    if let Ok(infra) = InfrastructureConfig::load_from_db(&state.settings.hmi_auth.database_url).await {
        if let Some(from_payload) = target_sup_from_payload {
            target_sup = from_payload;
        } else if let Some(supervisor) = infra.supervisors.first() {
            target_sup = supervisor.url.clone();
        }

        if let Some(opc) = infra.opc_servers.first() {
            if let Some(endpoint) = &opc.opc_endpoint {
                if !endpoint.trim().is_empty() {
                    target_opc_tcp = endpoint.clone();
                }
            }
        }

        if let Some(selected_supervisor) = infra.supervisors.iter().find(|sup| sup.url == target_sup) {
            if let Some(host_client) = infra
                .controller_host_clients
                .iter()
                .find(|client| client.supervisor_id == selected_supervisor.id)
            {
                auth_mode = host_client.auth_mode.clone();
                let (resolved_username, resolved_password) =
                    infra.resolve_controller_host_username_password(&selected_supervisor.id);
                auth_username = resolved_username;
                auth_password = resolved_password;
                security_policy = host_client.security_policy.clone();
                message_mode = host_client.security_mode.clone();
            }
        }
    }

    // Inject OPC URL into the payload if not present
    if let Some(obj) = payload.as_object_mut() {
        if !obj.contains_key("opc_url") {
            obj.insert("opc_url".to_string(), serde_json::json!(target_opc_tcp));
        }
        if !obj.contains_key("security_policy") {
            obj.insert("security_policy".to_string(), serde_json::json!(security_policy));
        }
        if !obj.contains_key("message_mode") {
            obj.insert("message_mode".to_string(), serde_json::json!(message_mode));
        }
        if !obj.contains_key("auth_mode") {
            obj.insert("auth_mode".to_string(), serde_json::json!(normalize_auth_mode_payload(&auth_mode)));
        }
        if normalize_auth_mode_payload(&auth_mode) == "username" {
            if !obj.contains_key("username") {
                if let Some(username) = auth_username {
                    if !username.trim().is_empty() {
                        obj.insert("username".to_string(), serde_json::json!(username));
                    }
                }
            }
            if !obj.contains_key("password") {
                if let Some(password) = auth_password {
                    if !password.trim().is_empty() {
                        obj.insert("password".to_string(), serde_json::json!(password));
                    }
                }
            }
        }
    }

    let client = Client::new();
    let url = format!("{}/api/controllers/{}/start", target_sup, id);

    match client.post(&url).header("x-api-key", api_key).json(&payload).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                state
                    .auth
                    .audit(Some(&user), "controller.start", Some(&id), "success", None)
                    .await;
            }
            match resp.text().await {
                Ok(body) => (status, body).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Supervisor unreachable: {}", e)).into_response(),
    }
}

fn normalize_auth_mode_payload(mode: &str) -> &'static str {
    match mode.trim().to_ascii_lowercase().as_str() {
        "anonymous" | "anon" => "anonymous",
        "x509" | "cert" | "certificate" => "x509",
        _ => "username",
    }
}

pub async fn stop_controller_proxy(
    AxumPath(id): AxumPath<String>,
    State(state): State<AppState>,
    _: CsrfVerified,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ControllerLifecycle) {
        return e.into_response();
    }

    let target_sup = state.settings.services.supervisor_url.clone();
    let api_key = state.settings.services.supervisor_api_key.clone();

    let client = Client::new();
    let url = format!("{}/api/controllers/{}/stop", target_sup, id);

    match client.post(&url).header("x-api-key", api_key).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                state
                    .auth
                    .audit(Some(&user), "controller.stop", Some(&id), "success", None)
                    .await;
            }
            match resp.text().await {
                Ok(body) => (status, body).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Supervisor unreachable: {}", e)).into_response(),
    }
}

#[derive(Deserialize)]
pub struct LogTailQuery {
    pub lines: Option<usize>,
}

pub async fn get_controller_logs_proxy(
    AxumPath(id): AxumPath<String>,
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<LogTailQuery>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ReadView) {
        return e.into_response();
    }

    let target_sup = state.settings.services.supervisor_url.clone();
    let api_key = state.settings.services.supervisor_api_key.clone();

    let lines = query.lines.unwrap_or(80);
    let client = Client::new();
    let url = format!("{}/api/controllers/{}/logs/tail?lines={}", target_sup, id, lines);

    match client.get(&url).header("x-api-key", api_key).send().await {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(body) => (status, body).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Supervisor unreachable: {}", e)).into_response(),
    }
}

// --- QUERY HISTORICAL DATA FROM QUESTDB ---
#[derive(Deserialize)]
pub struct TrendsQuery {
    tags: String,        // Comma-separated: "TI1:PV,FC1:OP"
    start: String,       // ISO timestamp or relative: "2024-01-10T14:00:00Z" or "-1h"
    end: Option<String>, // ISO timestamp or "now"
    bucket_ms: Option<u64>,
    max_points: Option<usize>,
}

#[derive(Serialize)]
pub struct TrendsDataPoint {
    timestamp: String,
    tag: String,
    field: String,
    value: f64,
}

pub async fn get_trends_data(
    Query(params): Query<TrendsQuery>,
    State(state): State<AppState>,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ReadView) {
        return e.into_response();
    }

    use crate::historian::{query_historical_data, HistoricalQueryOptions};

    // Parse end time - use provided end or default to now
    let end_time = if let Some(end_str) = &params.end {
        if end_str == "now" {
            chrono::Utc::now()
        } else {
            match chrono::DateTime::parse_from_rfc3339(end_str) {
                Ok(dt) => dt.with_timezone(&chrono::Utc),
                Err(_) => {
                    return (
                        axum::http::StatusCode::BAD_REQUEST,
                        "Invalid end time format (use ISO 8601 or 'now')"
                    ).into_response();
                }
            }
        }
    } else {
        chrono::Utc::now()
    };

    // Parse start time - can be relative or absolute
    let start_time = if params.start.starts_with('-') {
        // Relative time like "-1h", "-15m"
        match parse_relative_time(&params.start, end_time) {
            Ok(t) => t,
            Err(e) => {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    format!("Invalid start time: {}", e)
                ).into_response();
            }
        }
    } else {
        // Absolute ISO timestamp
        match chrono::DateTime::parse_from_rfc3339(&params.start) {
            Ok(dt) => dt.with_timezone(&chrono::Utc),
            Err(_) => {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    "Invalid start time format (use ISO 8601 or relative like '-1h')"
                ).into_response();
            }
        }
    };

    // Parse node IDs (convert tag:field format)
    let node_ids: Vec<String> = params.tags
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if node_ids.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "No valid tags provided"
        ).into_response();
    }

    let range_ms = (end_time - start_time).num_milliseconds().max(1) as u64;
    let derived_bucket_ms = params.max_points
        .filter(|points| *points > 0)
        .map(|points| {
            let points_u64 = points as u64;
            let raw = (range_ms + points_u64 - 1) / points_u64;
            raw.max(1000)
        });

    let options = HistoricalQueryOptions {
        bucket_ms: params.bucket_ms.or(derived_bucket_ms),
    };

    // Query historian using shared function
    match query_historical_data(&state.historian_config, start_time, end_time, node_ids, options).await {
        Ok(node_histories) => {
            // Flatten grouped data into TrendsDataPoint format
            let mut data_points = Vec::new();
            
            for node_history in node_histories {
                for point in node_history.data {
                    data_points.push(TrendsDataPoint {
                        timestamp: point.timestamp,
                        tag: node_history.tag.clone(),
                        field: node_history.field.clone(),
                        value: point.value,
                    });
                }
            }
            
            Json(data_points).into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to query historical data: {}", e)
        ).into_response()
    }
}

/// Parse relative time strings like "1h", "30m", "1d" into absolute DateTime
fn parse_relative_time(time_str: &str, reference: chrono::DateTime<chrono::Utc>) -> Result<chrono::DateTime<chrono::Utc>, String> {
    use chrono::Duration;
    
    // Try parsing as ISO 8601 first
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(time_str) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    
    // Parse relative time (e.g., "1h", "30m", "2d")
    if time_str.len() < 2 {
        return Err("Time string too short".to_string());
    }
    
    let unit = time_str.chars().last().unwrap();
    let value_str = &time_str[..time_str.len()-1];
    let value: i64 = value_str.parse().map_err(|_| "Invalid time value")?;
    
    let duration = match unit {
        'h' => Duration::hours(value),
        'm' => Duration::minutes(value),
        's' => Duration::seconds(value),
        'd' => Duration::days(value),
        _ => return Err(format!("Unknown time unit: {}", unit)),
    };
    
    Ok(reference - duration)
}

// --- STEP RESPONSE API ROUTES ---

/// Request parameters for historical data query
#[derive(Debug, Deserialize)]
pub struct HistoricalDataQuery {
    pub start: String,  // ISO 8601 datetime
    pub end: String,    // ISO 8601 datetime
    pub nodes: String,  // Comma-separated node IDs (e.g., "MV1:OP,CV1:PV,CV2:PV")
}

/// Get historical time-series data for step response analysis
pub async fn get_step_response_data(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<HistoricalDataQuery>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ReadView) {
        return e.into_response();
    }

    use chrono::DateTime;
    use crate::historian::{query_historical_data, HistoricalQueryOptions};

    // Parse timestamps
    let start_time = match DateTime::parse_from_rfc3339(&params.start) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid start time format (use ISO 8601)"
            ).into_response();
        }
    };

    let end_time = match DateTime::parse_from_rfc3339(&params.end) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid end time format (use ISO 8601)"
            ).into_response();
        }
    };

    // Parse node IDs
    let node_ids: Vec<String> = params.nodes
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if node_ids.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "No node IDs specified"
        ).into_response();
    }

    // Query historian
    match query_historical_data(
        &state.historian_config,
        start_time,
        end_time,
        node_ids,
        HistoricalQueryOptions::default(),
    ).await {
        Ok(data) => Json(data).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to query historical data: {}", e)
        ).into_response()
    }
}

/// Get available tags for dropdown selection
pub async fn get_available_tags_api(
    State(state): State<AppState>,
    user: CurrentUser,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ReadView) {
        return e.into_response();
    }

    use crate::historian::get_available_tags;

    match get_available_tags(&state.historian_config).await {
        Ok(tags) => Json(tags).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get tags: {}", e)
        ).into_response()
    }
}

/// Step response calculation parameters
#[derive(Debug, Deserialize)]
pub struct StepResponseCalcRequest {
    pub mv_node: String,         // Which MV was stepped (e.g., "MV1:OP")
    pub cv_nodes: Vec<String>,   // Which CVs to model (e.g., ["CV1:PV", "CV2:PV"])
    pub mv_data: Vec<(String, f64)>,  // [(timestamp, value)]
    pub cv_data: Vec<Vec<(String, f64)>>,  // [[(timestamp, value)] for each CV]
    pub step_time: String,       // When the step occurred (ISO 8601)
    pub baseline_duration: f64,  // Seconds before step for baseline
    pub response_duration: f64,  // Seconds after step to capture
    pub sample_time: f64,        // Controller execution time in seconds (e.g., 20s)
}

/// Calculate step response coefficients from historical data
pub async fn calculate_step_response(
    State(_state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<StepResponseCalcRequest>,
) -> impl IntoResponse {
    if let Err(e) = user.require(Permission::ModelDeploy) {
        return e.into_response();
    }

    use chrono::DateTime;

    // Parse step time
    let step_time = match DateTime::parse_from_rfc3339(&request.step_time) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid step time format"
            ).into_response();
        }
    };

    // Calculate baseline (average before step)
    let baseline_start = step_time - chrono::Duration::seconds(request.baseline_duration as i64);
    let (mv_baseline, baseline_count) = calculate_baseline_with_count(&request.mv_data, &baseline_start, &step_time);

    if baseline_count == 0 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            format!("No data points found in baseline period ({}s before step). Try increasing baseline duration or check data availability.", request.baseline_duration)
        ).into_response();
    }

    // Find step size
    let (mv_after, after_count) = calculate_baseline_with_count(&request.mv_data, &step_time, 
        &(step_time + chrono::Duration::seconds(10)));  // First 10 seconds after step
    
    if after_count == 0 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "No data points found immediately after step time"
        ).into_response();
    }

    let step_size = mv_after - mv_baseline;

    if step_size.abs() < 0.01 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "Step size too small (< 0.01)"
        ).into_response();
    }

    // Calculate step response for each CV
    let num_points = (request.response_duration / request.sample_time).ceil() as usize;
    let mut responses = Vec::new();
    
    for (cv_node, cv_data) in request.cv_nodes.iter().zip(request.cv_data.iter()) {
        let cv_baseline = calculate_baseline(cv_data, &baseline_start, &step_time);
        
        // Collect all raw data points after step (within response duration)
        let mut raw_points: Vec<(f64, f64)> = Vec::new(); // (seconds_since_step, normalized_value)
        
        for (ts_str, value) in cv_data {
            if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
                let ts = ts.with_timezone(&chrono::Utc);
                if ts >= step_time {
                    let elapsed_secs = (ts - step_time).num_milliseconds() as f64 / 1000.0;
                    
                    if elapsed_secs <= request.response_duration {
                        let delta = (value - cv_baseline) / step_size;
                        raw_points.push((elapsed_secs, delta));
                    }
                }
            }
        }
        
        // Resample to uniform time grid using linear interpolation
        let coefficients = resample_step_response(&raw_points, num_points, request.sample_time);
        
        // Create fitted points with timestamps for visualization
        let fitted_points: Vec<(f64, f64)> = coefficients.iter().enumerate()
            .map(|(i, &val)| (i as f64 * request.sample_time, val))
            .collect();

        responses.push(serde_json::json!({
            "cv_node": cv_node,
            "cv_baseline": cv_baseline,
            "coefficients": coefficients,
            "raw_response": raw_points,  // For visualization
            "fitted_response": fitted_points,  // Resampled points
        }));
    }

    Json(serde_json::json!({
        "mv_node": request.mv_node,
        "mv_baseline": mv_baseline,
        "step_size": step_size,
        "step_time": request.step_time,
        "responses": responses,
    })).into_response()
}

/// Helper: Calculate baseline average with count
fn calculate_baseline_with_count(
    data: &[(String, f64)],
    start: &chrono::DateTime<chrono::Utc>,
    end: &chrono::DateTime<chrono::Utc>,
) -> (f64, usize) {
    use chrono::DateTime;
    
    let mut sum = 0.0;
    let mut count = 0;
    
    for (ts_str, value) in data {
        if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
            let ts = ts.with_timezone(&chrono::Utc);
            if ts >= *start && ts < *end {
                sum += value;
                count += 1;
            }
        }
    }
    
    let average = if count > 0 { sum / count as f64 } else { 0.0 };
    (average, count)
}

/// Helper: Calculate baseline average
fn calculate_baseline(
    data: &[(String, f64)],
    start: &chrono::DateTime<chrono::Utc>,
    end: &chrono::DateTime<chrono::Utc>,
) -> f64 {
    let (avg, _) = calculate_baseline_with_count(data, start, end);
    avg
}

/// Resample step response to uniform time grid with linear interpolation
fn resample_step_response(raw_points: &[(f64, f64)], num_points: usize, sample_time: f64) -> Vec<f64> {
    if raw_points.is_empty() {
        return vec![0.0; num_points];
    }
    
    let mut coefficients = Vec::with_capacity(num_points);
    
    for i in 0..num_points {
        let target_time = i as f64 * sample_time;
        
        // Find surrounding points for interpolation
        let mut before_idx = None;
        let mut after_idx = None;
        
        for (idx, &(t, _)) in raw_points.iter().enumerate() {
            if t <= target_time {
                before_idx = Some(idx);
            }
            if t >= target_time && after_idx.is_none() {
                after_idx = Some(idx);
                break;
            }
        }
        
        let value = match (before_idx, after_idx) {
            (Some(b), Some(a)) if b == a => {
                // Exact match
                raw_points[b].1
            }
            (Some(b), Some(a)) => {
                // Linear interpolation
                let (t1, v1) = raw_points[b];
                let (t2, v2) = raw_points[a];
                let ratio = (target_time - t1) / (t2 - t1);
                v1 + ratio * (v2 - v1)
            }
            (Some(b), None) => {
                // Extrapolate using last point (flat)
                raw_points[b].1
            }
            (None, Some(a)) => {
                // Before first point, use first value
                raw_points[a].1
            }
            (None, None) => 0.0,
        };
        
        coefficients.push(value);
    }
    
    coefficients
}

