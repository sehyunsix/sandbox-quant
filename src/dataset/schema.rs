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

CREATE TABLE IF NOT EXISTS raw_klines (
  kline_id BIGINT,
  mode VARCHAR NOT NULL,
  product VARCHAR NOT NULL,
  symbol VARCHAR NOT NULL,
  interval VARCHAR NOT NULL,
  open_time TIMESTAMP NOT NULL,
  close_time TIMESTAMP NOT NULL,
  open DOUBLE NOT NULL,
  high DOUBLE NOT NULL,
  low DOUBLE NOT NULL,
  close DOUBLE NOT NULL,
  volume DOUBLE NOT NULL,
  quote_volume DOUBLE NOT NULL,
  trade_count BIGINT NOT NULL,
  taker_buy_base_volume DOUBLE,
  taker_buy_quote_volume DOUBLE,
  raw_payload VARCHAR NOT NULL
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

CREATE TABLE IF NOT EXISTS backtest_runs (
  run_id BIGINT PRIMARY KEY,
  created_at TIMESTAMP NOT NULL,
  mode VARCHAR NOT NULL,
  template VARCHAR NOT NULL,
  instrument VARCHAR NOT NULL,
  from_date DATE NOT NULL,
  to_date DATE NOT NULL,
  db_path VARCHAR NOT NULL,
  liquidation_events BIGINT NOT NULL,
  book_ticker_events BIGINT NOT NULL,
  agg_trade_events BIGINT NOT NULL,
  derived_kline_1s_bars BIGINT NOT NULL,
  trigger_count BIGINT NOT NULL,
  closed_trades BIGINT NOT NULL,
  open_trades BIGINT NOT NULL,
  wins BIGINT NOT NULL,
  losses BIGINT NOT NULL,
  skipped_triggers BIGINT NOT NULL,
  starting_equity DOUBLE NOT NULL,
  ending_equity DOUBLE NOT NULL,
  net_pnl DOUBLE NOT NULL,
  observed_win_rate DOUBLE NOT NULL,
  average_net_pnl DOUBLE NOT NULL,
  configured_expected_value DOUBLE NOT NULL,
  risk_pct DOUBLE NOT NULL,
  win_rate_assumption DOUBLE NOT NULL,
  r_multiple DOUBLE NOT NULL,
  max_entry_slippage_pct DOUBLE NOT NULL,
  stop_distance_pct DOUBLE NOT NULL
);

CREATE TABLE IF NOT EXISTS backtest_trades (
  run_id BIGINT NOT NULL,
  trade_id BIGINT NOT NULL,
  trigger_time TIMESTAMP NOT NULL,
  entry_time TIMESTAMP NOT NULL,
  entry_price DOUBLE NOT NULL,
  stop_price DOUBLE NOT NULL,
  take_profit_price DOUBLE NOT NULL,
  qty DOUBLE NOT NULL,
  exit_time TIMESTAMP,
  exit_price DOUBLE,
  exit_reason VARCHAR,
  gross_pnl DOUBLE,
  fees DOUBLE,
  net_pnl DOUBLE,
  PRIMARY KEY (run_id, trade_id)
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
