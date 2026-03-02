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
    #[serde(default = "default_model_type")]
    pub model_type: String, // "parametric" or "step_response"
}

fn default_model_type() -> String {
    "parametric".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct Tuning {
    pub prediction_horizon: usize,
    pub control_horizon: usize,
    pub sample_time: f64,
    pub solver_tolerance: f64,
    pub max_iterations: usize,
    #[serde(default = "default_terminal_weight")]
    pub terminal_weight_factor: f64, // Feature 3: Multiply last step weight (default 10.0)
}

fn default_terminal_weight() -> f64 {
    10.0 // Industry standard: heavily weight terminal state for stability
}

#[derive(Debug, Deserialize, Clone)]
pub struct Variables {
    pub cvs: Vec<CvConfig>,
    pub mvs: Vec<MvConfig>,
    #[serde(default)]
    pub dvs: Vec<DvConfig>,
}

// --- CV DEFINITIONS ---

/// Optimization mode for CVs
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum OptimizationMode {
    /// Traditional target tracking: minimize (CV - target)^2
    Target { 
        #[allow(dead_code)] // Used in pattern matching in DMC logic
        value: f64 
    },
    /// Zone control: no cost if within [low, high], penalize violations
    Zone,
    /// Maximize CV within limits (linear cost pushing toward high limit)
    Maximize,
    /// Minimize CV within limits (linear cost pushing toward low limit)
    Minimize,
}

impl Default for OptimizationMode {
    fn default() -> Self {
        OptimizationMode::Target { value: 0.0 }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CvConfig {
    pub name: String,
    pub description: String,
    pub units: String,
    pub weight: f64,
    #[serde(default)] // Default to 0.0 if missing (Backwards Compat)
    pub alpha: f64,
    #[serde(default = "default_ece_factor")] // Default to 1.0 if missing
    pub ece_factor: f64, // Equal Concern Error normalization factor
    #[serde(default)] // Default to Target mode with value from limits.target
    pub optimization_mode: OptimizationMode,
    #[serde(default = "default_slack_weight")] // Penalty for soft constraint violations
    pub slack_weight: f64,
    #[serde(default)] // Feature 2: Integrating variable flag
    pub is_integrating: bool, // If true, use input disturbance instead of output bias
    pub limits: CvLimits,
    pub node_ids: CvNodes,
}

fn default_ece_factor() -> f64 {
    1.0
}

fn default_slack_weight() -> f64 {
    1000.0 // High penalty for constraint violations
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
    #[allow(dead_code)] // Reserved for future HMI/DCS integration
    pub limits: LimitNodes, 
}

// --- MV DEFINITIONS ---
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum MvOptimizationMode {
    /// Traditional MV target tracking
    Target {
        #[allow(dead_code)]
        value: f64,
    },
    /// Push MV toward upper operating limit
    Maximize,
    /// Push MV toward lower operating limit
    Minimize,
}

impl Default for MvOptimizationMode {
    fn default() -> Self {
        MvOptimizationMode::Target { value: 0.0 }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct MvConfig {
    pub name: String,
    pub description: String, //
    pub units: String,
    pub weight_r: f64,
    pub max_move: f64,
    #[serde(default)]
    pub optimization_mode: MvOptimizationMode,
    #[serde(default)]
    pub target: Option<f64>,        // Optional target for economic MVs
    #[serde(default)]
    pub target_weight: f64,         // Weight for target tracking (0.0 = no target)
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
    #[allow(dead_code)] // Reserved for future feedback validation
    pub pv: String,          // Actual process measurement
    pub sp: String,          // Setpoint (where MPC writes)
    pub op: String,          // PID output to valve
    pub mode: String,        // Actual PID mode (Generic: 0=Operator, 1=Auto, 2=Cascade, 3=RemoteCascade)
    pub mode_target: String, // Target/requested mode from operator
    #[serde(default)]
    pub target: Option<String>, // Optional economic target node
    pub future_plan: String,
    pub limits: LimitNodes, 
}

#[derive(Debug, Deserialize, Clone)]
pub struct DvConfig {
    pub name: String,
    pub description: String,
    pub units: String,
    pub limits: DvLimits,
    pub node_ids: DvNodes,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DvLimits {
    pub low: f64,
    pub high: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DvNodes {
    pub pv: String,
    pub limits: DvLimitNodes,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DvLimitNodes {
    pub high: String,
    pub low: String,
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
    // Parametric model (FOPDT)
    #[serde(default)]
    pub gain: Vec<Vec<f64>>,
    #[serde(default)]
    pub tau: Vec<Vec<f64>>,
    #[serde(default)]
    pub dead_time: Vec<Vec<f64>>,
    
    // DV (Disturbance Variable) FOPDT parameters
    #[serde(default)]
    pub gain_dv: Vec<Vec<f64>>,
    #[serde(default)]
    pub tau_dv: Vec<Vec<f64>>,
    #[serde(default)]
    pub dead_time_dv: Vec<Vec<f64>>,
    
    // Non-parametric model (Step Response Coefficients)
    // step_coefficients[cv_idx][mv_idx][time_step]
    #[serde(default)]
    pub step_coefficients: Vec<Vec<Vec<f64>>>,
    
    // dv_coefficients[cv_idx][dv_idx][time_step]
    #[serde(default)]
    pub dv_coefficients: Vec<Vec<Vec<f64>>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppSettings {
    pub opcua: OpcuaConfig,
    pub identity: IdentityConfig,
    pub auth: AuthConfig,
    pub runtime: RuntimeConfig,
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
    pub model_path: String,
    #[allow(dead_code)] // Reserved for future reconnection logic
    pub reconnect_delay_sec: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PathConfig {
    #[allow(dead_code)] // Reserved for alternative PKI configuration
    pub pki_dir: String,
    pub cert_path: String,
    pub key_path: String,
}

// Remove impl AppSettings::load as we will build it manually
