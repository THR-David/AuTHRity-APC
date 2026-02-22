use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InfrastructureConfig {
    pub supervisors: Vec<ServiceConfig>,
    pub opc_servers: Vec<ServiceConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServiceConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opc_endpoint: Option<String>,
}

impl InfrastructureConfig {
    pub async fn load(path: &str) -> Result<Self> {
        if Path::new(path).exists() {
            let content = fs::read_to_string(path).await?;
            let config = serde_json::from_str(&content)?;
            Ok(config)
        } else {
            // Return default config if file doesn't exist
            Ok(Self {
                supervisors: vec![ServiceConfig {
                    id: "default-sup".to_string(),
                    name: "Localhost Supervisor".to_string(),
                    url: "http://127.0.0.1:8080".to_string(),
                    opc_endpoint: None,
                }],
                opc_servers: vec![ServiceConfig {
                    id: "default-opc".to_string(),
                    name: "Localhost Connectivity".to_string(),
                    url: "http://127.0.0.1:9090".to_string(),
                    opc_endpoint: Some("opc.tcp://127.0.0.1:4840".to_string()),
                }],
            })
        }
    }

    pub async fn save(&self, path: &str) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(path, content).await?;
        Ok(())
    }
}
