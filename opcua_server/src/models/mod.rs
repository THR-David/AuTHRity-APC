use serde::Deserialize;
use serde_yaml::Value as YamlValue;
use std::sync::Arc;
use opcua::server::node_manager::memory::{InMemoryNodeManager, SimpleNodeManagerImpl};

/// Shared application state passed to API handlers
#[derive(Clone)]
pub struct AppState {
    pub node_manager: Arc<InMemoryNodeManager<SimpleNodeManagerImpl>>,
    pub namespace: u16,
}

/// Node specification from YAML model files
#[derive(Debug, Deserialize)]
pub struct NodeSpec {
    #[serde(rename = "nodeId")]
    pub node_id: String,
    #[serde(rename = "nodeClass")]
    pub node_class: Option<String>,
    #[serde(rename = "browseName")]
    pub browse_name: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "description")]
    pub description: Option<String>,
    #[serde(rename = "engineeringUnits")]
    pub engineering_units: Option<String>,
    #[serde(rename = "dataType")]
    pub data_type: Option<String>,
    #[serde(rename = "valueRank")]
    pub value_rank: Option<i32>,
    #[serde(rename = "arrayDimensions")]
    pub array_dimensions: Option<Vec<usize>>,
    #[serde(rename = "initialValue")]
    pub initial_value: Option<YamlValue>,
}

/// Root structure for YAML node definition files
#[derive(Debug, Deserialize)]
pub struct NodesFile {
    pub nodes: Vec<NodeSpec>,
}
