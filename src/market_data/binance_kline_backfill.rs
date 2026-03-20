use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use reqwest::blocking::Client;
use serde_json::Value;
use tracing::info;

use crate::app::bootstrap::BinanceMode;
use crate::storage::postgres_market_data::{
    connect as connect_postgres, init_schema as init_postgres_schema, insert_kline,
    latest_shared_kline_open_time_ms, PostgresKlineRecord,
};

pub const DEFAULT_BINANCE_BACKFILL_SYMBOLS: &[&str] = &["BTCUSDT", "SOLUSDT", "XRPUSDT", "ETHUSDT"];
pub const DEFAULT_BINANCE_BACKFILL_PRODUCT: &str = "um";
pub const DEFAULT_BINANCE_BACKFILL_INTERVAL: &str = "1m";
pub const DEFAULT_BINANCE_BACKFILL_START_DATE: &str = "2023-03-15";
pub const DEFAULT_BINANCE_BACKFILL_POLL_SECONDS: u64 = 30;
const DEFAULT_LIMIT: usize = 1_500;

#[derive(Debug, Clone, PartialEq)]
pub struct BinanceKlineBackfillConfig {
    pub postgres_url: String,
    pub symbols: Vec<String>,
    pub mode: BinanceMode,
    pub product: String,
    pub interval: String,
    pub fallback_start_ms: i64,
    pub continuous: bool,
    pub poll_seconds: u64,
}

pub fn run_binance_kline_backfill(
    config: &BinanceKlineBackfillConfig,
    stop_flag: Option<Arc<AtomicBool>>,
) -> Result<()> {
    let mut client = connect_postgres(&config.postgres_url)?;
    let _ = init_postgres_schema(&mut client, &config.postgres_url)?;
    let http = Client::builder()
        .build()
        .context("failed to build HTTP client")?;

    info!(
        service = "postgres-kline-backfill",
        mode = config.mode.as_str(),
        product = config.product,
        interval = config.interval,
        symbols = %config.symbols.join(","),
        continuous = config.continuous,
        poll_seconds = config.poll_seconds,
        "postgres kline backfill starting"
    );

    loop {
        if stop_flag
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            break;
        }

        let now_minute = current_closed_minute_open_ms();
        let mut cycle_rows = 0u64;
        for symbol in &config.symbols {
            let inserted = backfill_symbol(
                &http,
                &mut client,
                config.mode,
                &config.product,
                &config.interval,
                symbol,
                config.fallback_start_ms,
                now_minute,
            )?;
            cycle_rows += inserted;
            info!(
                service = "postgres-kline-backfill",
                mode = config.mode.as_str(),
                symbol = symbol,
                inserted_rows = inserted,
                "symbol backfill cycle completed"
            );
        }

        info!(
            service = "postgres-kline-backfill",
            mode = config.mode.as_str(),
            inserted_rows = cycle_rows,
            "postgres kline backfill cycle completed"
        );

        if !config.continuous {
            break;
        }
        std::thread::sleep(StdDuration::from_secs(config.poll_seconds));
    }

    info!(
        service = "postgres-kline-backfill",
        mode = config.mode.as_str(),
        "postgres kline backfill stopped"
    );
    Ok(())
}

pub fn parse_start_time(raw: &str) -> Result<i64> {
    if let Ok(datetime) = DateTime::parse_from_rfc3339(raw) {
        return Ok(datetime.timestamp_millis());
    }
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let datetime = date
            .and_hms_opt(0, 0, 0)
            .context("invalid fallback start date")?;
        return Ok(Utc.from_utc_datetime(&datetime).timestamp_millis());
    }
    if let Ok(datetime) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&datetime).timestamp_millis());
    }
    anyhow::bail!("unsupported SANDBOX_QUANT_BACKFILL_FROM format: {raw}")
}

pub fn interval_millis(interval: &str) -> Result<i64> {
    match interval {
        "1m" => Ok(Duration::minutes(1).num_milliseconds()),
        other => anyhow::bail!("unsupported interval: {other}"),
    }
}

pub fn current_closed_minute_open_ms() -> i64 {
    let now = Utc::now();
    let closed = now
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .unwrap_or(now)
        - Duration::minutes(1);
    closed.timestamp_millis()
}

fn backfill_symbol(
    http: &Client,
    client: &mut postgres::Client,
    mode: BinanceMode,
    product: &str,
    interval: &str,
    symbol: &str,
    fallback_start_ms: i64,
    max_open_time_ms: i64,
) -> Result<u64> {
    let mut start_ms = latest_shared_kline_open_time_ms(client, product, symbol, interval)?
        .unwrap_or(fallback_start_ms);
    if start_ms < fallback_start_ms {
        start_ms = fallback_start_ms;
    } else {
        start_ms += interval_millis(interval)?;
    }

    if start_ms > max_open_time_ms {
        return Ok(0);
    }

    let mut inserted_rows = 0u64;
    let interval_ms = interval_millis(interval)?;

    while start_ms <= max_open_time_ms {
        let url = format!(
            "https://fapi.binance.com/fapi/v1/klines?symbol={symbol}&interval={interval}&startTime={start_ms}&limit={DEFAULT_LIMIT}"
        );
        let rows = http
            .get(&url)
            .send()
            .with_context(|| format!("failed to fetch klines for {symbol}"))?
            .error_for_status()
            .with_context(|| format!("kline HTTP status error for {symbol}"))?
            .json::<Vec<Vec<Value>>>()
            .with_context(|| format!("failed to decode kline response for {symbol}"))?;

        if rows.is_empty() {
            break;
        }

        let mut last_open_time_ms = start_ms;
        for row in rows {
            let record = parse_kline_row(mode, product, symbol, interval, row)?;
            last_open_time_ms = record.open_time_ms;
            if record.open_time_ms > max_open_time_ms {
                break;
            }
            insert_kline(client, &record)?;
            inserted_rows += 1;
        }

        let next_start_ms = last_open_time_ms + interval_ms;
        if next_start_ms <= start_ms {
            break;
        }
        start_ms = next_start_ms;
    }

    Ok(inserted_rows)
}

fn parse_kline_row(
    _mode: BinanceMode,
    product: &str,
    symbol: &str,
    interval: &str,
    row: Vec<Value>,
) -> Result<PostgresKlineRecord> {
    if row.len() < 11 {
        anyhow::bail!("kline row too short for {symbol}");
    }

    let open_time_ms = value_as_i64(&row[0])?;
    let open = value_as_f64(&row[1])?;
    let high = value_as_f64(&row[2])?;
    let low = value_as_f64(&row[3])?;
    let close = value_as_f64(&row[4])?;
    let volume = value_as_f64(&row[5])?;
    let close_time_ms = value_as_i64(&row[6])?;
    let quote_volume = value_as_f64(&row[7])?;
    let trade_count = value_as_i64(&row[8])?;
    let taker_buy_base_volume = value_as_optional_f64(&row[9])?;
    let taker_buy_quote_volume = value_as_optional_f64(&row[10])?;

    Ok(PostgresKlineRecord {
        product: product.to_string(),
        symbol: symbol.to_string(),
        interval_name: interval.to_string(),
        open_time_ms,
        close_time_ms,
        open,
        high,
        low,
        close,
        volume,
        quote_volume,
        trade_count,
        taker_buy_base_volume,
        taker_buy_quote_volume,
        raw_payload: serde_json::to_string(&row).context("failed to encode raw kline payload")?,
    })
}

fn value_as_i64(value: &Value) -> Result<i64> {
    match value {
        Value::Number(number) => number.as_i64().context("expected integer number"),
        Value::String(text) => text.parse::<i64>().context("expected integer string"),
        _ => anyhow::bail!("expected integer value"),
    }
}

fn value_as_f64(value: &Value) -> Result<f64> {
    match value {
        Value::Number(number) => number.as_f64().context("expected float number"),
        Value::String(text) => text.parse::<f64>().context("expected float string"),
        _ => anyhow::bail!("expected float value"),
    }
}

fn value_as_optional_f64(value: &Value) -> Result<Option<f64>> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) if text.is_empty() => Ok(None),
        _ => value_as_f64(value).map(Some),
    }
}
