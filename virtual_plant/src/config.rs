// src/config.rs
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;
use anyhow::{Context, Result};

// --- ROOT STRUCTURE ---
#[derive(Debug, Deserialize, Clone)]
pub struct UnifiedModel {
    pub metadata: Metadata,
    pub tuning: Tuning,
    pub variables: Variables,
    pub physics: Physics,
}

impl UnifiedModel {
    pub fn load(path: &str) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open config file: {}", path))?;
        let reader = BufReader::new(file);
        let config = serde_json::from_reader(reader)
            .context("Failed to parse JSON config")?;
        Ok(config)
    }
}

// --- SUB-STRUCTURES ---

#[derive(Debug, Deserialize, Clone)]
pub struct Metadata {
    pub name: String,
    pub description: String,
    pub version: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Tuning {
    pub prediction_horizon: usize,
    pub control_horizon: usize,
    pub sample_time: f64,
    pub solver_tolerance: f64,
    pub max_iterations: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Variables {
    pub cvs: Vec<CvConfig>,
    pub mvs: Vec<MvConfig>,
    #[serde(default)]
    pub dvs: Vec<DvConfig>,
}

// --- CV DEFINITIONS ---
#[derive(Debug, Deserialize, Clone)]
pub struct CvConfig {
    pub name: String,
    pub description: String,
    pub units: String,
    pub weight: f64,
    #[serde(default)] // Default to 0.0 if missing (Backwards Compat)
    pub alpha: f64,   // ✅ ADDED
    pub limits: CvLimits,
    pub node_ids: CvNodes,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CvLimits {
    pub low_low: f64,
    pub low: f64,
    pub target: f64,
    pub high: f64,
    pub high_high: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CvNodes {
    pub pv: String,
    pub target: String,
    pub prediction: String,
    // Included to match model.json structure
    pub limits: LimitNodes, 
}

// --- MV DEFINITIONS ---
#[derive(Debug, Deserialize, Clone)]
pub struct MvConfig {
    pub name: String,
    pub description: String, //
    pub units: String,
    pub weight_r: f64,
    pub max_move: f64,
    pub limits: MvLimits,
    pub node_ids: MvNodes,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MvLimits {
    pub low_low: f64,
    pub low: f64,
    pub high: f64,
    pub high_high: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MvNodes {
    pub sp: String,
    pub op: String,
    pub future_plan: String,
    // This was the missing piece causing your error
    pub limits: LimitNodes, 
}

#[derive(Debug, Deserialize, Clone)]
pub struct DvConfig {
    pub name: String,
    pub description: String,
    pub units: String,
    pub limits: MvLimits, // Re-use MvLimits (Low/High)
    pub node_ids: DvNodes,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DvNodes {
    pub pv: String,
}

// --- SHARED STRUCTS ---
#[derive(Debug, Deserialize, Clone)]
pub struct LimitNodes {
    pub high: String,
    pub low: String,
    pub hh: String,
    pub ll: String,
}

// --- PHYSICS (MATRICES) ---
#[derive(Debug, Deserialize, Clone)]
pub struct Physics {
    pub gain: Vec<Vec<f64>>,
    pub tau: Vec<Vec<f64>>,
    pub dead_time: Vec<Vec<f64>>,
}

// --- RUNTIME SETTINGS (FROM settings.toml) ---
#[derive(Debug, Deserialize, Clone)]
pub struct AppSettings {
    pub opcua: OpcuaConfig,
    pub identity: IdentityConfig,
    pub auth: AuthConfig,
    pub runtime: RuntimeConfig,
    pub debutanizer: DebutanizerConfig,
    pub cstr: CstrConfig,
    pub paths: PathConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpcuaConfig {
    pub endpoint_url: String,
    pub security_policy: String,
    pub message_mode: String,
    pub namespace_index: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IdentityConfig {
    pub app_name: String,
    pub app_uri: String,
    pub auto_create_keys: bool,
    pub trust_server_certs: bool,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Username,
    X509,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RuntimeConfig {
    pub speed_multiplier: usize,
    pub reconnect_delay_sec: u64,
    pub cycle_time_ms: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DebutanizerConfig {
    pub num_stages: usize,
    pub feed_stage: usize,
    pub relative_volatility: f64,
    pub hold_up_molar: f64,
    pub dt_seconds: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CstrConfig {
    // Empty for now - CSTR uses defaults
}

#[derive(Debug, Deserialize, Clone)]
pub struct PathConfig {
    pub pki_dir: String,
    pub cert_path: String,
    pub key_path: String,
}

impl AppSettings {
    pub fn load(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .context("Failed to read settings.toml")?;
        toml::from_str(&content).context("Failed to parse TOML")
    }
}