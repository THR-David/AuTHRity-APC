use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use tokio::fs;

const INFRA_CONFIG_KEY: &str = "infrastructure_config";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InfrastructureConfig {
    pub supervisors: Vec<ServiceConfig>,
    pub opc_servers: Vec<ServiceConfig>,
    #[serde(default)]
    pub username_secrets: HashMap<String, UsernameSecret>,
    #[serde(default)]
    pub hmi_client: HmiClientSettings,
    #[serde(default)]
    pub controller_host_clients: Vec<ControllerHostClientSettings>,
    #[serde(default)]
    pub opcua_defaults: OpcUaSecuritySettings,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServiceConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opc_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OpcUaSecuritySettings {
    pub security_policy: String,
    pub security_mode: String,
    pub auth_mode: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HmiClientSettings {
    pub security_policy: String,
    pub security_mode: String,
    pub auth_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ControllerHostClientSettings {
    pub supervisor_id: String,
    pub security_policy: String,
    pub security_mode: String,
    pub auth_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UsernameSecret {
    pub username: String,
    pub password: String,
}

impl Default for HmiClientSettings {
    fn default() -> Self {
        Self {
            security_policy: "Basic256Sha256".to_string(),
            security_mode: "SignAndEncrypt".to_string(),
            auth_mode: "Username".to_string(),
            username_ref: Some("hmi_client_default".to_string()),
            cert_ref: None,
            username: None,
            password: None,
        }
    }
}

impl Default for OpcUaSecuritySettings {
    fn default() -> Self {
        Self {
            security_policy: "Basic256Sha256".to_string(),
            security_mode: "SignAndEncrypt".to_string(),
            auth_mode: "Username".to_string(),
        }
    }
}

fn default_controller_host_clients(supervisors: &[ServiceConfig]) -> Vec<ControllerHostClientSettings> {
    supervisors
        .iter()
        .map(|sup| ControllerHostClientSettings {
            supervisor_id: sup.id.clone(),
            security_policy: "Basic256Sha256".to_string(),
            security_mode: "SignAndEncrypt".to_string(),
            auth_mode: "Username".to_string(),
            username_ref: Some(secret_ref_for_supervisor(&sup.id)),
            cert_ref: None,
            username: None,
            password: None,
        })
        .collect()
}

impl InfrastructureConfig {
    pub fn redacted_for_api(&self) -> Self {
        let mut redacted = self.clone();
        let (hmi_username, _) = self.resolve_hmi_username_password();
        redacted.hmi_client.username = hmi_username;
        redacted.hmi_client.password = None;
        for client in &mut redacted.controller_host_clients {
            let (username, _) = self.resolve_controller_host_username_password(&client.supervisor_id);
            client.username = username;
            client.password = None;
        }
        redacted
    }

    pub fn with_secret_refs(mut self, existing: Option<&InfrastructureConfig>) -> Self {
        if let Some(current) = existing {
            for (key, value) in &current.username_secrets {
                self.username_secrets
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
        }

        self.capture_hmi_secret(existing);
        self.capture_controller_host_secrets(existing);
        self
    }

    pub fn resolve_hmi_username_password(&self) -> (Option<String>, Option<String>) {
        if let Some(reference) = self.hmi_client.username_ref.as_ref() {
            if let Some(secret) = self.username_secrets.get(reference) {
                return (Some(secret.username.clone()), Some(secret.password.clone()));
            }
        }

        (self.hmi_client.username.clone(), self.hmi_client.password.clone())
    }

    pub fn resolve_controller_host_username_password(
        &self,
        supervisor_id: &str,
    ) -> (Option<String>, Option<String>) {
        if let Some(client) = self
            .controller_host_clients
            .iter()
            .find(|candidate| candidate.supervisor_id == supervisor_id)
        {
            if let Some(reference) = client.username_ref.as_ref() {
                if let Some(secret) = self.username_secrets.get(reference) {
                    return (Some(secret.username.clone()), Some(secret.password.clone()));
                }
            }
            return (client.username.clone(), client.password.clone());
        }

        (None, None)
    }

    pub async fn init_schema(database_url: &str) -> Result<()> {
        let pool = connect(database_url).await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS infra_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(())
    }

    pub async fn seed_if_empty(database_url: &str, hosts_json_path: &str) -> Result<()> {
        Self::init_schema(database_url).await?;
        let pool = connect(database_url).await?;
        let existing = sqlx::query("SELECT value FROM infra_settings WHERE key = ?")
            .bind(INFRA_CONFIG_KEY)
            .fetch_optional(&pool)
            .await?;

        if existing.is_none() {
            let config = Self::load(hosts_json_path).await?;
            config.save_to_db(database_url).await?;
        }

        Ok(())
    }

    pub async fn load_from_db(database_url: &str) -> Result<Self> {
        Self::init_schema(database_url).await?;
        let pool = connect(database_url).await?;

        let row = sqlx::query("SELECT value FROM infra_settings WHERE key = ?")
            .bind(INFRA_CONFIG_KEY)
            .fetch_optional(&pool)
            .await?;

        let Some(row) = row else {
            return Err(anyhow::anyhow!("Infrastructure config is not initialized in SQLite"));
        };

        let payload: String = row.try_get("value")?;
        let mut config: InfrastructureConfig = serde_json::from_str(&payload)?;
        config.normalize_defaults();
        Ok(config)
    }

    pub async fn save_to_db(&self, database_url: &str) -> Result<()> {
        Self::init_schema(database_url).await?;
        let pool = connect(database_url).await?;
        let mut payload = self.clone();
        payload.normalize_defaults();

        let value = serde_json::to_string_pretty(&payload)?;
        sqlx::query(
            "INSERT INTO infra_settings (key, value, updated_at) VALUES (?, ?, CURRENT_TIMESTAMP) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
        )
        .bind(INFRA_CONFIG_KEY)
        .bind(value)
        .execute(&pool)
        .await?;

        Ok(())
    }

    pub async fn load(path: &str) -> Result<Self> {
        if Path::new(path).exists() {
            let content = fs::read_to_string(path).await?;
            let mut config: InfrastructureConfig = serde_json::from_str(&content)?;
            config.normalize_defaults();
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    pub async fn save(&self, path: &str) -> Result<()> {
        let mut payload = self.clone();
        payload.normalize_defaults();
        let content = serde_json::to_string_pretty(&payload)?;
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(path, content).await?;
        Ok(())
    }

    fn normalize_defaults(&mut self) {


        if self.opc_servers.is_empty() {
            self.opc_servers = Self::default().opc_servers;
        }

        if self.hmi_client.security_policy.trim().is_empty() {
            self.hmi_client.security_policy = "Basic256Sha256".to_string();
        }
        if self.hmi_client.security_mode.trim().is_empty() {
            self.hmi_client.security_mode = "SignAndEncrypt".to_string();
        }
        if self.hmi_client.auth_mode.trim().is_empty() {
            self.hmi_client.auth_mode = "Username".to_string();
        }

        if self.hmi_client.username_ref.is_none() {
            self.hmi_client.username_ref = Some("hmi_client_default".to_string());
        }

        if let (Some(username), Some(password), Some(reference)) = (
            self.hmi_client.username.clone(),
            self.hmi_client.password.clone(),
            self.hmi_client.username_ref.clone(),
        ) {
            if !username.trim().is_empty() && !password.trim().is_empty() {
                self.username_secrets
                    .insert(reference, UsernameSecret { username, password });
            }
        }
        self.hmi_client.username = None;
        self.hmi_client.password = None;

        for opc in &mut self.opc_servers {
            if opc.security_policy.is_none() {
                opc.security_policy = Some(self.hmi_client.security_policy.clone());
            }
            if opc.security_mode.is_none() {
                opc.security_mode = Some(self.hmi_client.security_mode.clone());
            }
            if opc.auth_mode.is_none() {
                opc.auth_mode = Some(self.hmi_client.auth_mode.clone());
            }
        }

        if self.hmi_client.username.is_none() {
            self.hmi_client.username = Some("hmi".to_string());
        }
        if self.hmi_client.password.is_none() {
            self.hmi_client.password = Some("password".to_string());
        }

        let mut synced_clients: Vec<ControllerHostClientSettings> = Vec::with_capacity(self.supervisors.len());
        for supervisor in &self.supervisors {
            if let Some(existing) = self
                .controller_host_clients
                .iter()
                .find(|client| client.supervisor_id == supervisor.id)
            {
                synced_clients.push(existing.clone());
            } else {
                synced_clients.push(ControllerHostClientSettings {
                    supervisor_id: supervisor.id.clone(),
                    security_policy: self.hmi_client.security_policy.clone(),
                    security_mode: self.hmi_client.security_mode.clone(),
                    auth_mode: self.hmi_client.auth_mode.clone(),
                    username_ref: Some(secret_ref_for_supervisor(&supervisor.id)),
                    cert_ref: None,
                    username: None,
                    password: None,
                });
            }
        }
        self.controller_host_clients = synced_clients;

        for client in &mut self.controller_host_clients {
            if client.security_policy.trim().is_empty() {
                client.security_policy = self.hmi_client.security_policy.clone();
            }
            if client.security_mode.trim().is_empty() {
                client.security_mode = self.hmi_client.security_mode.clone();
            }
            if client.auth_mode.trim().is_empty() {
                client.auth_mode = self.hmi_client.auth_mode.clone();
            }
            if client.username_ref.is_none() {
                client.username_ref = Some(secret_ref_for_supervisor(&client.supervisor_id));
            }

            if let (Some(username), Some(password), Some(reference)) = (
                client.username.clone(),
                client.password.clone(),
                client.username_ref.clone(),
            ) {
                if !username.trim().is_empty() && !password.trim().is_empty() {
                    self.username_secrets
                        .insert(reference, UsernameSecret { username, password });
                }
            }

            client.username = None;
            client.password = None;
        }
    }

    fn capture_hmi_secret(&mut self, existing: Option<&InfrastructureConfig>) {
        let existing_ref = existing.and_then(|cfg| cfg.hmi_client.username_ref.clone());
        let fallback_ref = self.hmi_client.username_ref.clone().or(existing_ref);
        let reference = fallback_ref.unwrap_or_else(|| "hmi_client_default".to_string());

        self.hmi_client.username_ref = Some(reference.clone());

        if let (Some(username), Some(password)) = (
            self.hmi_client.username.clone(),
            self.hmi_client.password.clone(),
        ) {
            if !username.trim().is_empty() && !password.trim().is_empty() {
                self.username_secrets
                    .insert(reference, UsernameSecret { username, password });
            }
        }

        self.hmi_client.username = None;
        self.hmi_client.password = None;
    }

    fn capture_controller_host_secrets(&mut self, existing: Option<&InfrastructureConfig>) {
        for client in &mut self.controller_host_clients {
            let existing_ref = existing
                .and_then(|cfg| {
                    cfg.controller_host_clients
                        .iter()
                        .find(|old| old.supervisor_id == client.supervisor_id)
                })
                .and_then(|old| old.username_ref.clone());

            let reference = client
                .username_ref
                .clone()
                .or(existing_ref)
                .unwrap_or_else(|| secret_ref_for_supervisor(&client.supervisor_id));

            client.username_ref = Some(reference.clone());

            if let (Some(username), Some(password)) = (client.username.clone(), client.password.clone()) {
                if !username.trim().is_empty() && !password.trim().is_empty() {
                    self.username_secrets
                        .insert(reference, UsernameSecret { username, password });
                }
            }

            client.username = None;
            client.password = None;
        }
    }
}

impl Default for InfrastructureConfig {
    fn default() -> Self {
        Self {
            supervisors: vec![ServiceConfig {
                id: "default-sup".to_string(),
                name: "Localhost Supervisor".to_string(),
                url: "http://127.0.0.1:8080".to_string(),
                opc_endpoint: None,
                security_policy: None,
                security_mode: None,
                auth_mode: None,
            }],
            opc_servers: vec![ServiceConfig {
                id: "default-opc".to_string(),
                name: "Localhost Connectivity".to_string(),
                url: "http://127.0.0.1:9090".to_string(),
                opc_endpoint: Some("opc.tcp://localhost:4855".to_string()),
                security_policy: None,
                security_mode: None,
                auth_mode: None,
            }],
            username_secrets: HashMap::from([
                (
                    "hmi_client_default".to_string(),
                    UsernameSecret {
                        username: "hmi".to_string(),
                        password: "password".to_string(),
                    },
                ),
                (
                    "controller_host_default-sup".to_string(),
                    UsernameSecret {
                        username: "hmi".to_string(),
                        password: "password".to_string(),
                    },
                ),
            ]),
            hmi_client: HmiClientSettings::default(),
            controller_host_clients: default_controller_host_clients(&vec![ServiceConfig {
                id: "default-sup".to_string(),
                name: "Localhost Supervisor".to_string(),
                url: "http://127.0.0.1:8080".to_string(),
                opc_endpoint: None,
                security_policy: None,
                security_mode: None,
                auth_mode: None,
            }]),
            opcua_defaults: OpcUaSecuritySettings::default(),
        }
    }
}

async fn connect(database_url: &str) -> Result<SqlitePool> {
    let normalized_url = normalize_sqlite_url(database_url);
    let options = SqliteConnectOptions::from_str(&normalized_url)?.create_if_missing(true);
    Ok(SqlitePool::connect_with(options).await?)
}

fn normalize_sqlite_url(input: &str) -> String {
    if input.starts_with("sqlite://") && !input.starts_with("sqlite:///") {
        let rest = input.trim_start_matches("sqlite://");
        return format!("sqlite:{}", rest);
    }
    input.to_string()
}

fn secret_ref_for_supervisor(supervisor_id: &str) -> String {
    let sanitized: String = supervisor_id
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' })
        .collect();
    format!("controller_host_{}", sanitized)
}
