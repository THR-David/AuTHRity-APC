use anyhow::Result;
use questdb::ingress::{Sender, Buffer, TimestampNanos, ProtocolVersion};
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, Duration};
use crate::opc_worker::{OpcUpdate, OpcCommand};
use crate::config::HistorianConfig;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
enum LoggedValue {
    Number(f64),
    Bool(bool),
    Text(String),
    Null,
}

#[derive(Debug, Clone)]
struct LoggedSample {
    value: LoggedValue,
    logged_at: Instant,
}

fn get_field(node_id: &str) -> Option<&str> {
    node_id.split_once(':').map(|(_, field)| field)
}

fn logged_value_from_json(value: &serde_json::Value) -> LoggedValue {
    match value {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                LoggedValue::Number(f)
            } else if let Some(i) = n.as_i64() {
                LoggedValue::Number(i as f64)
            } else {
                LoggedValue::Text(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => LoggedValue::Bool(*b),
        serde_json::Value::String(s) => LoggedValue::Text(s.clone()),
        serde_json::Value::Null => LoggedValue::Null,
        _ => LoggedValue::Text(value.to_string()),
    }
}

fn deadband_settings_for_node(config: &HistorianConfig, node_id: &str) -> (f64, f64, u64) {
    let mut absolute = config.deadband.absolute_default.max(0.0);
    let mut relative_percent = config.deadband.relative_percent_default.max(0.0);
    let mut max_silence_sec = config.deadband.max_silence_sec;

    if let Some(field) = get_field(node_id) {
        if let Some(rule) = config.deadband.field_overrides.get(field) {
            if let Some(v) = rule.absolute {
                absolute = v.max(0.0);
            }
            if let Some(v) = rule.relative_percent {
                relative_percent = v.max(0.0);
            }
            if let Some(v) = rule.max_silence_sec {
                max_silence_sec = v;
            }
        }
    }

    if let Some(rule) = config.deadband.node_overrides.get(node_id) {
        if let Some(v) = rule.absolute {
            absolute = v.max(0.0);
        }
        if let Some(v) = rule.relative_percent {
            relative_percent = v.max(0.0);
        }
        if let Some(v) = rule.max_silence_sec {
            max_silence_sec = v;
        }
    }

    (absolute, relative_percent, max_silence_sec)
}

fn should_log_update(
    config: &HistorianConfig,
    node_id: &str,
    current: &LoggedValue,
    last_logged: &HashMap<String, LoggedSample>,
    now: Instant,
) -> bool {
    if !config.deadband.enabled {
        return true;
    }

    let Some(previous) = last_logged.get(node_id) else {
        return true;
    };

    let (absolute, relative_percent, max_silence_sec) = deadband_settings_for_node(config, node_id);
    if max_silence_sec > 0 && now.duration_since(previous.logged_at).as_secs() >= max_silence_sec {
        return true;
    }

    match (&previous.value, current) {
        (LoggedValue::Number(prev), LoggedValue::Number(curr)) => {
            let delta = (curr - prev).abs();
            let scale = prev.abs().max(curr.abs());
            let relative_threshold = (relative_percent / 100.0) * scale;
            let threshold = absolute.max(relative_threshold);
            delta >= threshold
        }
        (LoggedValue::Bool(prev), LoggedValue::Bool(curr)) => prev != curr,
        (LoggedValue::Text(prev), LoggedValue::Text(curr)) => prev != curr,
        (LoggedValue::Null, LoggedValue::Null) => false,
        _ => true,
    }
}

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
    let mut last_logged: HashMap<String, LoggedSample> = HashMap::new();
    
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

                let current_value = logged_value_from_json(&update.value);
                let now = Instant::now();
                if !should_log_update(&config, &update.node_id, &current_value, &last_logged, now) {
                    continue;
                }

                // Write to buffer using ILP format
                match write_to_buffer(&mut buffer, &config.table_name, &update) {
                    Ok(_) => {
                        last_logged.insert(
                            update.node_id.clone(),
                            LoggedSample {
                                value: current_value,
                                logged_at: now,
                            },
                        );
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

#[derive(Debug, Clone, Default)]
pub struct HistoricalQueryOptions {
    pub bucket_ms: Option<u64>,
}

fn format_sample_by_interval(bucket_ms: u64) -> String {
    let ceil_div = |a: u64, b: u64| -> u64 { a.div_ceil(b) };

    if bucket_ms % (60 * 60 * 1000) == 0 {
        return format!("{}h", bucket_ms / (60 * 60 * 1000));
    }
    if bucket_ms % (60 * 1000) == 0 {
        return format!("{}m", bucket_ms / (60 * 1000));
    }
    if bucket_ms % 1000 == 0 {
        return format!("{}s", bucket_ms / 1000);
    }

    // Some QuestDB builds reject millisecond units in SAMPLE BY.
    // Round up to the nearest supported whole-second bucket.
    let seconds = ceil_div(bucket_ms, 1000).max(1);
    format!("{}s", seconds)
}

fn build_raw_query(
    table_name: &str,
    tag: &str,
    field: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> String {
    format!(
        "SELECT timestamp, value FROM {} WHERE tag = '{}' AND field = '{}' AND timestamp >= '{}' AND timestamp < '{}' ORDER BY timestamp",
        table_name,
        tag,
        field,
        start_time.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
        end_time.format("%Y-%m-%dT%H:%M:%S%.3fZ")
    )
}

fn build_aggregated_query(
    table_name: &str,
    tag: &str,
    field: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    bucket_ms: u64,
) -> String {
    let sample_by = format_sample_by_interval(bucket_ms);
    // Keep syntax broadly compatible across QuestDB versions.
    format!(
        "SELECT timestamp, avg(value) FROM {} WHERE tag = '{}' AND field = '{}' AND timestamp >= '{}' AND timestamp < '{}' SAMPLE BY {}",
        table_name,
        tag,
        field,
        start_time.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
        end_time.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
        sample_by,
    )
}

fn round_questdb_value(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
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
    options: HistoricalQueryOptions,
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

        let aggregated_query = options
            .bucket_ms
            .filter(|v| *v > 0)
            .map(|bucket_ms| {
                build_aggregated_query(
                    &config.table_name,
                    tag,
                    field,
                    start_time,
                    end_time,
                    bucket_ms,
                )
            });

        let raw_query = build_raw_query(
            &config.table_name,
            tag,
            field,
            start_time,
            end_time,
        );

        let query = aggregated_query.clone().unwrap_or_else(|| raw_query.clone());

        // Query QuestDB REST API
        let url = format!("http://{}:{}/exec", config.host, config.rest_port);
        let response = client
            .get(&url)
            .query(&[("query", &query)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<no body>".to_string());

            // If aggregated query fails, fall back to raw points for compatibility.
            if aggregated_query.is_some() {
                eprintln!(
                    "⚠️ QuestDB aggregated query failed for {}: {} | {}. Retrying raw.",
                    node_id, status, body
                );

                let retry = client
                    .get(&url)
                    .query(&[("query", &raw_query)])
                    .send()
                    .await?;

                if !retry.status().is_success() {
                    let retry_status = retry.status();
                    let retry_body = retry
                        .text()
                        .await
                        .unwrap_or_else(|_| "<no body>".to_string());
                    eprintln!(
                        "⚠️ QuestDB raw retry failed for {}: {} | {}",
                        node_id, retry_status, retry_body
                    );
                    continue;
                }

                let json: serde_json::Value = retry.json().await?;
                let mut data_points = Vec::new();

                if let Some(dataset) = json["dataset"].as_array() {
                    for row in dataset {
                        if let Some(arr) = row.as_array() {
                            if arr.len() >= 2 {
                                if let (Some(ts), Some(val)) = (arr[0].as_str(), arr[1].as_f64()) {
                                    data_points.push(TimeSeriesPoint {
                                        timestamp: ts.to_string(),
                                        value: round_questdb_value(val),
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

                continue;
            }

            eprintln!("⚠️ QuestDB query failed for {}: {} | {}", node_id, status, body);
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
                                        value: round_questdb_value(val),
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

