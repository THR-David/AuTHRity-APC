# QuestDB Historian Setup (code-derived)

## Installation

QuestDB is an external dependency; install and run it separately.

## Configuration

HMI settings are in `hmi/config/settings.toml`:

```toml
[historian]
enabled = true               # Set to false to disable recording
host = "localhost"
ilp_port = 9009             # ILP ingress (write)
http_port = 9000            # Legacy/console port (if used by your QuestDB build)
rest_port = 9000            # REST query port (used by /api/trends)
table_name = "process_data"
batch_size = 100            # Flush after N records
flush_interval_ms = 1000    # Or flush every X ms
snapshot_interval_sec = 60  # Periodic full-tag snapshot (ReadAll)

[historian.deadband]
enabled = true
absolute_default = 0.005        # Minimum absolute change to log
relative_percent_default = 0.1  # Relative deadband (% of current magnitude)
max_silence_sec = 300           # Force write at least every N seconds

[historian.deadband.field_overrides.Mode]
absolute = 1.0
relative_percent = 0.0
```

## Data Schema (what the historian writes)

**Table: `process_data`**

| Column | Type | Description |
|--------|------|-------------|
| `timestamp` | TIMESTAMP | Server timestamp (QuestDB assigns) |
| `tag` | SYMBOL | Node prefix (e.g., "TI1") |
| `field` | SYMBOL | Node suffix (e.g., "PV", "Target", "OP") |
| `value` | DOUBLE/BOOL/LONG | Numeric or boolean values |
| `value_str` | STRING | String values |
| `value_array` | STRING | JSON array (predictions/future plan) |
| `status` | LONG | OPC UA status code |

## Verify Data

Open QuestDB Console: http://localhost:9000

```sql
-- Check recent data
SELECT * FROM process_data 
ORDER BY timestamp DESC 
LIMIT 100;

-- Count records per tag
SELECT tag, COUNT(*) 
FROM process_data 
GROUP BY tag;

-- Query specific variable over time
SELECT timestamp, value 
FROM process_data 
WHERE tag = 'TI1' AND field = 'PV'
ORDER BY timestamp DESC 
LIMIT 1000;
```

## Troubleshooting

**Connection refused:**
- Check QuestDB is running: `docker ps` or task manager
- Verify port 9009 is not blocked by firewall

**No data appearing:**
- Check HMI logs for "Historian: Connected"
- Verify `historian.enabled = true` in settings.toml
- Check QuestDB logs: `/var/lib/questdb/log/`

**High memory usage:**
- Reduce `batch_size` in settings
- Increase `flush_interval_ms` to batch less frequently
