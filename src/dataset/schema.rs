use std::fs;
use std::path::Path;

use duckdb::Connection;

use crate::error::storage_error::StorageError;

pub const MARKET_DATA_SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS raw_liquidation_events (
  event_id BIGINT,
  mode VARCHAR NOT NULL,
  symbol VARCHAR NOT NULL,
  event_time TIMESTAMP NOT NULL,
  receive_time TIMESTAMP NOT NULL,
  force_side VARCHAR NOT NULL,
  price DOUBLE NOT NULL,
  qty DOUBLE NOT NULL,
  notional DOUBLE NOT NULL,
  raw_payload VARCHAR NOT NULL
);

CREATE TABLE IF NOT EXISTS raw_book_ticker (
  tick_id BIGINT,
  mode VARCHAR NOT NULL,
  symbol VARCHAR NOT NULL,
  event_time TIMESTAMP NOT NULL,
  receive_time TIMESTAMP NOT NULL,
  bid DOUBLE NOT NULL,
  bid_qty DOUBLE NOT NULL,
  ask DOUBLE NOT NULL,
  ask_qty DOUBLE NOT NULL
);

CREATE TABLE IF NOT EXISTS raw_agg_trades (
  trade_id BIGINT,
  mode VARCHAR NOT NULL,
  symbol VARCHAR NOT NULL,
  event_time TIMESTAMP NOT NULL,
  receive_time TIMESTAMP NOT NULL,
  price DOUBLE NOT NULL,
  qty DOUBLE NOT NULL,
  is_buyer_maker BOOLEAN NOT NULL
);

CREATE VIEW IF NOT EXISTS derived_kline_1s AS
SELECT
  mode,
  symbol,
  date_trunc('second', event_time) AS open_time,
  date_trunc('second', event_time) + INTERVAL 1 SECOND AS close_time,
  arg_min(price, event_time) AS open,
  max(price) AS high,
  min(price) AS low,
  arg_max(price, event_time) AS close,
  sum(qty) AS volume,
  sum(price * qty) AS quote_volume,
  count(*) AS trade_count
FROM raw_agg_trades
GROUP BY 1, 2, 3;

CREATE TABLE IF NOT EXISTS recorder_checkpoints (
  stream_name VARCHAR NOT NULL,
  mode VARCHAR NOT NULL,
  symbol VARCHAR NOT NULL,
  last_event_time TIMESTAMP,
  last_updated_at TIMESTAMP NOT NULL
);
"#;

pub fn init_schema_for_path(db_path: &Path) -> Result<(), StorageError> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|error| StorageError::DatabaseInitFailed {
            path: parent.display().to_string(),
            message: error.to_string(),
        })?;
    }
    let connection =
        Connection::open(db_path).map_err(|error| StorageError::DatabaseInitFailed {
            path: db_path.display().to_string(),
            message: error.to_string(),
        })?;
    connection
        .execute_batch(MARKET_DATA_SCHEMA_SQL)
        .map_err(|error| StorageError::DatabaseInitFailed {
            path: db_path.display().to_string(),
            message: error.to_string(),
        })?;
    Ok(())
}
