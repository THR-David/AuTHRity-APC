use serde::Deserialize;
use std::fs;
use std::path::Path;

// Constants
pub const MODELS_DIR: &str = "models";
pub const SERVER_CONF: &str = "config/serversettings.env";
pub const SERVER_CONF_SAMPLE: &str = "config/serversettings.env.sample";
pub const LEGACY_SERVER_CONF: &str = "config/server.conf";
pub const NAMESPACE_URI: &str = "urn:ModelPredictiveControlServer:Model";

pub fn ensure_server_conf_exists() {
    let runtime_path = Path::new(SERVER_CONF);
    if runtime_path.exists() {
        return;
    }

    if let Some(parent) = runtime_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("Failed to create config dir {:?}: {}", parent, e);
            return;
        }
    }

    let legacy_path = Path::new(LEGACY_SERVER_CONF);
    if legacy_path.exists() {
        if let Err(e) = fs::copy(legacy_path, runtime_path) {
            eprintln!(
                "Failed to migrate {:?} to {:?}: {}",
                legacy_path, runtime_path, e
            );
        } else {
            println!(
                "Migrated OPC UA server config from {} to {}",
                LEGACY_SERVER_CONF, SERVER_CONF
            );
        }
        return;
    }

    let sample_path = Path::new(SERVER_CONF_SAMPLE);
    if sample_path.exists() {
        if let Err(e) = fs::copy(sample_path, runtime_path) {
            eprintln!(
                "Failed to seed {:?} from sample {:?}: {}",
                runtime_path, sample_path, e
            );
        } else {
            println!("Seeded OPC UA server config from {}", SERVER_CONF_SAMPLE);
        }
        return;
    }

    eprintln!(
        "No OPC UA server config found. Expected '{}' (or sample '{}').",
        SERVER_CONF, SERVER_CONF_SAMPLE
    );
}

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
