use std::path::Path;

use chrono::{NaiveDate, TimeZone, Utc};
use duckdb::{params, Connection as DuckConnection};
use postgres::{Client, GenericClient, NoTls};

use crate::app::bootstrap::BinanceMode;
use crate::backtest_app::runner::{BacktestReport, BacktestTrade};
use crate::dataset::schema::{init_schema_for_path, MARKET_DATA_SCHEMA_VERSION};
use crate::error::storage_error::StorageError;
use crate::record::coordination::RecorderCoordination;

pub const POSTGRES_MARKET_DATA_SCHEMA_VERSION: &str = MARKET_DATA_SCHEMA_VERSION;
pub const SHARED_MARKET_DATA_MODE: &str = "shared";

const POSTGRES_MARKET_DATA_SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS schema_metadata (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS raw_liquidation_events (
  event_id BIGSERIAL PRIMARY KEY,
  product TEXT NOT NULL,
  symbol TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  receive_time TIMESTAMPTZ NOT NULL,
  force_side TEXT NOT NULL,
  price DOUBLE PRECISION NOT NULL,
  qty DOUBLE PRECISION NOT NULL,
  notional DOUBLE PRECISION NOT NULL,
  raw_payload TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_liquidation_events_natural_idx
ON raw_liquidation_events (product, symbol, event_time, force_side, price, qty);

CREATE TABLE IF NOT EXISTS raw_book_ticker (
  tick_id BIGSERIAL PRIMARY KEY,
  symbol TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  receive_time TIMESTAMPTZ NOT NULL,
  bid DOUBLE PRECISION NOT NULL,
  bid_qty DOUBLE PRECISION NOT NULL,
  ask DOUBLE PRECISION NOT NULL,
  ask_qty DOUBLE PRECISION NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_book_ticker_natural_idx
ON raw_book_ticker (symbol, event_time, bid, ask, bid_qty, ask_qty);

CREATE TABLE IF NOT EXISTS raw_agg_trades (
  trade_id BIGSERIAL PRIMARY KEY,
  symbol TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  receive_time TIMESTAMPTZ NOT NULL,
  price DOUBLE PRECISION NOT NULL,
  qty DOUBLE PRECISION NOT NULL,
  is_buyer_maker BOOLEAN NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_agg_trades_natural_idx
ON raw_agg_trades (symbol, event_time, price, qty, is_buyer_maker);

CREATE TABLE IF NOT EXISTS raw_klines (
  kline_id BIGSERIAL PRIMARY KEY,
  product TEXT NOT NULL,
  symbol TEXT NOT NULL,
  interval_name TEXT NOT NULL,
  open_time TIMESTAMPTZ NOT NULL,
  close_time TIMESTAMPTZ NOT NULL,
  open DOUBLE PRECISION NOT NULL,
  high DOUBLE PRECISION NOT NULL,
  low DOUBLE PRECISION NOT NULL,
  close DOUBLE PRECISION NOT NULL,
  volume DOUBLE PRECISION NOT NULL,
  quote_volume DOUBLE PRECISION NOT NULL,
  trade_count BIGINT NOT NULL,
  taker_buy_base_volume DOUBLE PRECISION,
  taker_buy_quote_volume DOUBLE PRECISION,
  raw_payload TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_klines_natural_idx
ON raw_klines (product, symbol, interval_name, open_time);

CREATE INDEX IF NOT EXISTS raw_klines_chart_idx
ON raw_klines (interval_name, symbol, open_time)
INCLUDE (close, close_time, volume);

CREATE TABLE IF NOT EXISTS backtest_runs (
  export_run_id BIGSERIAL PRIMARY KEY,
  source_run_id BIGINT NOT NULL,
  exported_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
  mode TEXT NOT NULL,
  template TEXT NOT NULL,
  instrument TEXT NOT NULL,
  from_date DATE NOT NULL,
  to_date DATE NOT NULL,
  source_db_path TEXT NOT NULL,
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
  starting_equity DOUBLE PRECISION NOT NULL,
  ending_equity DOUBLE PRECISION NOT NULL,
  net_pnl DOUBLE PRECISION NOT NULL,
  observed_win_rate DOUBLE PRECISION NOT NULL,
  average_net_pnl DOUBLE PRECISION NOT NULL,
  configured_expected_value DOUBLE PRECISION NOT NULL,
  risk_pct DOUBLE PRECISION NOT NULL,
  win_rate_assumption DOUBLE PRECISION NOT NULL,
  r_multiple DOUBLE PRECISION NOT NULL,
  max_entry_slippage_pct DOUBLE PRECISION NOT NULL,
  stop_distance_pct DOUBLE PRECISION NOT NULL
);

CREATE INDEX IF NOT EXISTS backtest_runs_mode_lookup_idx
ON backtest_runs (mode, instrument, template, export_run_id DESC);

CREATE TABLE IF NOT EXISTS backtest_trades (
  export_run_id BIGINT NOT NULL REFERENCES backtest_runs (export_run_id) ON DELETE CASCADE,
  trade_id BIGINT NOT NULL,
  trigger_time TIMESTAMPTZ NOT NULL,
  entry_time TIMESTAMPTZ NOT NULL,
  entry_price DOUBLE PRECISION NOT NULL,
  stop_price DOUBLE PRECISION NOT NULL,
  take_profit_price DOUBLE PRECISION NOT NULL,
  qty DOUBLE PRECISION NOT NULL,
  exit_time TIMESTAMPTZ,
  exit_price DOUBLE PRECISION,
  exit_reason TEXT,
  gross_pnl DOUBLE PRECISION,
  fees DOUBLE PRECISION,
  net_pnl DOUBLE PRECISION,
  PRIMARY KEY (export_run_id, trade_id)
);

CREATE INDEX IF NOT EXISTS backtest_trades_exit_lookup_idx
ON backtest_trades (export_run_id, exit_time);

CREATE TABLE IF NOT EXISTS backtest_equity_points (
  export_run_id BIGINT NOT NULL REFERENCES backtest_runs (export_run_id) ON DELETE CASCADE,
  point_id BIGINT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  equity DOUBLE PRECISION NOT NULL,
  cumulative_net_pnl DOUBLE PRECISION NOT NULL,
  PRIMARY KEY (export_run_id, point_id)
);

CREATE INDEX IF NOT EXISTS backtest_equity_points_time_lookup_idx
ON backtest_equity_points (export_run_id, event_time);
"#;

const POSTGRES_MARKET_DATA_MODELESS_MIGRATION_SQL: &str = r#"
DROP INDEX IF EXISTS raw_liquidation_events_natural_idx;
DROP INDEX IF EXISTS raw_book_ticker_natural_idx;
DROP INDEX IF EXISTS raw_agg_trades_natural_idx;
DROP INDEX IF EXISTS raw_klines_natural_idx;
DROP INDEX IF EXISTS raw_klines_chart_idx;

ALTER TABLE IF EXISTS raw_liquidation_events DROP COLUMN IF EXISTS mode CASCADE;
ALTER TABLE IF EXISTS raw_book_ticker DROP COLUMN IF EXISTS mode CASCADE;
ALTER TABLE IF EXISTS raw_agg_trades DROP COLUMN IF EXISTS mode CASCADE;
ALTER TABLE IF EXISTS raw_klines DROP COLUMN IF EXISTS mode CASCADE;

DELETE FROM raw_klines a
USING raw_klines b
WHERE a.kline_id > b.kline_id
  AND a.product = b.product
  AND a.symbol = b.symbol
  AND a.interval_name = b.interval_name
  AND a.open_time = b.open_time;

CREATE UNIQUE INDEX IF NOT EXISTS raw_liquidation_events_natural_idx
ON raw_liquidation_events (product, symbol, event_time, force_side, price, qty);

CREATE UNIQUE INDEX IF NOT EXISTS raw_book_ticker_natural_idx
ON raw_book_ticker (symbol, event_time, bid, ask, bid_qty, ask_qty);

CREATE UNIQUE INDEX IF NOT EXISTS raw_agg_trades_natural_idx
ON raw_agg_trades (symbol, event_time, price, qty, is_buyer_maker);

CREATE UNIQUE INDEX IF NOT EXISTS raw_klines_natural_idx
ON raw_klines (product, symbol, interval_name, open_time);

CREATE INDEX IF NOT EXISTS raw_klines_chart_idx
ON raw_klines (interval_name, symbol, open_time)
INCLUDE (close, close_time, volume);
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectorStorageBackend {
    DuckDb,
    Postgres,
}

impl CollectorStorageBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DuckDb => "duckdb",
            Self::Postgres => "postgres",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostgresKlineRecord {
    pub product: String,
    pub symbol: String,
    pub interval_name: String,
    pub open_time_ms: i64,
    pub close_time_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub quote_volume: f64,
    pub trade_count: i64,
    pub taker_buy_base_volume: Option<f64>,
    pub taker_buy_quote_volume: Option<f64>,
    pub raw_payload: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostgresLiquidationRecord {
    pub product: String,
    pub symbol: String,
    pub event_time_ms: i64,
    pub receive_time_ms: i64,
    pub force_side: String,
    pub price: f64,
    pub qty: f64,
    pub notional: f64,
    pub raw_payload: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostgresBookTickerRecord {
    pub symbol: String,
    pub event_time_ms: i64,
    pub receive_time_ms: i64,
    pub bid: f64,
    pub bid_qty: f64,
    pub ask: f64,
    pub ask_qty: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostgresAggTradeRecord {
    pub symbol: String,
    pub event_time_ms: i64,
    pub receive_time_ms: i64,
    pub price: f64,
    pub qty: f64,
    pub is_buyer_maker: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostgresKlineSummaryRow {
    pub product: String,
    pub symbol: String,
    pub interval_name: String,
    pub row_count: i64,
    pub min_time: Option<String>,
    pub max_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostgresLiquidationSummaryRow {
    pub symbol: String,
    pub row_count: i64,
    pub min_time: Option<String>,
    pub max_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostgresSummary {
    pub schema_version: String,
    pub previous_version: Option<String>,
    pub klines: Vec<PostgresKlineSummaryRow>,
    pub liquidations: Vec<PostgresLiquidationSummaryRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostgresToDuckDbSnapshotConfig {
    pub postgres_url: String,
    pub mode: BinanceMode,
    pub base_dir: String,
    pub symbols: Vec<String>,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub product: Option<String>,
    pub interval_name: Option<String>,
    pub include_klines: bool,
    pub include_liquidations: bool,
    pub include_book_tickers: bool,
    pub include_agg_trades: bool,
    pub clear_duckdb_range: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresToDuckDbSnapshotReport {
    pub snapshot_export_id: i64,
    pub db_path: String,
    pub kline_rows: usize,
    pub liquidation_rows: usize,
    pub book_ticker_rows: usize,
    pub agg_trade_rows: usize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PostgresMarketFreshness {
    pub last_liquidation_event_time: Option<String>,
    pub liquidation_age_sec: Option<i64>,
    pub last_book_ticker_event_time: Option<String>,
    pub book_ticker_age_sec: Option<i64>,
    pub last_agg_trade_event_time: Option<String>,
    pub agg_trade_age_sec: Option<i64>,
    pub last_kline_close_time: Option<String>,
    pub kline_age_sec: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
struct BacktestEquityPoint {
    point_id: i64,
    event_time_ms: i64,
    equity: f64,
    cumulative_net_pnl: f64,
}

pub fn postgres_url_from_env() -> Result<String, StorageError> {
    if let Some(url) = read_postgres_url_from_runtime_env() {
        return Ok(url);
    }

    let dotenv_values = read_postgres_values_from_dotenv();
    if let Some(url) = build_postgres_url_from_values(&dotenv_values) {
        return Ok(url);
    }

    Err(StorageError::WriteFailedWithContext {
        message: "missing PostgreSQL URL; set SANDBOX_QUANT_POSTGRES_URL, DATABASE_URL, or POSTGRES_* runtime vars".to_string(),
    })
}

fn read_postgres_url_from_runtime_env() -> Option<String> {
    std::env::var("SANDBOX_QUANT_POSTGRES_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("DATABASE_URL")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            let values = read_postgres_values_from_env();
            build_postgres_url_from_values(&values)
        })
}

fn read_postgres_values_from_env() -> Vec<(String, String)> {
    [
        "POSTGRES_USER",
        "POSTGRES_PASSWORD",
        "POSTGRES_PORT",
        "POSTGRES_DB",
        "POSTGRES_PUBLISH_ADDRESS",
        "POSTGRES_HOST",
    ]
    .into_iter()
    .filter_map(|key| std::env::var(key).ok().map(|value| (key.to_string(), value)))
    .collect()
}

fn read_postgres_values_from_dotenv() -> Vec<(String, String)> {
    dotenv_candidates()
        .into_iter()
        .find_map(|path| read_selected_dotenv_values(&path))
        .unwrap_or_default()
}

fn dotenv_candidates() -> Vec<std::path::PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![
        Path::new(".env").to_path_buf(),
        manifest_dir.join(".env"),
        Path::new("ops/grafana/.env").to_path_buf(),
        manifest_dir.join("ops/grafana/.env"),
    ]
}

fn read_selected_dotenv_values(path: &Path) -> Option<Vec<(String, String)>> {
    if !path.exists() {
        return None;
    }
    let entries = dotenvy::from_path_iter(path).ok()?;
    let allowed = [
        "SANDBOX_QUANT_POSTGRES_URL",
        "DATABASE_URL",
        "POSTGRES_USER",
        "POSTGRES_PASSWORD",
        "POSTGRES_PORT",
        "POSTGRES_DB",
        "POSTGRES_PUBLISH_ADDRESS",
        "POSTGRES_HOST",
    ];
    let values = entries
        .filter_map(Result::ok)
        .filter(|(key, _)| allowed.contains(&key.as_str()))
        .collect::<Vec<_>>();
    Some(values)
}

fn build_postgres_url_from_values(values: &[(String, String)]) -> Option<String> {
    let direct_url = find_value(values, "SANDBOX_QUANT_POSTGRES_URL")
        .or_else(|| find_value(values, "DATABASE_URL"));
    if let Some(url) = direct_url.filter(|value| !value.trim().is_empty()) {
        return Some(url);
    }

    let user = find_value(values, "POSTGRES_USER")?;
    let password = find_value(values, "POSTGRES_PASSWORD")?;
    let database = find_value(values, "POSTGRES_DB")?;
    let host = find_value(values, "POSTGRES_HOST")
        .or_else(|| find_value(values, "POSTGRES_PUBLISH_ADDRESS"))
        .map(normalize_postgres_host)
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = find_value(values, "POSTGRES_PORT").unwrap_or_else(|| "5432".to_string());

    Some(format!(
        "postgres://{}:{}@{}:{}/{}",
        user, password, host, port, database
    ))
}

fn find_value(values: &[(String, String)], key: &str) -> Option<String> {
    values.iter().find_map(|(entry_key, entry_value)| {
        if entry_key == key {
            Some(entry_value.clone())
        } else {
            None
        }
    })
}

fn normalize_postgres_host(host: String) -> String {
    match host.trim() {
        "" | "0.0.0.0" => "127.0.0.1".to_string(),
        value => value.to_string(),
    }
}

pub fn connect(url: &str) -> Result<Client, StorageError> {
    Client::connect(url, NoTls).map_err(|error| StorageError::DatabaseInitFailed {
        path: mask_postgres_url(url),
        message: error.to_string(),
    })
}

pub fn init_schema(client: &mut Client, url: &str) -> Result<Option<String>, StorageError> {
    let previous_version = existing_schema_version(client)?;
    client
        .batch_execute(POSTGRES_MARKET_DATA_SCHEMA_SQL)
        .map_err(|error| StorageError::DatabaseInitFailed {
            path: mask_postgres_url(url),
            message: error.to_string(),
        })?;
    client
        .batch_execute(POSTGRES_MARKET_DATA_MODELESS_MIGRATION_SQL)
        .map_err(|error| StorageError::DatabaseInitFailed {
            path: mask_postgres_url(url),
            message: error.to_string(),
        })?;
    client
        .execute(
            "INSERT INTO schema_metadata (key, value, updated_at)
             VALUES ('market_data_schema_version', $1, CURRENT_TIMESTAMP)
             ON CONFLICT (key) DO UPDATE
             SET value = EXCLUDED.value, updated_at = EXCLUDED.updated_at",
            &[&POSTGRES_MARKET_DATA_SCHEMA_VERSION],
        )
        .map_err(|error| StorageError::DatabaseInitFailed {
            path: mask_postgres_url(url),
            message: error.to_string(),
        })?;
    Ok(previous_version)
}

pub fn ensure_recorder_schema_ready(client: &mut Client, url: &str) -> Result<(), StorageError> {
    for table in [
        "raw_liquidation_events",
        "raw_book_ticker",
        "raw_agg_trades",
    ] {
        let exists = client
            .query_one(
                "SELECT EXISTS (
                    SELECT 1
                    FROM information_schema.tables
                    WHERE table_schema = 'public' AND table_name = $1
                )",
                &[&table],
            )
            .map_err(|error| StorageError::DatabaseInitFailed {
                path: mask_postgres_url(url),
                message: error.to_string(),
            })?
            .get::<_, bool>(0);
        if !exists {
            return Err(StorageError::DatabaseInitFailed {
                path: mask_postgres_url(url),
                message: format!(
                    "required recorder table missing: {table}; run the PostgreSQL schema bootstrap/migration first"
                ),
            });
        }
    }
    Ok(())
}

pub fn export_backtest_report(
    client: &mut Client,
    report: &BacktestReport,
) -> Result<i64, StorageError> {
    let closed_trades = report
        .trades
        .iter()
        .filter(|trade| trade.net_pnl.is_some())
        .count() as i64;
    let equity_points = build_backtest_equity_points(report)?;
    let mode = report.mode.as_str().to_string();
    let template = report.template.slug().to_string();
    let instrument = report.instrument.clone();
    let source_db_path = report.db_path.display().to_string();
    let source_run_id = match report.run_id {
        Some(run_id) => run_id,
        None => next_backtest_source_run_id(client, &source_db_path)?,
    };
    let mut transaction = client.transaction().map_err(storage_err)?;
    let export_run_id: i64 = transaction
        .query_one(
            "INSERT INTO backtest_runs (
                source_run_id, exported_at, mode, template, instrument, from_date, to_date,
                source_db_path, liquidation_events, book_ticker_events, agg_trade_events,
                derived_kline_1s_bars, trigger_count, closed_trades, open_trades, wins,
                losses, skipped_triggers, starting_equity, ending_equity, net_pnl,
                observed_win_rate, average_net_pnl, configured_expected_value, risk_pct,
                win_rate_assumption, r_multiple, max_entry_slippage_pct, stop_distance_pct
             ) VALUES (
                $1, CURRENT_TIMESTAMP, $2, $3, $4, $5, $6,
                $7, $8, $9, $10,
                $11, $12, $13, $14, $15,
                $16, $17, $18, $19, $20,
                $21, $22, $23, $24,
                $25, $26, $27, $28
             )
             RETURNING export_run_id",
            &[
                &source_run_id,
                &mode,
                &template,
                &instrument,
                &report.from,
                &report.to,
                &source_db_path,
                &(report.dataset.liquidation_events as i64),
                &(report.dataset.book_ticker_events as i64),
                &(report.dataset.agg_trade_events as i64),
                &(report.dataset.derived_kline_1s_bars as i64),
                &(report.trigger_count as i64),
                &closed_trades,
                &(report.open_trades as i64),
                &(report.wins as i64),
                &(report.losses as i64),
                &(report.skipped_triggers as i64),
                &report.starting_equity,
                &report.ending_equity,
                &report.net_pnl,
                &report.observed_win_rate,
                &report.average_net_pnl,
                &report.configured_expected_value,
                &report.config.risk_pct,
                &report.config.win_rate_assumption,
                &report.config.r_multiple,
                &report.config.max_entry_slippage_pct,
                &report.config.stop_distance_pct,
            ],
        )
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: format!("insert backtest_runs failed: {error}"),
        })?
        .get(0);
    for trade in &report.trades {
        insert_backtest_trade(&mut transaction, export_run_id, trade)?;
    }
    for point in equity_points {
        insert_backtest_equity_point(&mut transaction, export_run_id, &point)?;
    }
    transaction.commit().map_err(storage_err)?;
    Ok(export_run_id)
}

pub fn insert_kline(client: &mut Client, record: &PostgresKlineRecord) -> Result<(), StorageError> {
    let open_time = timestamp_from_millis(record.open_time_ms)?;
    let close_time = timestamp_from_millis(record.close_time_ms)?;
    client
        .execute(
            "INSERT INTO raw_klines (
                product, symbol, interval_name, open_time, close_time,
                open, high, low, close, volume, quote_volume, trade_count,
                taker_buy_base_volume, taker_buy_quote_volume, raw_payload
             ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10, $11, $12, $13, $14, $15
             )
             ON CONFLICT (product, symbol, interval_name, open_time) DO NOTHING",
            &[
                &record.product,
                &record.symbol,
                &record.interval_name,
                &open_time,
                &close_time,
                &record.open,
                &record.high,
                &record.low,
                &record.close,
                &record.volume,
                &record.quote_volume,
                &record.trade_count,
                &record.taker_buy_base_volume,
                &record.taker_buy_quote_volume,
                &record.raw_payload,
            ],
        )
        .map(|_| ())
        .map_err(storage_err)
}

pub fn insert_liquidation(
    client: &mut Client,
    record: &PostgresLiquidationRecord,
) -> Result<(), StorageError> {
    let event_time = timestamp_from_millis(record.event_time_ms)?;
    let receive_time = timestamp_from_millis(record.receive_time_ms)?;
    client
        .execute(
            "INSERT INTO raw_liquidation_events (
                product, symbol, event_time, receive_time, force_side, price, qty, notional, raw_payload
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9
             )
             ON CONFLICT (product, symbol, event_time, force_side, price, qty) DO NOTHING",
            &[
                &record.product,
                &record.symbol,
                &event_time,
                &receive_time,
                &record.force_side,
                &record.price,
                &record.qty,
                &record.notional,
                &record.raw_payload,
            ],
        )
        .map(|_| ())
        .map_err(storage_err)
}

pub fn insert_book_ticker(
    client: &mut Client,
    record: &PostgresBookTickerRecord,
) -> Result<(), StorageError> {
    let event_time = timestamp_from_millis(record.event_time_ms)?;
    let receive_time = timestamp_from_millis(record.receive_time_ms)?;
    client
        .execute(
            "INSERT INTO raw_book_ticker (
                symbol, event_time, receive_time, bid, bid_qty, ask, ask_qty
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7
             )
             ON CONFLICT (symbol, event_time, bid, ask, bid_qty, ask_qty) DO NOTHING",
            &[
                &record.symbol,
                &event_time,
                &receive_time,
                &record.bid,
                &record.bid_qty,
                &record.ask,
                &record.ask_qty,
            ],
        )
        .map(|_| ())
        .map_err(storage_err)
}

pub fn insert_agg_trade(
    client: &mut Client,
    record: &PostgresAggTradeRecord,
) -> Result<(), StorageError> {
    let event_time = timestamp_from_millis(record.event_time_ms)?;
    let receive_time = timestamp_from_millis(record.receive_time_ms)?;
    client
        .execute(
            "INSERT INTO raw_agg_trades (
                symbol, event_time, receive_time, price, qty, is_buyer_maker
             ) VALUES (
                $1, $2, $3, $4, $5, $6
             )
             ON CONFLICT (symbol, event_time, price, qty, is_buyer_maker) DO NOTHING",
            &[
                &record.symbol,
                &event_time,
                &receive_time,
                &record.price,
                &record.qty,
                &record.is_buyer_maker,
            ],
        )
        .map(|_| ())
        .map_err(storage_err)
}

pub fn metrics_for_postgres_url(
    url: &str,
) -> Result<crate::dataset::types::RecorderMetrics, StorageError> {
    let mut client = connect(url)?;
    ensure_recorder_schema_ready(&mut client, url)?;
    Ok(crate::dataset::types::RecorderMetrics {
        liquidation_events: query_count(&mut client, "raw_liquidation_events")?,
        book_ticker_events: query_count(&mut client, "raw_book_ticker")?,
        agg_trade_events: query_count(&mut client, "raw_agg_trades")?,
        derived_kline_1s_bars: 0,
        schema_version: existing_schema_version(&mut client)?,
        last_liquidation_event_time: query_latest_timestamp(
            &mut client,
            "raw_liquidation_events",
            "event_time",
        )?,
        last_book_ticker_event_time: query_latest_timestamp(
            &mut client,
            "raw_book_ticker",
            "event_time",
        )?,
        last_agg_trade_event_time: query_latest_timestamp(
            &mut client,
            "raw_agg_trades",
            "event_time",
        )?,
        top_liquidation_symbols: query_top_symbols(&mut client, "raw_liquidation_events")?,
        top_book_ticker_symbols: query_top_symbols(&mut client, "raw_book_ticker")?,
        top_agg_trade_symbols: query_top_symbols(&mut client, "raw_agg_trades")?,
    })
}

pub fn market_freshness_for_postgres_url(
    url: &str,
    interval_name: &str,
) -> Result<PostgresMarketFreshness, StorageError> {
    let mut client = connect(url)?;
    ensure_recorder_schema_ready(&mut client, url)?;
    let (last_liquidation_event_time, liquidation_age_sec) =
        query_latest_timestamp_and_age(&mut client, "raw_liquidation_events", "event_time")?;
    let (last_book_ticker_event_time, book_ticker_age_sec) =
        query_latest_timestamp_and_age(&mut client, "raw_book_ticker", "event_time")?;
    let (last_agg_trade_event_time, agg_trade_age_sec) =
        query_latest_timestamp_and_age(&mut client, "raw_agg_trades", "event_time")?;
    let (last_kline_close_time, kline_age_sec) =
        query_latest_kline_timestamp_and_age(&mut client, interval_name)?;
    Ok(PostgresMarketFreshness {
        last_liquidation_event_time,
        liquidation_age_sec,
        last_book_ticker_event_time,
        book_ticker_age_sec,
        last_agg_trade_event_time,
        agg_trade_age_sec,
        last_kline_close_time,
        kline_age_sec,
    })
}

pub fn load_summary(
    client: &mut Client,
    previous_version: Option<String>,
) -> Result<PostgresSummary, StorageError> {
    let schema_version = existing_schema_version(client)?.unwrap_or_else(|| "missing".to_string());

    let klines = client
        .query(
            "SELECT product, symbol, interval_name, COUNT(*) AS row_count,
                    CAST(MIN(open_time) AS TEXT), CAST(MAX(close_time) AS TEXT)
             FROM raw_klines
             GROUP BY product, symbol, interval_name
             ORDER BY product, symbol, interval_name",
            &[],
        )
        .map_err(storage_err)?
        .into_iter()
        .map(|row| PostgresKlineSummaryRow {
            product: row.get(0),
            symbol: row.get(1),
            interval_name: row.get(2),
            row_count: row.get(3),
            min_time: row.get(4),
            max_time: row.get(5),
        })
        .collect();

    let liquidations = client
        .query(
            "SELECT symbol, COUNT(*) AS row_count,
                    CAST(MIN(event_time) AS TEXT), CAST(MAX(event_time) AS TEXT)
             FROM raw_liquidation_events
             GROUP BY symbol
             ORDER BY symbol",
            &[],
        )
        .map_err(storage_err)?
        .into_iter()
        .map(|row| PostgresLiquidationSummaryRow {
            symbol: row.get(0),
            row_count: row.get(1),
            min_time: row.get(2),
            max_time: row.get(3),
        })
        .collect();

    Ok(PostgresSummary {
        schema_version,
        previous_version,
        klines,
        liquidations,
    })
}

pub fn latest_shared_kline_open_time_ms(
    client: &mut Client,
    product: &str,
    symbol: &str,
    interval_name: &str,
) -> Result<Option<i64>, StorageError> {
    client
        .query_opt(
            "SELECT (EXTRACT(EPOCH FROM MAX(open_time)) * 1000)::BIGINT
             FROM raw_klines
             WHERE product = $1 AND symbol = $2 AND interval_name = $3",
            &[&product, &symbol, &interval_name],
        )
        .map_err(storage_err)
        .map(|row| row.and_then(|row| row.get::<_, Option<i64>>(0)))
}

pub fn backtest_summary_for_postgres_url(
    url: &str,
    _mode: BinanceMode,
    symbol: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<crate::dataset::types::BacktestDatasetSummary, StorageError> {
    let mut client = connect(url)?;
    init_schema(&mut client, url)?;
    let symbol_text = symbol.to_string();
    let from_time_ms = from
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| StorageError::WriteFailedWithContext {
            message: format!("invalid backtest from date: {from}"),
        })?
        .and_utc()
        .timestamp_millis() as f64;
    let to_time_ms = to
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| StorageError::WriteFailedWithContext {
            message: format!("invalid backtest to date: {to}"),
        })?
        .and_utc()
        .timestamp_millis() as f64;
    let symbol_found = client
        .query_one(
            "SELECT EXISTS(
                SELECT 1 FROM raw_klines WHERE symbol = $1
            )",
            &[&symbol_text],
        )
        .map_err(storage_err)?
        .get::<_, bool>(0);

    let kline_count = postgres_count_in_range(
        &mut client,
        "raw_klines",
        "open_time",
        &symbol_text,
        from_time_ms,
        to_time_ms,
    )?;

    Ok(crate::dataset::types::BacktestDatasetSummary {
        mode: _mode,
        symbol: symbol.to_string(),
        symbol_found,
        from: from.to_string(),
        to: to.to_string(),
        liquidation_events: 0,
        book_ticker_events: 0,
        agg_trade_events: 0,
        derived_kline_1s_bars: kline_count,
    })
}

pub fn load_raw_kline_rows_for_postgres_url(
    url: &str,
    _mode: BinanceMode,
    symbol: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Option<(String, Vec<crate::dataset::types::DerivedKlineRow>)>, StorageError> {
    let mut client = connect(url)?;
    init_schema(&mut client, url)?;
    let symbol_text = symbol.to_string();
    let from_time_ms = from
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| StorageError::WriteFailedWithContext {
            message: format!("invalid backtest from date: {from}"),
        })?
        .and_utc()
        .timestamp_millis() as f64;
    let to_time_ms = to
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| StorageError::WriteFailedWithContext {
            message: format!("invalid backtest to date: {to}"),
        })?
        .and_utc()
        .timestamp_millis() as f64;

    let interval_rows = client
        .query(
            "SELECT DISTINCT interval_name
             FROM raw_klines
             WHERE symbol = $1
               AND open_time >= to_timestamp($2::double precision / 1000.0)
               AND open_time <= to_timestamp($3::double precision / 1000.0)",
            &[&symbol_text, &from_time_ms, &to_time_ms],
        )
        .map_err(storage_err)?;
    let interval = interval_rows
        .into_iter()
        .map(|row| row.get::<_, String>(0))
        .min_by_key(|value| raw_kline_interval_rank(value));
    let Some(interval) = interval else {
        return Ok(None);
    };

    let rows = client
        .query(
            "SELECT
                (EXTRACT(EPOCH FROM open_time) * 1000)::BIGINT AS open_time_ms,
                (EXTRACT(EPOCH FROM close_time) * 1000)::BIGINT AS close_time_ms,
                open, high, low, close, volume, quote_volume, trade_count
             FROM raw_klines
             WHERE symbol = $1
               AND interval_name = $2
               AND open_time >= to_timestamp($3::double precision / 1000.0)
               AND open_time <= to_timestamp($4::double precision / 1000.0)
             ORDER BY open_time ASC",
            &[&symbol_text, &interval, &from_time_ms, &to_time_ms],
        )
        .map_err(storage_err)?;
    let result = rows
        .into_iter()
        .map(|row| crate::dataset::types::DerivedKlineRow {
            open_time_ms: row.get(0),
            close_time_ms: row.get(1),
            open: row.get(2),
            high: row.get(3),
            low: row.get(4),
            close: row.get(5),
            volume: row.get(6),
            quote_volume: row.get(7),
            trade_count: row.get::<_, i64>(8).max(0) as u64,
        })
        .collect::<Vec<_>>();

    Ok(Some((interval, result)))
}

pub fn export_snapshot_to_duckdb(
    config: &PostgresToDuckDbSnapshotConfig,
) -> Result<PostgresToDuckDbSnapshotReport, StorageError> {
    let mut client = connect(&config.postgres_url)?;
    init_schema(&mut client, &config.postgres_url)?;

    let db_path = RecorderCoordination::new(config.base_dir.clone()).db_path(config.mode);
    init_schema_for_path(&db_path)?;
    let duck =
        DuckConnection::open(&db_path).map_err(|error| StorageError::DatabaseInitFailed {
            path: db_path.display().to_string(),
            message: error.to_string(),
        })?;

    let product = config.product.clone();
    let interval_name = config.interval_name.clone();
    let from_ts = format!("{} 00:00:00", config.from);
    let to_ts = format!("{} 23:59:59", config.to);
    let from_time_ms = config
        .from
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| StorageError::WriteFailedWithContext {
            message: format!("invalid snapshot from date: {}", config.from),
        })?
        .and_utc()
        .timestamp_millis() as f64;
    let to_time_ms = config
        .to
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| StorageError::WriteFailedWithContext {
            message: format!("invalid snapshot to date: {}", config.to),
        })?
        .and_utc()
        .timestamp_millis() as f64;

    let mut kline_rows = 0usize;
    let mut liquidation_rows = 0usize;
    let mut book_ticker_rows = 0usize;
    let mut agg_trade_rows = 0usize;

    for symbol in &config.symbols {
        if config.clear_duckdb_range {
            if config.include_klines {
                clear_duckdb_klines(
                    &duck,
                    symbol,
                    config.product.as_deref(),
                    config.interval_name.as_deref(),
                    &from_ts,
                    &to_ts,
                )?;
            }
            if config.include_liquidations {
                clear_duckdb_liquidations(&duck, symbol, &from_ts, &to_ts)?;
            }
            if config.include_book_tickers {
                clear_duckdb_book_tickers(&duck, symbol, &from_ts, &to_ts)?;
            }
            if config.include_agg_trades {
                clear_duckdb_agg_trades(&duck, symbol, &from_ts, &to_ts)?;
            }
        }

        if config.include_klines {
            let rows = client
                .query(
                    "SELECT product, symbol, interval_name,
                            (EXTRACT(EPOCH FROM open_time) * 1000)::BIGINT AS open_time_ms,
                            (EXTRACT(EPOCH FROM close_time) * 1000)::BIGINT AS close_time_ms,
                            open, high, low, close, volume, quote_volume, trade_count,
                            taker_buy_base_volume, taker_buy_quote_volume, raw_payload
                     FROM raw_klines
                     WHERE symbol = $1
                       AND open_time >= to_timestamp($2::double precision / 1000.0)
                       AND open_time <= to_timestamp($3::double precision / 1000.0)
                       AND ($4::TEXT IS NULL OR product = $4)
                       AND ($5::TEXT IS NULL OR interval_name = $5)
                    ORDER BY open_time ASC",
                    &[symbol, &from_time_ms, &to_time_ms, &product, &interval_name],
                )
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: format!("query raw_klines from postgres failed: {error}"),
                })?;

            let mut next_id = next_duckdb_kline_id(&duck)?;
            for row in rows {
                duck.execute(
                    "INSERT INTO raw_klines (
                        kline_id, mode, product, symbol, interval, open_time, close_time,
                        open, high, low, close, volume, quote_volume, trade_count,
                        taker_buy_base_volume, taker_buy_quote_volume, raw_payload
                    ) VALUES (
                        ?, ?, ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0),
                        ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                    )",
                    params![
                        next_id,
                        SHARED_MARKET_DATA_MODE.to_string(),
                        row.get::<_, String>(0),
                        row.get::<_, String>(1),
                        row.get::<_, String>(2),
                        row.get::<_, i64>(3),
                        row.get::<_, i64>(4),
                        row.get::<_, f64>(5),
                        row.get::<_, f64>(6),
                        row.get::<_, f64>(7),
                        row.get::<_, f64>(8),
                        row.get::<_, f64>(9),
                        row.get::<_, f64>(10),
                        row.get::<_, i64>(11),
                        row.get::<_, Option<f64>>(12),
                        row.get::<_, Option<f64>>(13),
                        row.get::<_, String>(14),
                    ],
                )
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: format!("insert raw_klines into duckdb failed: {error}"),
                })?;
                next_id += 1;
                kline_rows += 1;
            }
        }

        if config.include_liquidations {
            let rows = client
                .query(
                    "SELECT symbol,
                            (EXTRACT(EPOCH FROM event_time) * 1000)::BIGINT AS event_time_ms,
                            (EXTRACT(EPOCH FROM receive_time) * 1000)::BIGINT AS receive_time_ms,
                            force_side, price, qty, notional, raw_payload
                     FROM raw_liquidation_events
                     WHERE symbol = $1
                       AND event_time >= to_timestamp($2::double precision / 1000.0)
                       AND event_time <= to_timestamp($3::double precision / 1000.0)
                     ORDER BY event_time ASC",
                    &[symbol, &from_time_ms, &to_time_ms],
                )
                .map_err(storage_err)?;
            let mut next_id = next_duckdb_liquidation_event_id(&duck)?;
            for row in rows {
                duck.execute(
                    "INSERT INTO raw_liquidation_events (
                        event_id, mode, symbol, event_time, receive_time, force_side, price, qty, notional, raw_payload
                     ) VALUES (
                        ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0), ?, ?, ?, ?, ?
                     )",
                    params![
                        next_id,
                        SHARED_MARKET_DATA_MODE.to_string(),
                        row.get::<_, String>(0),
                        row.get::<_, i64>(1),
                        row.get::<_, i64>(2),
                        row.get::<_, String>(3),
                        row.get::<_, f64>(4),
                        row.get::<_, f64>(5),
                        row.get::<_, f64>(6),
                        row.get::<_, String>(7),
                    ],
                )
                .map_err(storage_err)?;
                next_id += 1;
                liquidation_rows += 1;
            }
        }

        if config.include_book_tickers {
            let rows = client
                .query(
                    "SELECT symbol,
                            (EXTRACT(EPOCH FROM event_time) * 1000)::BIGINT AS event_time_ms,
                            (EXTRACT(EPOCH FROM receive_time) * 1000)::BIGINT AS receive_time_ms,
                            bid, bid_qty, ask, ask_qty
                     FROM raw_book_ticker
                     WHERE symbol = $1
                       AND event_time >= to_timestamp($2::double precision / 1000.0)
                       AND event_time <= to_timestamp($3::double precision / 1000.0)
                     ORDER BY event_time ASC",
                    &[symbol, &from_time_ms, &to_time_ms],
                )
                .map_err(storage_err)?;
            let mut next_id = next_duckdb_book_ticker_id(&duck)?;
            for row in rows {
                duck.execute(
                    "INSERT INTO raw_book_ticker (
                        tick_id, mode, symbol, event_time, receive_time, bid, bid_qty, ask, ask_qty
                     ) VALUES (
                        ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0), ?, ?, ?, ?
                     )",
                    params![
                        next_id,
                        SHARED_MARKET_DATA_MODE.to_string(),
                        row.get::<_, String>(0),
                        row.get::<_, i64>(1),
                        row.get::<_, i64>(2),
                        row.get::<_, f64>(3),
                        row.get::<_, f64>(4),
                        row.get::<_, f64>(5),
                        row.get::<_, f64>(6),
                    ],
                )
                .map_err(storage_err)?;
                next_id += 1;
                book_ticker_rows += 1;
            }
        }

        if config.include_agg_trades {
            let rows = client
                .query(
                    "SELECT symbol,
                            (EXTRACT(EPOCH FROM event_time) * 1000)::BIGINT AS event_time_ms,
                            (EXTRACT(EPOCH FROM receive_time) * 1000)::BIGINT AS receive_time_ms,
                            price, qty, is_buyer_maker
                     FROM raw_agg_trades
                     WHERE symbol = $1
                       AND event_time >= to_timestamp($2::double precision / 1000.0)
                       AND event_time <= to_timestamp($3::double precision / 1000.0)
                     ORDER BY event_time ASC",
                    &[symbol, &from_time_ms, &to_time_ms],
                )
                .map_err(storage_err)?;
            let mut next_id = next_duckdb_agg_trade_id(&duck)?;
            for row in rows {
                duck.execute(
                    "INSERT INTO raw_agg_trades (
                        trade_id, mode, symbol, event_time, receive_time, price, qty, is_buyer_maker
                     ) VALUES (
                        ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0), ?, ?, ?
                     )",
                    params![
                        next_id,
                        SHARED_MARKET_DATA_MODE.to_string(),
                        row.get::<_, String>(0),
                        row.get::<_, i64>(1),
                        row.get::<_, i64>(2),
                        row.get::<_, f64>(3),
                        row.get::<_, f64>(4),
                        row.get::<_, bool>(5),
                    ],
                )
                .map_err(storage_err)?;
                next_id += 1;
                agg_trade_rows += 1;
            }
        }
    }

    let snapshot_export_id = next_duckdb_snapshot_export_id(&duck)?;
    duck.execute(
        "INSERT INTO snapshot_exports (
            export_id, created_at, source_backend, source_target, mode, symbols_csv, from_date, to_date,
            product, interval_name, include_klines, include_liquidations, include_book_tickers, include_agg_trades,
            clear_duckdb_range, exported_kline_rows, exported_liquidation_rows, exported_book_ticker_rows, exported_agg_trade_rows
         ) VALUES (
            ?, CURRENT_TIMESTAMP, 'postgres', ?, ?, ?, CAST(? AS DATE), CAST(? AS DATE),
            ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
         )",
        params![
            snapshot_export_id,
            mask_postgres_url(&config.postgres_url),
            config.mode.as_str(),
            config.symbols.join(","),
            config.from.to_string(),
            config.to.to_string(),
            product,
            interval_name,
            config.include_klines,
            config.include_liquidations,
            config.include_book_tickers,
            config.include_agg_trades,
            config.clear_duckdb_range,
            kline_rows as i64,
            liquidation_rows as i64,
            book_ticker_rows as i64,
            agg_trade_rows as i64,
        ],
    )
    .map_err(|error| StorageError::WriteFailedWithContext {
        message: format!("insert snapshot_exports into duckdb failed: {error}"),
    })?;

    Ok(PostgresToDuckDbSnapshotReport {
        snapshot_export_id,
        db_path: db_path.display().to_string(),
        kline_rows,
        liquidation_rows,
        book_ticker_rows,
        agg_trade_rows,
    })
}

fn insert_backtest_trade(
    client: &mut impl GenericClient,
    export_run_id: i64,
    trade: &BacktestTrade,
) -> Result<(), StorageError> {
    let exit_reason = trade
        .exit_reason
        .as_ref()
        .map(|reason| reason.as_str().to_string());
    let trigger_time_ms = trade.trigger_time.timestamp_millis() as f64;
    let entry_time_ms = trade.entry_time.timestamp_millis() as f64;
    let exit_time_ms = trade.exit_time.map(|value| value.timestamp_millis() as f64);
    client
        .execute(
            "INSERT INTO backtest_trades (
                export_run_id, trade_id, trigger_time, entry_time, entry_price, stop_price,
                take_profit_price, qty, exit_time, exit_price, exit_reason, gross_pnl, fees, net_pnl
             ) VALUES (
                $1, $2, to_timestamp($3::double precision / 1000.0), to_timestamp($4::double precision / 1000.0), $5, $6,
                $7, $8, to_timestamp($9::double precision / 1000.0), $10, $11, $12, $13, $14
             )",
                &[
                    &export_run_id,
                    &(trade.trade_id as i64),
                    &trigger_time_ms,
                    &entry_time_ms,
                    &trade.entry_price,
                    &trade.stop_price,
                    &trade.take_profit_price,
                    &trade.qty,
                    &exit_time_ms,
                    &trade.exit_price,
                    &exit_reason,
                    &trade.gross_pnl,
                &trade.fees,
                &trade.net_pnl,
            ],
        )
        .map(|_| ())
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: format!("insert backtest_trades failed: {error:?}"),
        })
}

fn insert_backtest_equity_point(
    client: &mut impl GenericClient,
    export_run_id: i64,
    point: &BacktestEquityPoint,
) -> Result<(), StorageError> {
    let event_time_ms = point.event_time_ms as f64;
    client
        .execute(
            "INSERT INTO backtest_equity_points (
                export_run_id, point_id, event_time, equity, cumulative_net_pnl
             ) VALUES ($1, $2, to_timestamp($3::double precision / 1000.0), $4, $5)",
            &[
                &export_run_id,
                &point.point_id,
                &event_time_ms,
                &point.equity,
                &point.cumulative_net_pnl,
            ],
        )
        .map(|_| ())
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: format!("insert backtest_equity_points failed: {error:?}"),
        })
}

fn build_backtest_equity_points(
    report: &BacktestReport,
) -> Result<Vec<BacktestEquityPoint>, StorageError> {
    let start_time =
        report
            .from
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| StorageError::WriteFailedWithContext {
                message: format!("invalid backtest start date: {}", report.from),
            })?;
    let mut points = vec![BacktestEquityPoint {
        point_id: 0,
        event_time_ms: chrono::DateTime::<Utc>::from_naive_utc_and_offset(start_time, Utc)
            .timestamp_millis(),
        equity: report.starting_equity,
        cumulative_net_pnl: 0.0,
    }];

    let mut realized = report
        .trades
        .iter()
        .filter_map(|trade| Some((trade.exit_time?, trade.net_pnl?)))
        .collect::<Vec<_>>();
    realized.sort_by_key(|(event_time, _)| *event_time);

    let mut cumulative_net_pnl = 0.0;
    let mut point_id = 1i64;
    for (event_time, net_pnl) in realized {
        cumulative_net_pnl += net_pnl;
        points.push(BacktestEquityPoint {
            point_id,
            event_time_ms: event_time.timestamp_millis(),
            equity: report.starting_equity + cumulative_net_pnl,
            cumulative_net_pnl,
        });
        point_id += 1;
    }

    if points.len() == 1 {
        let end_time = report.to.and_hms_opt(23, 59, 59).ok_or_else(|| {
            StorageError::WriteFailedWithContext {
                message: format!("invalid backtest end date: {}", report.to),
            }
        })?;
        points.push(BacktestEquityPoint {
            point_id,
            event_time_ms: chrono::DateTime::<Utc>::from_naive_utc_and_offset(end_time, Utc)
                .timestamp_millis(),
            equity: report.ending_equity,
            cumulative_net_pnl: report.net_pnl,
        });
    }

    Ok(points)
}

fn next_backtest_source_run_id(
    client: &mut Client,
    source_db_path: &str,
) -> Result<i64, StorageError> {
    client
        .query_one(
            "SELECT COALESCE(MAX(source_run_id), 0) + 1
             FROM backtest_runs
             WHERE source_db_path = $1",
            &[&source_db_path],
        )
        .map(|row| row.get(0))
        .map_err(storage_err)
}

fn existing_schema_version(client: &mut Client) -> Result<Option<String>, StorageError> {
    let table_exists = client
        .query_one(
            "SELECT EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_schema = 'public' AND table_name = 'schema_metadata'
            )",
            &[],
        )
        .map_err(storage_err)?
        .get::<_, bool>(0);
    if !table_exists {
        return Ok(None);
    }
    client
        .query_opt(
            "SELECT value FROM schema_metadata WHERE key = 'market_data_schema_version'",
            &[],
        )
        .map_err(storage_err)
        .map(|row| row.map(|row| row.get(0)))
}

fn query_count(client: &mut Client, table: &str) -> Result<u64, StorageError> {
    client
        .query_one(&format!("SELECT COUNT(*) FROM {table}"), &[])
        .map_err(storage_err)
        .map(|row| row.get::<_, i64>(0).max(0) as u64)
}

fn postgres_count_in_range(
    client: &mut Client,
    table: &str,
    time_column: &str,
    symbol: &str,
    from_time_ms: f64,
    to_time_ms: f64,
) -> Result<u64, StorageError> {
    client
        .query_one(
            &format!(
                "SELECT COUNT(*) FROM {table}
                 WHERE symbol = $1
                   AND {time_column} >= to_timestamp($2::double precision / 1000.0)
                   AND {time_column} <= to_timestamp($3::double precision / 1000.0)"
            ),
            &[&symbol, &from_time_ms, &to_time_ms],
        )
        .map_err(storage_err)
        .map(|row| row.get::<_, i64>(0).max(0) as u64)
}

fn raw_kline_interval_rank(interval: &str) -> usize {
    match interval {
        "1m" => 0,
        "3m" => 1,
        "5m" => 2,
        "15m" => 3,
        "30m" => 4,
        "1h" => 5,
        "4h" => 6,
        "1d" => 7,
        "1w" => 8,
        "1mo" => 9,
        _ => usize::MAX,
    }
}

fn query_latest_timestamp(
    client: &mut Client,
    table: &str,
    column: &str,
) -> Result<Option<String>, StorageError> {
    client
        .query_one(
            &format!("SELECT CAST(MAX({column}) AS TEXT) FROM {table}"),
            &[],
        )
        .map_err(storage_err)
        .map(|row| row.get(0))
}

fn query_latest_timestamp_and_age(
    client: &mut Client,
    table: &str,
    column: &str,
) -> Result<(Option<String>, Option<i64>), StorageError> {
    client
        .query_one(
            &format!(
                "SELECT
                    CAST(MAX({column}) AS TEXT),
                    CASE
                        WHEN MAX({column}) IS NULL THEN NULL
                        ELSE CAST(EXTRACT(EPOCH FROM (NOW() - MAX({column}))) AS BIGINT)
                    END
                 FROM {table}"
            ),
            &[],
        )
        .map_err(storage_err)
        .map(|row| (row.get(0), row.get(1)))
}

fn query_latest_kline_timestamp_and_age(
    client: &mut Client,
    interval_name: &str,
) -> Result<(Option<String>, Option<i64>), StorageError> {
    client
        .query_one(
            "SELECT
                CAST(MAX(close_time) AS TEXT),
                CASE
                    WHEN MAX(close_time) IS NULL THEN NULL
                    ELSE CAST(EXTRACT(EPOCH FROM (NOW() - MAX(close_time))) AS BIGINT)
                END
             FROM raw_klines
             WHERE interval_name = $1",
            &[&interval_name],
        )
        .map_err(storage_err)
        .map(|row| (row.get(0), row.get(1)))
}

fn query_top_symbols(client: &mut Client, table: &str) -> Result<Vec<String>, StorageError> {
    client
        .query(
            &format!(
                "SELECT symbol, COUNT(*) AS row_count FROM {table} GROUP BY symbol ORDER BY row_count DESC, symbol ASC LIMIT 5"
            ),
            &[],
        )
        .map_err(storage_err)
        .map(|rows| {
            rows.into_iter()
                .map(|row| format!("{}:{}", row.get::<_, String>(0), row.get::<_, i64>(1)))
                .collect()
        })
}

fn clear_duckdb_klines(
    duck: &DuckConnection,
    symbol: &str,
    product: Option<&str>,
    interval_name: Option<&str>,
    from_ts: &str,
    to_ts: &str,
) -> Result<(), StorageError> {
    duck.execute(
        "DELETE FROM raw_klines
         WHERE symbol = ?
           AND open_time >= CAST(? AS TIMESTAMP)
           AND open_time <= CAST(? AS TIMESTAMP)
           AND (? IS NULL OR product = ?)
           AND (? IS NULL OR interval = ?)",
        params![
            symbol,
            from_ts,
            to_ts,
            product,
            product,
            interval_name,
            interval_name
        ],
    )
    .map(|_| ())
    .map_err(storage_err)
}

fn clear_duckdb_liquidations(
    duck: &DuckConnection,
    symbol: &str,
    from_ts: &str,
    to_ts: &str,
) -> Result<(), StorageError> {
    duck.execute(
        "DELETE FROM raw_liquidation_events
         WHERE symbol = ?
           AND event_time >= CAST(? AS TIMESTAMP)
           AND event_time <= CAST(? AS TIMESTAMP)",
        params![symbol, from_ts, to_ts],
    )
    .map(|_| ())
    .map_err(storage_err)
}

fn clear_duckdb_book_tickers(
    duck: &DuckConnection,
    symbol: &str,
    from_ts: &str,
    to_ts: &str,
) -> Result<(), StorageError> {
    duck.execute(
        "DELETE FROM raw_book_ticker
         WHERE symbol = ?
           AND event_time >= CAST(? AS TIMESTAMP)
           AND event_time <= CAST(? AS TIMESTAMP)",
        params![symbol, from_ts, to_ts],
    )
    .map(|_| ())
    .map_err(storage_err)
}

fn clear_duckdb_agg_trades(
    duck: &DuckConnection,
    symbol: &str,
    from_ts: &str,
    to_ts: &str,
) -> Result<(), StorageError> {
    duck.execute(
        "DELETE FROM raw_agg_trades
         WHERE symbol = ?
           AND event_time >= CAST(? AS TIMESTAMP)
           AND event_time <= CAST(? AS TIMESTAMP)",
        params![symbol, from_ts, to_ts],
    )
    .map(|_| ())
    .map_err(storage_err)
}

fn next_duckdb_kline_id(connection: &DuckConnection) -> Result<i64, StorageError> {
    connection
        .prepare("SELECT COALESCE(MAX(kline_id), 0) + 1 FROM raw_klines")
        .and_then(|mut statement| statement.query_row([], |row| row.get(0)))
        .map_err(storage_err)
}

fn next_duckdb_liquidation_event_id(connection: &DuckConnection) -> Result<i64, StorageError> {
    connection
        .prepare("SELECT COALESCE(MAX(event_id), 0) + 1 FROM raw_liquidation_events")
        .and_then(|mut statement| statement.query_row([], |row| row.get(0)))
        .map_err(storage_err)
}

fn next_duckdb_book_ticker_id(connection: &DuckConnection) -> Result<i64, StorageError> {
    connection
        .prepare("SELECT COALESCE(MAX(tick_id), 0) + 1 FROM raw_book_ticker")
        .and_then(|mut statement| statement.query_row([], |row| row.get(0)))
        .map_err(storage_err)
}

fn next_duckdb_agg_trade_id(connection: &DuckConnection) -> Result<i64, StorageError> {
    connection
        .prepare("SELECT COALESCE(MAX(trade_id), 0) + 1 FROM raw_agg_trades")
        .and_then(|mut statement| statement.query_row([], |row| row.get(0)))
        .map_err(storage_err)
}

fn next_duckdb_snapshot_export_id(connection: &DuckConnection) -> Result<i64, StorageError> {
    connection
        .prepare("SELECT COALESCE(MAX(export_id), 0) + 1 FROM snapshot_exports")
        .and_then(|mut statement| statement.query_row([], |row| row.get(0)))
        .map_err(storage_err)
}

pub fn mask_postgres_url(url: &str) -> String {
    if let Some((scheme, rest)) = url.split_once("://") {
        if let Some((_, host_part)) = rest.rsplit_once('@') {
            return format!("{scheme}://***@{host_part}");
        }
    }
    "postgres://***".to_string()
}

fn storage_err(error: impl std::fmt::Display) -> StorageError {
    StorageError::WriteFailedWithContext {
        message: error.to_string(),
    }
}

fn timestamp_from_millis(value: i64) -> Result<chrono::DateTime<Utc>, StorageError> {
    Utc.timestamp_millis_opt(value)
        .single()
        .ok_or_else(|| StorageError::WriteFailedWithContext {
            message: format!("invalid timestamp millis: {value}"),
        })
}
