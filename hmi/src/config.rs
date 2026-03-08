use serde::Deserialize;
use anyhow::{Context, Result};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct AppSettings {
    pub opcua: OpcuaConfig,
    pub identity: IdentityConfig,
    pub auth: AuthConfig,
    pub runtime: RuntimeConfig,
    pub services: ServicesConfig,
    pub paths: PathConfig,
    pub historian: HistorianConfig,
    pub hmi_auth: HmiAuthConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HmiAuthConfig {
    pub database_url: String,
    #[serde(default = "default_session_inactivity_minutes")]
    pub session_inactivity_minutes: u64,
}

fn default_session_inactivity_minutes() -> u64 {
    60 // 1 hour
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServicesConfig {
    pub supervisor_url: String,   // e.g. "http://127.0.0.1:8080"
    pub supervisor_api_key: String,
    pub opc_hot_reload_url: String, // e.g. "http://127.0.0.1:9090"
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpcuaConfig {
    pub endpoint_url: String,
    pub security_policy: String,
    pub message_mode: String,
    pub namespace_index: u16,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Username,
    X509,
}

impl AuthMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthMode::Username => "username",
            AuthMode::X509 => "x509",
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IdentityConfig {
    pub app_name: String,
    pub app_uri: String,
    pub auto_create_keys: bool,
    pub trust_server_certs: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RuntimeConfig {
    pub model_path: String,
    pub reconnect_delay_sec: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PathConfig {
    pub pki_dir: String,
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HistorianConfig {
    pub enabled: bool,
    pub host: String,
    pub ilp_port: u16,
    pub http_port: u16,
    pub rest_port: u16,  // QuestDB REST API port (default 9000)
    pub table_name: String,
    pub batch_size: usize,
    pub flush_interval_ms: u64,
    #[serde(default = "default_snapshot_interval_sec")]
    pub snapshot_interval_sec: u64,  // Periodic logging of all nodes (even if unchanged)
    #[serde(default)]
    pub deadband: HistorianDeadbandConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HistorianDeadbandConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_deadband_absolute")]
    pub absolute_default: f64,
    #[serde(default = "default_deadband_relative_percent")]
    pub relative_percent_default: f64,
    #[serde(default = "default_deadband_max_silence_sec")]
    pub max_silence_sec: u64,
    #[serde(default)]
    pub node_overrides: HashMap<String, HistorianDeadbandRule>,
    #[serde(default)]
    pub field_overrides: HashMap<String, HistorianDeadbandRule>,
}

impl Default for HistorianDeadbandConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            absolute_default: default_deadband_absolute(),
            relative_percent_default: default_deadband_relative_percent(),
            max_silence_sec: default_deadband_max_silence_sec(),
            node_overrides: HashMap::new(),
            field_overrides: HashMap::new(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct HistorianDeadbandRule {
    pub absolute: Option<f64>,
    pub relative_percent: Option<f64>,
    pub max_silence_sec: Option<u64>,
}

fn default_snapshot_interval_sec() -> u64 {
    60  // Default: log all nodes every 60 seconds (1 minute)
}

fn default_deadband_absolute() -> f64 {
    0.005
}

fn default_deadband_relative_percent() -> f64 {
    0.1
}

fn default_deadband_max_silence_sec() -> u64 {
    300
}

impl AppSettings {
    pub fn load(path: &str) -> Result<Self> {
        // Standard file reading logic (same as your Model load)
        let content = std::fs::read_to_string(path)
            .context("Failed to read settings.toml")?;
        toml::from_str(&content).context("Failed to parse TOML")
    }
}