use chrono::NaiveDate;
use duckdb::{params, Connection as DuckConnection};
use postgres::{Client, NoTls};

use crate::app::bootstrap::BinanceMode;
use crate::dataset::schema::{init_schema_for_path, MARKET_DATA_SCHEMA_VERSION};
use crate::error::storage_error::StorageError;
use crate::record::coordination::RecorderCoordination;

pub const POSTGRES_MARKET_DATA_SCHEMA_VERSION: &str = MARKET_DATA_SCHEMA_VERSION;

const POSTGRES_MARKET_DATA_SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS schema_metadata (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS raw_liquidation_events (
  event_id BIGSERIAL PRIMARY KEY,
  mode TEXT NOT NULL,
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
ON raw_liquidation_events (mode, product, symbol, event_time, force_side, price, qty);

CREATE TABLE IF NOT EXISTS raw_book_ticker (
  tick_id BIGSERIAL PRIMARY KEY,
  mode TEXT NOT NULL,
  symbol TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  receive_time TIMESTAMPTZ NOT NULL,
  bid DOUBLE PRECISION NOT NULL,
  bid_qty DOUBLE PRECISION NOT NULL,
  ask DOUBLE PRECISION NOT NULL,
  ask_qty DOUBLE PRECISION NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_book_ticker_natural_idx
ON raw_book_ticker (mode, symbol, event_time, bid, ask, bid_qty, ask_qty);

CREATE TABLE IF NOT EXISTS raw_agg_trades (
  trade_id BIGSERIAL PRIMARY KEY,
  mode TEXT NOT NULL,
  symbol TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  receive_time TIMESTAMPTZ NOT NULL,
  price DOUBLE PRECISION NOT NULL,
  qty DOUBLE PRECISION NOT NULL,
  is_buyer_maker BOOLEAN NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS raw_agg_trades_natural_idx
ON raw_agg_trades (mode, symbol, event_time, price, qty, is_buyer_maker);

CREATE TABLE IF NOT EXISTS raw_klines (
  kline_id BIGSERIAL PRIMARY KEY,
  mode TEXT NOT NULL,
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
ON raw_klines (mode, product, symbol, interval_name, open_time);
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
    pub mode: String,
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
    pub mode: String,
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
    pub mode: String,
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
    pub mode: String,
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

pub fn postgres_url_from_env() -> Result<String, StorageError> {
    std::env::var("SANDBOX_QUANT_POSTGRES_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .map_err(|_| StorageError::WriteFailedWithContext {
            message: "missing PostgreSQL URL; set SANDBOX_QUANT_POSTGRES_URL or DATABASE_URL"
                .to_string(),
        })
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

pub fn insert_kline(client: &mut Client, record: &PostgresKlineRecord) -> Result<(), StorageError> {
    client
        .execute(
            "INSERT INTO raw_klines (
                mode, product, symbol, interval_name, open_time, close_time,
                open, high, low, close, volume, quote_volume, trade_count,
                taker_buy_base_volume, taker_buy_quote_volume, raw_payload
             ) VALUES (
                $1, $2, $3, $4, to_timestamp($5 / 1000.0), to_timestamp($6 / 1000.0),
                $7, $8, $9, $10, $11, $12, $13, $14, $15, $16
             )
             ON CONFLICT (mode, product, symbol, interval_name, open_time) DO NOTHING",
            &[
                &record.mode,
                &record.product,
                &record.symbol,
                &record.interval_name,
                &record.open_time_ms,
                &record.close_time_ms,
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
    client
        .execute(
            "INSERT INTO raw_liquidation_events (
                mode, product, symbol, event_time, receive_time, force_side, price, qty, notional, raw_payload
             ) VALUES (
                $1, $2, $3, to_timestamp($4 / 1000.0), to_timestamp($5 / 1000.0), $6, $7, $8, $9, $10
             )
             ON CONFLICT (mode, product, symbol, event_time, force_side, price, qty) DO NOTHING",
            &[
                &record.mode,
                &record.product,
                &record.symbol,
                &record.event_time_ms,
                &record.receive_time_ms,
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
    client
        .execute(
            "INSERT INTO raw_book_ticker (
                mode, symbol, event_time, receive_time, bid, bid_qty, ask, ask_qty
             ) VALUES (
                $1, $2, to_timestamp($3 / 1000.0), to_timestamp($4 / 1000.0), $5, $6, $7, $8
             )
             ON CONFLICT (mode, symbol, event_time, bid, ask, bid_qty, ask_qty) DO NOTHING",
            &[
                &record.mode,
                &record.symbol,
                &record.event_time_ms,
                &record.receive_time_ms,
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
    client
        .execute(
            "INSERT INTO raw_agg_trades (
                mode, symbol, event_time, receive_time, price, qty, is_buyer_maker
             ) VALUES (
                $1, $2, to_timestamp($3 / 1000.0), to_timestamp($4 / 1000.0), $5, $6, $7
             )
             ON CONFLICT (mode, symbol, event_time, price, qty, is_buyer_maker) DO NOTHING",
            &[
                &record.mode,
                &record.symbol,
                &record.event_time_ms,
                &record.receive_time_ms,
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
    let _ = init_schema(&mut client, url)?;
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

    let from_ts = format!("{} 00:00:00", config.from);
    let to_ts = format!("{} 23:59:59", config.to);

    let mut kline_rows = 0usize;
    let mut liquidation_rows = 0usize;
    let mut book_ticker_rows = 0usize;
    let mut agg_trade_rows = 0usize;

    for symbol in &config.symbols {
        if config.clear_duckdb_range {
            if config.include_klines {
                clear_duckdb_klines(
                    &duck,
                    config.mode,
                    symbol,
                    config.product.as_deref(),
                    config.interval_name.as_deref(),
                    &from_ts,
                    &to_ts,
                )?;
            }
            if config.include_liquidations {
                clear_duckdb_liquidations(&duck, config.mode, symbol, &from_ts, &to_ts)?;
            }
            if config.include_book_tickers {
                clear_duckdb_book_tickers(&duck, config.mode, symbol, &from_ts, &to_ts)?;
            }
            if config.include_agg_trades {
                clear_duckdb_agg_trades(&duck, config.mode, symbol, &from_ts, &to_ts)?;
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
                     WHERE mode = $1
                       AND symbol = $2
                       AND open_time >= CAST($3 AS TIMESTAMPTZ)
                       AND open_time <= CAST($4 AS TIMESTAMPTZ)
                       AND ($5::TEXT IS NULL OR product = $5)
                       AND ($6::TEXT IS NULL OR interval_name = $6)
                     ORDER BY open_time ASC",
                    &[
                        &config.mode.as_str(),
                        &symbol,
                        &from_ts,
                        &to_ts,
                        &config.product,
                        &config.interval_name,
                    ],
                )
                .map_err(storage_err)?;

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
                        config.mode.as_str(),
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
                .map_err(storage_err)?;
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
                     WHERE mode = $1
                       AND symbol = $2
                       AND event_time >= CAST($3 AS TIMESTAMPTZ)
                       AND event_time <= CAST($4 AS TIMESTAMPTZ)
                     ORDER BY event_time ASC",
                    &[&config.mode.as_str(), &symbol, &from_ts, &to_ts],
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
                        config.mode.as_str(),
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
                     WHERE mode = $1
                       AND symbol = $2
                       AND event_time >= CAST($3 AS TIMESTAMPTZ)
                       AND event_time <= CAST($4 AS TIMESTAMPTZ)
                     ORDER BY event_time ASC",
                    &[&config.mode.as_str(), &symbol, &from_ts, &to_ts],
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
                        config.mode.as_str(),
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
                     WHERE mode = $1
                       AND symbol = $2
                       AND event_time >= CAST($3 AS TIMESTAMPTZ)
                       AND event_time <= CAST($4 AS TIMESTAMPTZ)
                     ORDER BY event_time ASC",
                    &[&config.mode.as_str(), &symbol, &from_ts, &to_ts],
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
                        config.mode.as_str(),
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
            config.product.as_deref(),
            config.interval_name.as_deref(),
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
    .map_err(storage_err)?;

    Ok(PostgresToDuckDbSnapshotReport {
        snapshot_export_id,
        db_path: db_path.display().to_string(),
        kline_rows,
        liquidation_rows,
        book_ticker_rows,
        agg_trade_rows,
    })
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
    mode: BinanceMode,
    symbol: &str,
    product: Option<&str>,
    interval_name: Option<&str>,
    from_ts: &str,
    to_ts: &str,
) -> Result<(), StorageError> {
    duck.execute(
        "DELETE FROM raw_klines
         WHERE mode = ?
           AND symbol = ?
           AND open_time >= CAST(? AS TIMESTAMP)
           AND open_time <= CAST(? AS TIMESTAMP)
           AND (? IS NULL OR product = ?)
           AND (? IS NULL OR interval = ?)",
        params![
            mode.as_str(),
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
    mode: BinanceMode,
    symbol: &str,
    from_ts: &str,
    to_ts: &str,
) -> Result<(), StorageError> {
    duck.execute(
        "DELETE FROM raw_liquidation_events
         WHERE mode = ?
           AND symbol = ?
           AND event_time >= CAST(? AS TIMESTAMP)
           AND event_time <= CAST(? AS TIMESTAMP)",
        params![mode.as_str(), symbol, from_ts, to_ts],
    )
    .map(|_| ())
    .map_err(storage_err)
}

fn clear_duckdb_book_tickers(
    duck: &DuckConnection,
    mode: BinanceMode,
    symbol: &str,
    from_ts: &str,
    to_ts: &str,
) -> Result<(), StorageError> {
    duck.execute(
        "DELETE FROM raw_book_ticker
         WHERE mode = ?
           AND symbol = ?
           AND event_time >= CAST(? AS TIMESTAMP)
           AND event_time <= CAST(? AS TIMESTAMP)",
        params![mode.as_str(), symbol, from_ts, to_ts],
    )
    .map(|_| ())
    .map_err(storage_err)
}

fn clear_duckdb_agg_trades(
    duck: &DuckConnection,
    mode: BinanceMode,
    symbol: &str,
    from_ts: &str,
    to_ts: &str,
) -> Result<(), StorageError> {
    duck.execute(
        "DELETE FROM raw_agg_trades
         WHERE mode = ?
           AND symbol = ?
           AND event_time >= CAST(? AS TIMESTAMP)
           AND event_time <= CAST(? AS TIMESTAMP)",
        params![mode.as_str(), symbol, from_ts, to_ts],
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
