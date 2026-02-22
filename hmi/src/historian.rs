use anyhow::Result;
use questdb::ingress::{Sender, Buffer, TimestampNanos, ProtocolVersion};
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, Duration};
use crate::opc_worker::{OpcUpdate, OpcCommand};
use crate::config::HistorianConfig;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// QuestDB Historian - Continuous Time-Series Recorder
/// 
/// Subscribes to OPC UA updates and writes to QuestDB using ILP (InfluxDB Line Protocol).
/// Uses batching for performance - flushes every N records or every X milliseconds.
/// Additionally requests periodic snapshots of all nodes to ensure constant values are logged.
pub async fn run_historian(
    config: HistorianConfig,
    mut rx: broadcast::Receiver<OpcUpdate>,
    cmd_tx: mpsc::Sender<OpcCommand>,
) -> Result<()> {
    
    if !config.enabled {
        println!("📊 Historian: Disabled in config");
        return Ok(());
    }

    println!("📊 Historian: Connecting to QuestDB at {}:{}", config.host, config.ilp_port);

    // Connect to QuestDB ILP endpoint
    let mut sender = Sender::from_conf(&format!("tcp::addr={}:{};", config.host, config.ilp_port))?;
    let mut buffer = Buffer::new(ProtocolVersion::V1);
    
    let mut batch_count = 0;
    let mut flush_timer = interval(Duration::from_millis(config.flush_interval_ms));
    flush_timer.tick().await; // First tick completes immediately

    let mut snapshot_timer = interval(Duration::from_secs(config.snapshot_interval_sec));
    snapshot_timer.tick().await; // First tick completes immediately

    println!("✅ Historian: Connected. Recording to table '{}'", config.table_name);
    println!("📸 Historian: Snapshot mode enabled (every {}s)", config.snapshot_interval_sec);

    loop {
        tokio::select! {
            // Receive OPC UA updates
            Ok(update) = rx.recv() => {
                // Skip internal metadata nodes
                if update.node_id.starts_with("_") || update.node_id.starts_with("System:") {
                    continue;
                }

                // Write to buffer using ILP format
                match write_to_buffer(&mut buffer, &config.table_name, &update) {
                    Ok(_) => {
                        batch_count += 1;
                        
                        // Flush if batch size reached
                        if batch_count >= config.batch_size {
                            if let Err(e) = sender.flush(&mut buffer) {
                                eprintln!("❌ Historian flush error: {}", e);
                            }
                            buffer.clear();
                            batch_count = 0;
                        }
                    }
                    Err(e) => {
                        eprintln!("⚠️ Historian write error for {}: {}", update.node_id, e);
                    }
                }
            }
            
            // Periodic flush timer
            _ = flush_timer.tick() => {
                if batch_count > 0 {
                    if let Err(e) = sender.flush(&mut buffer) {
                        eprintln!("❌ Historian flush error: {}", e);
                    }
                    buffer.clear();
                    batch_count = 0;
                }
            }

            // Periodic snapshot timer - request all nodes to be read and logged
            _ = snapshot_timer.tick() => {
                if let Err(e) = cmd_tx.send(OpcCommand::ReadAll).await {
                    eprintln!("⚠️ Historian: Failed to request snapshot: {}", e);
                }
            }
            
            else => {
                println!("📊 Historian: Channel closed, shutting down");
                break;
            }
        }
    }

    // Final flush on shutdown
    if batch_count > 0 {
        let _ = sender.flush(&mut buffer);
    }

    Ok(())
}

/// Writes an OPC UA update to QuestDB buffer using ILP format
fn write_to_buffer(buffer: &mut Buffer, table_name: &str, update: &OpcUpdate) -> Result<()> {
    // Parse node_id into tag and field
    // Format: "CV1:PV" → tag=CV1, field=PV
    let parts: Vec<&str> = update.node_id.split(':').collect();
    let (tag, field) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        ("unknown", update.node_id.as_str())
    };

    // Start ILP line: table_name,tag=value
    buffer.table(table_name)?
        .symbol("tag", tag)?
        .symbol("field", field)?;

    // Write value based on type
    match &update.value {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                buffer.column_f64("value", f)?;
            } else if let Some(i) = n.as_i64() {
                buffer.column_i64("value", i)?;
            }
        }
        serde_json::Value::Bool(b) => {
            buffer.column_bool("value", *b)?;
        }
        serde_json::Value::String(s) => {
            buffer.column_str("value_str", s)?;
        }
        serde_json::Value::Array(arr) => {
            // Store arrays as JSON string for now
            buffer.column_str("value_array", &serde_json::to_string(arr)?)?;
        }
        _ => {
            buffer.column_str("value_str", "null")?;
        }
    }

    // Add status code
    buffer.column_i64("status", update.status as i64)?;

    // Timestamp: use current time (QuestDB will use server time if not specified)
    buffer.at(TimestampNanos::now())?;

    Ok(())
}

/// Time-series data point for step response analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    pub timestamp: String,  // ISO 8601 format
    pub value: f64,
}

/// Historical data for a single node/tag
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeHistory {
    pub node_id: String,
    pub tag: String,
    pub field: String,
    pub data: Vec<TimeSeriesPoint>,
}

/// Query historical data from QuestDB for step response analysis
/// 
/// Fetches time-series data for specified nodes within a time range.
/// Uses QuestDB's REST API for querying.
pub async fn query_historical_data(
    config: &HistorianConfig,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    node_ids: Vec<String>,
) -> Result<Vec<NodeHistory>> {
    if !config.enabled {
        return Ok(vec![]);
    }

    let client = reqwest::Client::new();
    let mut results = Vec::new();

    for node_id in node_ids {
        // Parse node_id into tag:field
        let parts: Vec<&str> = node_id.split(':').collect();
        let (tag, field) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            continue;
        };

        // Build SQL query for QuestDB - get all raw data points
        let query = format!(
            "SELECT timestamp, value FROM {} WHERE tag = '{}' AND field = '{}' AND timestamp >= '{}' AND timestamp < '{}' ORDER BY timestamp",
            config.table_name,
            tag,
            field,
            start_time.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            end_time.format("%Y-%m-%dT%H:%M:%S%.3fZ")
        );

        // Query QuestDB REST API
        let url = format!("http://{}:{}/exec", config.host, config.rest_port);
        let response = client
            .get(&url)
            .query(&[("query", &query)])
            .send()
            .await?;

        if !response.status().is_success() {
            eprintln!("⚠️ QuestDB query failed for {}: {}", node_id, response.status());
            continue;
        }

        // Parse JSON response
        let json: serde_json::Value = response.json().await?;
        let mut data_points = Vec::new();

        if let Some(dataset) = json["dataset"].as_array() {
            for row in dataset {
                if let Some(arr) = row.as_array() {
                    if arr.len() >= 2 {
                        if let (Some(ts), Some(val)) = (arr[0].as_str(), arr[1].as_f64()) {
                            data_points.push(TimeSeriesPoint {
                                timestamp: ts.to_string(),
                                value: val,
                            });
                        }
                    }
                }
            }
        }

        results.push(NodeHistory {
            node_id: node_id.clone(),
            tag: tag.to_string(),
            field: field.to_string(),
            data: data_points,
        });
    }

    Ok(results)
}

/// Get available tags from QuestDB (for UI dropdowns)
pub async fn get_available_tags(config: &HistorianConfig) -> Result<Vec<String>> {
    if !config.enabled {
        return Ok(vec![]);
    }

    let client = reqwest::Client::new();
    let query = format!(
        "SELECT DISTINCT tag FROM {} ORDER BY tag",
        config.table_name
    );

    let url = format!("http://{}:{}/exec", config.host, config.rest_port);
    let response = client
        .get(&url)
        .query(&[("query", &query)])
        .send()
        .await?;

    let json: serde_json::Value = response.json().await?;
    let mut tags = Vec::new();

    if let Some(dataset) = json["dataset"].as_array() {
        for row in dataset {
            if let Some(arr) = row.as_array() {
                if let Some(tag) = arr.get(0).and_then(|v| v.as_str()) {
                    tags.push(tag.to_string());
                }
            }
        }
    }

    Ok(tags)
}

