use anyhow::{Result, anyhow};
use opcua::client::Session;
use opcua::types::{
    AttributeId, DataValue, NodeId, NumericRange, 
    ReadValueId, TimestampsToReturn, Variant, WriteValue, StatusCode
};

/// Reads a list of NodeIds in a single network call (Bulk Read).
/// Returns a Vector of f64 values in the same order as the input NodeIds.
pub async fn read_bulk(session: &Session, node_ids: &[NodeId]) -> Result<Vec<f64>> {
    // 1. Prepare the request
    let nodes_to_read: Vec<ReadValueId> = node_ids.iter()
        .map(|id| ReadValueId {
            node_id: id.clone(),
            attribute_id: AttributeId::Value as u32,
            index_range: NumericRange::None,
            data_encoding: Default::default(),
        })
        .collect();

    // 2. Send Request
    let results = session.read(&nodes_to_read, TimestampsToReturn::Both, 0.0)
        .await
        .map_err(|e| anyhow!("OPC UA Read Network Error: {}", e))?;

    // 3. Unpack Results (Variant -> f64)
    let mut values = Vec::new();
    for (i, val) in results.iter().enumerate() {
        // Check Status Code (Good vs Bad)
        if let Some(ref status) = val.status {
            if status.is_bad() {
                return Err(anyhow!("Bad Status on Node {:?}: {:?}", node_ids[i], status));
            }
        }
        
        // Extract number (Handle Double or Float)
        let float_val = match &val.value {
            Some(Variant::Double(v)) => *v,
            Some(Variant::Float(v)) => *v as f64,
            Some(Variant::Int32(v)) => *v as f64, // Added Int32 support just in case
            _ => return Err(anyhow!("Node {:?} is not a number. Got: {:?}", node_ids[i], val.value)),
        };
        values.push(float_val);
    }

    Ok(values)
}

/// Writes a single f64 value to a NodeId.
pub async fn write_single(session: &Session, node_id: &NodeId, value: f64) -> Result<()> {
    let write_req = WriteValue {
        node_id: node_id.clone(),
        attribute_id: AttributeId::Value as u32,
        index_range: NumericRange::None,
        value: DataValue::new_now(Variant::Double(value)),
    };

    let results = session.write(&[write_req]).await
        .map_err(|e| anyhow!("OPC UA Write Network Error: {}", e))?;

    // Check if the server accepted the write
    if let Some(code) = results.first() {
        if code.is_bad() {
            return Err(anyhow!("Server rejected write to {:?}: {}", node_id, code));
        }
    }
    Ok(())
}

pub async fn write_array(
    session: &Session, 
    node_id: &NodeId, 
    values: Vec<f64>
) -> Result<(), StatusCode> {
    // Create a WriteValue with a Variant::Array
    let variant = Variant::from(values);
    
    let write_value = WriteValue {
        node_id: node_id.clone(),
        attribute_id: AttributeId::Value as u32,
        index_range: NumericRange::None,
        value: DataValue::new_now(variant),
    };

    let results = session.write(&[write_value]).await.map_err(|_| StatusCode::BadUnexpectedError)?;
    
    if let Some(code) = results.first() {
        if code.is_bad() { return Err(*code); }
    }
    Ok(())
}