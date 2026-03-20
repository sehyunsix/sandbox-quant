use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use reqwest::blocking::Client;
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::observability::logging::init_logging;
use sandbox_quant::storage::postgres_market_data::{
    connect as connect_postgres, init_schema as init_postgres_schema, insert_kline,
    latest_shared_kline_open_time_ms, postgres_url_from_env, PostgresKlineRecord,
};
use serde_json::Value;
use tracing::info;

const DEFAULT_SYMBOLS: &[&str] = &["BTCUSDT", "SOLUSDT", "XRPUSDT", "ETHUSDT"];
const DEFAULT_PRODUCT: &str = "um";
const DEFAULT_INTERVAL: &str = "1m";
const DEFAULT_LIMIT: usize = 1_500;
const DEFAULT_START_DATE: &str = "2023-03-15";
const DEFAULT_POLL_SECONDS: u64 = 30;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let postgres_url = postgres_url_from_env().context("missing PostgreSQL URL")?;
    let symbols = configured_symbols();
    let mode = configured_mode();
    let product = configured_product();
    let interval = configured_interval();
    let fallback_start_ms = configured_fallback_start_ms()?;
    let continuous = configured_continuous();
    let poll_seconds = configured_poll_seconds();
    init_logging("postgres-kline-backfill", Some(mode.as_str()))?;

    let mut client = connect_postgres(&postgres_url)?;
    let _ = init_postgres_schema(&mut client, &postgres_url)?;
    let http = Client::builder()
        .build()
        .context("failed to build HTTP client")?;

    info!(service = "postgres-kline-backfill", mode = mode.as_str(), product = product, interval = interval, symbols = %symbols.join(","), continuous, poll_seconds, "postgres kline backfill starting");

    loop {
        let now_minute = current_closed_minute_open_ms();
        let mut cycle_rows = 0u64;
        for symbol in &symbols {
            let inserted = backfill_symbol(
                &http,
                &mut client,
                mode,
                &product,
                &interval,
                symbol,
                fallback_start_ms,
                now_minute,
            )?;
            cycle_rows += inserted;
            info!(
                service = "postgres-kline-backfill",
                mode = mode.as_str(),
                symbol = symbol,
                inserted_rows = inserted,
                "symbol backfill cycle completed"
            );
        }
        info!(
            service = "postgres-kline-backfill",
            mode = mode.as_str(),
            inserted_rows = cycle_rows,
            "postgres kline backfill cycle completed"
        );
        if !continuous {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(poll_seconds));
    }

    info!(
        service = "postgres-kline-backfill",
        mode = mode.as_str(),
        "postgres kline backfill stopped"
    );
    Ok(())
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
    let mut start_ms = latest_kline_open_time_ms(client, mode, product, symbol, interval)?
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

fn latest_kline_open_time_ms(
    client: &mut postgres::Client,
    _mode: BinanceMode,
    product: &str,
    symbol: &str,
    interval: &str,
) -> Result<Option<i64>> {
    latest_shared_kline_open_time_ms(client, product, symbol, interval)
        .context("failed to query latest kline open_time")
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

fn configured_symbols() -> Vec<String> {
    std::env::var("SANDBOX_QUANT_BACKFILL_SYMBOLS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|symbol| symbol.trim().to_ascii_uppercase())
                .filter(|symbol| !symbol.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|symbols| !symbols.is_empty())
        .unwrap_or_else(|| {
            DEFAULT_SYMBOLS
                .iter()
                .map(|symbol| symbol.to_string())
                .collect()
        })
}

fn configured_mode() -> BinanceMode {
    match std::env::var("BINANCE_MODE").ok().as_deref() {
        Some("real") => BinanceMode::Real,
        _ => BinanceMode::Demo,
    }
}

fn configured_product() -> String {
    std::env::var("SANDBOX_QUANT_BACKFILL_PRODUCT").unwrap_or_else(|_| DEFAULT_PRODUCT.to_string())
}

fn configured_interval() -> String {
    std::env::var("SANDBOX_QUANT_BACKFILL_INTERVAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_INTERVAL.to_string())
}

fn configured_continuous() -> bool {
    matches!(
        std::env::var("SANDBOX_QUANT_BACKFILL_CONTINUOUS")
            .ok()
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn configured_poll_seconds() -> u64 {
    std::env::var("SANDBOX_QUANT_BACKFILL_POLL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_POLL_SECONDS)
}

fn configured_fallback_start_ms() -> Result<i64> {
    let raw = std::env::var("SANDBOX_QUANT_BACKFILL_FROM")
        .unwrap_or_else(|_| DEFAULT_START_DATE.to_string());
    parse_start_time(&raw)
}

fn parse_start_time(raw: &str) -> Result<i64> {
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

fn interval_millis(interval: &str) -> Result<i64> {
    match interval {
        "1m" => Ok(Duration::minutes(1).num_milliseconds()),
        other => anyhow::bail!("unsupported interval: {other}"),
    }
}

fn current_closed_minute_open_ms() -> i64 {
    let now = Utc::now();
    let closed = now
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .unwrap_or(now)
        - Duration::minutes(1);
    closed.timestamp_millis()
}
