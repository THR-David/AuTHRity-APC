use serde::Deserialize;
use std::fs;

// Constants
pub const MODELS_DIR: &str = "models";
pub const SERVER_CONF: &str = "config/server.conf";
pub const NAMESPACE_URI: &str = "urn:ModelPredictiveControlServer:Model";

#[derive(Debug, Deserialize)]
pub struct Config {
    pub api: ApiConfig,
}

#[derive(Debug, Deserialize)]
pub struct ApiConfig {
    pub port: u16,
}

impl Config {
    pub fn load() -> Self {
        let config_path = "config/config.toml";
        
        match fs::read_to_string(config_path) {
            Ok(contents) => {
                toml::from_str(&contents).unwrap_or_else(|e| {
                    eprintln!("Failed to parse {}: {}, using defaults", config_path, e);
                    Self::default()
                })
            }
            Err(_) => {
                eprintln!("Config file {} not found, using defaults", config_path);
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            api: ApiConfig { port: 9090 },
        }
    }
}
