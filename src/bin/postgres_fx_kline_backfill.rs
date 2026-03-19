use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use reqwest::blocking::Client;
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::observability::logging::init_logging;
use sandbox_quant::storage::postgres_market_data::{
    connect as connect_postgres, init_schema as init_postgres_schema, insert_kline,
    latest_shared_kline_open_time_ms, postgres_url_from_env, PostgresKlineRecord,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

const DEFAULT_PAIRS: &[&str] = &["USD/KRW", "JPY/KRW", "EUR/KRW", "CNY/KRW"];
const DEFAULT_PRODUCT: &str = "fx-twelvedata";
const DEFAULT_INTERVAL: &str = "1min";
const DEFAULT_OUTPUT_SIZE: usize = 5_000;
const DEFAULT_START_DATE: &str = "2023-03-15";
const DEFAULT_POLL_SECONDS: u64 = 60;
const DEFAULT_MAX_REQUESTS_PER_MINUTE: u64 = 7;
const DEFAULT_RATE_LIMIT_SLEEP_SECONDS: u64 = 65;

#[derive(Debug, Deserialize)]
struct FxResponse {
    values: Option<Vec<FxBar>>,
    status: Option<String>,
    code: Option<u16>,
    message: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct FxBar {
    datetime: String,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: Option<String>,
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let postgres_url = postgres_url_from_env().context("missing PostgreSQL URL")?;
    let api_key = std::env::var("TWELVE_DATA_API_KEY")
        .context("missing TWELVE_DATA_API_KEY for Twelve Data FX backfill")?;
    let pairs = configured_pairs();
    let mode = configured_mode();
    let product = configured_product();
    let interval = configured_interval();
    let fallback_start_ms = configured_fallback_start_ms()?;
    let continuous = configured_continuous();
    let poll_seconds = configured_poll_seconds();
    let request_gap = configured_request_gap();
    let rate_limit_sleep = configured_rate_limit_sleep();
    init_logging("postgres-fx-backfill", Some(mode.as_str()))?;

    let mut client = connect_postgres(&postgres_url)?;
    let _ = init_postgres_schema(&mut client, &postgres_url)?;
    let http = Client::builder()
        .build()
        .context("failed to build HTTP client")?;

    info!(service = "postgres-fx-backfill", mode = mode.as_str(), product = product, interval = interval, pairs = %pairs.join(","), continuous, poll_seconds, request_gap_ms = request_gap.as_millis() as u64, "postgres fx kline backfill starting");

    loop {
        let max_open_time_ms = current_closed_minute_open_ms();
        let mut cycle_rows = 0u64;

        for pair in &pairs {
            let inserted = backfill_pair(
                &http,
                &mut client,
                &api_key,
                mode,
                &product,
                &interval,
                pair,
                fallback_start_ms,
                max_open_time_ms,
                request_gap,
                rate_limit_sleep,
            )?;
            cycle_rows += inserted;
            info!(
                service = "postgres-fx-backfill",
                mode = mode.as_str(),
                pair = pair,
                inserted_rows = inserted,
                "fx pair backfill cycle completed"
            );
        }

        info!(
            service = "postgres-fx-backfill",
            mode = mode.as_str(),
            inserted_rows = cycle_rows,
            "postgres fx kline backfill cycle completed"
        );
        if !continuous {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(poll_seconds));
    }

    info!(
        service = "postgres-fx-backfill",
        mode = mode.as_str(),
        "postgres fx kline backfill stopped"
    );
    Ok(())
}

fn backfill_pair(
    http: &Client,
    client: &mut postgres::Client,
    api_key: &str,
    mode: BinanceMode,
    product: &str,
    interval: &str,
    pair: &str,
    fallback_start_ms: i64,
    max_open_time_ms: i64,
    request_gap: std::time::Duration,
    rate_limit_sleep: std::time::Duration,
) -> Result<u64> {
    let interval_ms = interval_millis(interval)?;
    let start_ms = latest_kline_open_time_ms(client, mode, product, pair, "1m")?
        .map(|value| value + interval_ms)
        .unwrap_or(fallback_start_ms);

    if start_ms > max_open_time_ms {
        return Ok(0);
    }

    let mut query_start_ms = start_ms;
    let mut inserted_rows = 0u64;

    while query_start_ms <= max_open_time_ms {
        let query_end_ms =
            (query_start_ms + interval_ms * (DEFAULT_OUTPUT_SIZE as i64 - 1)).min(max_open_time_ms);
        let url = format!(
            "https://api.twelvedata.com/time_series?symbol={symbol}&interval={interval}&start_date={start}&end_date={end}&order=ASC&timezone=UTC&outputsize={outputsize}&apikey={apikey}",
            symbol = pair,
            interval = interval,
            start = format_twelvedata_datetime(query_start_ms)?,
            end = format_twelvedata_datetime(query_end_ms)?,
            outputsize = DEFAULT_OUTPUT_SIZE,
            apikey = api_key
        );
        let response = fetch_fx_response(http, &url, pair, request_gap, rate_limit_sleep)?;

        if response.status.as_deref() == Some("error") || response.code.is_some() {
            anyhow::bail!(
                "Twelve Data error for {}: {}",
                pair,
                response
                    .message
                    .unwrap_or_else(|| "unknown Twelve Data error".to_string())
            );
        }

        let Some(rows) = response.values else {
            break;
        };
        if rows.is_empty() {
            break;
        }

        let mut last_open_time_ms = query_start_ms;
        for row in rows {
            let record = parse_fx_bar(mode, product, interval, pair, row)?;
            last_open_time_ms = record.open_time_ms;
            if record.open_time_ms < start_ms || record.open_time_ms > max_open_time_ms {
                continue;
            }
            insert_kline(client, &record)?;
            inserted_rows += 1;
        }

        let next_start_ms = last_open_time_ms + interval_ms;
        if next_start_ms <= query_start_ms {
            break;
        }
        query_start_ms = next_start_ms;
    }

    Ok(inserted_rows)
}

fn fetch_fx_response(
    http: &Client,
    url: &str,
    pair: &str,
    request_gap: std::time::Duration,
    rate_limit_sleep: std::time::Duration,
) -> Result<FxResponse> {
    loop {
        let response = http
            .get(url)
            .send()
            .with_context(|| format!("failed to fetch Twelve Data FX bars for {}", pair))?
            .error_for_status()
            .with_context(|| format!("Twelve Data FX HTTP status error for {}", pair))?
            .json::<FxResponse>()
            .with_context(|| format!("failed to decode Twelve Data FX response for {}", pair))?;
        std::thread::sleep(request_gap);

        let rate_limited = response.code == Some(429)
            || response
                .message
                .as_deref()
                .is_some_and(|message| message.contains("run out of API credits"));
        if rate_limited {
            warn!(
                service = "postgres-fx-backfill",
                pair = pair,
                sleep_sec = rate_limit_sleep.as_secs(),
                "twelvedata rate limit hit"
            );
            std::thread::sleep(rate_limit_sleep);
            continue;
        }
        return Ok(response);
    }
}

fn latest_kline_open_time_ms(
    client: &mut postgres::Client,
    _mode: BinanceMode,
    product: &str,
    symbol: &str,
    interval: &str,
) -> Result<Option<i64>> {
    latest_shared_kline_open_time_ms(client, product, symbol, interval)
        .context("failed to query latest FX kline open_time")
}

fn parse_fx_bar(
    _mode: BinanceMode,
    product: &str,
    interval: &str,
    pair: &str,
    row: FxBar,
) -> Result<PostgresKlineRecord> {
    let open_time = NaiveDateTime::parse_from_str(&row.datetime, "%Y-%m-%d %H:%M:%S")
        .context("failed to parse Twelve Data datetime")?;
    let open_time_ms = Utc.from_utc_datetime(&open_time).timestamp_millis();
    let interval_ms = interval_millis(interval)?;

    Ok(PostgresKlineRecord {
        product: product.to_string(),
        symbol: pair.to_string(),
        interval_name: "1m".to_string(),
        open_time_ms,
        close_time_ms: open_time_ms + interval_ms - 1,
        open: row.open.parse::<f64>().context("invalid FX open")?,
        high: row.high.parse::<f64>().context("invalid FX high")?,
        low: row.low.parse::<f64>().context("invalid FX low")?,
        close: row.close.parse::<f64>().context("invalid FX close")?,
        volume: row
            .volume
            .as_deref()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(0.0),
        quote_volume: 0.0,
        trade_count: 0,
        taker_buy_base_volume: None,
        taker_buy_quote_volume: None,
        raw_payload: serde_json::to_string(&row).context("failed to encode FX raw payload")?,
    })
}

fn configured_pairs() -> Vec<String> {
    std::env::var("SANDBOX_QUANT_FX_PAIRS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|pair| pair.trim().to_ascii_uppercase())
                .filter(|pair| !pair.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|pairs| !pairs.is_empty())
        .unwrap_or_else(|| DEFAULT_PAIRS.iter().map(|pair| pair.to_string()).collect())
}

fn configured_mode() -> BinanceMode {
    match std::env::var("BINANCE_MODE").ok().as_deref() {
        Some("real") => BinanceMode::Real,
        _ => BinanceMode::Demo,
    }
}

fn configured_product() -> String {
    std::env::var("SANDBOX_QUANT_FX_PRODUCT").unwrap_or_else(|_| DEFAULT_PRODUCT.to_string())
}

fn configured_interval() -> String {
    std::env::var("SANDBOX_QUANT_FX_INTERVAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_INTERVAL.to_string())
}

fn configured_continuous() -> bool {
    matches!(
        std::env::var("SANDBOX_QUANT_FX_CONTINUOUS").ok().as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn configured_poll_seconds() -> u64 {
    std::env::var("SANDBOX_QUANT_FX_POLL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_POLL_SECONDS)
}

fn configured_request_gap() -> std::time::Duration {
    let max_requests_per_minute = std::env::var("SANDBOX_QUANT_FX_MAX_REQUESTS_PER_MINUTE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_REQUESTS_PER_MINUTE);
    let gap_ms = ((60_000f64 / max_requests_per_minute as f64).ceil() as u64).max(1_000);
    std::time::Duration::from_millis(gap_ms)
}

fn configured_rate_limit_sleep() -> std::time::Duration {
    let seconds = std::env::var("SANDBOX_QUANT_FX_RATE_LIMIT_SLEEP_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_RATE_LIMIT_SLEEP_SECONDS);
    std::time::Duration::from_secs(seconds)
}

fn configured_fallback_start_ms() -> Result<i64> {
    let raw =
        std::env::var("SANDBOX_QUANT_FX_FROM").unwrap_or_else(|_| DEFAULT_START_DATE.to_string());
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
    anyhow::bail!("unsupported SANDBOX_QUANT_FX_FROM format: {raw}")
}

fn interval_millis(interval: &str) -> Result<i64> {
    match interval {
        "1min" | "1m" => Ok(Duration::minutes(1).num_milliseconds()),
        other => anyhow::bail!("unsupported FX interval: {other}"),
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

fn format_twelvedata_datetime(value_ms: i64) -> Result<String> {
    let datetime = Utc
        .timestamp_millis_opt(value_ms)
        .single()
        .context("invalid Twelve Data datetime millis")?;
    Ok(datetime.format("%Y-%m-%d %H:%M:%S").to_string())
}
