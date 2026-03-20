use anyhow::{Context, Result};
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use reqwest::blocking::Client;
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::observability::logging::init_logging;
use sandbox_quant::storage::postgres_market_data::{
    connect as connect_postgres, init_schema as init_postgres_schema, insert_kline,
    latest_shared_kline_open_time_ms, postgres_url_from_env, PostgresKlineRecord,
};
use serde::{Deserialize, Serialize};
use std::time::Duration as StdDuration;
use tracing::{info, warn};

const DEFAULT_ASSETS: &[&str] = &["BTC", "SOL", "XRP", "ETH"];
const DEFAULT_INTERVAL: &str = "1m";
const DEFAULT_COUNT: usize = 200;
const DEFAULT_START_DATE: &str = "2023-03-15";
const DEFAULT_POLL_SECONDS: u64 = 30;
const DEFAULT_REQUEST_GAP_MS: u64 = 150;
const DEFAULT_RATE_LIMIT_SLEEP_SECONDS: u64 = 3;

#[derive(Debug, Clone, Copy)]
struct ExchangeSpec {
    name: &'static str,
    market_all_url: &'static str,
    candles_base_url: &'static str,
    product: &'static str,
    cursor_timezone: CursorTimezone,
}

#[derive(Debug, Clone, Copy)]
enum CursorTimezone {
    UtcZulu,
    KstNaive,
}

#[derive(Debug, Deserialize)]
struct MarketCode {
    market: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct KrCandle {
    market: String,
    candle_date_time_utc: String,
    opening_price: f64,
    high_price: f64,
    low_price: f64,
    trade_price: f64,
    candle_acc_trade_price: f64,
    candle_acc_trade_volume: f64,
    unit: u32,
}

const EXCHANGES: &[ExchangeSpec] = &[
    ExchangeSpec {
        name: "upbit",
        market_all_url: "https://api.upbit.com/v1/market/all?isDetails=false",
        candles_base_url: "https://api.upbit.com/v1/candles/minutes",
        product: "upbit-krw",
        cursor_timezone: CursorTimezone::UtcZulu,
    },
    ExchangeSpec {
        name: "bithumb",
        market_all_url: "https://api.bithumb.com/v1/market/all?isDetails=false",
        candles_base_url: "https://api.bithumb.com/v1/candles/minutes",
        product: "bithumb-krw",
        cursor_timezone: CursorTimezone::KstNaive,
    },
];

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let postgres_url = postgres_url_from_env().context("missing PostgreSQL URL")?;
    let assets = configured_assets();
    let mode = configured_mode();
    let interval = configured_interval();
    let fallback_start_ms = configured_fallback_start_ms()?;
    let continuous = configured_continuous();
    let poll_seconds = configured_poll_seconds();
    let request_gap = configured_request_gap();
    let rate_limit_sleep = configured_rate_limit_sleep();
    init_logging("postgres-kr-backfill", Some(mode.as_str()))?;

    let mut client = connect_postgres(&postgres_url)?;
    let _ = init_postgres_schema(&mut client, &postgres_url)?;
    let http = Client::builder()
        .build()
        .context("failed to build HTTP client")?;

    info!(service = "postgres-kr-backfill", mode = mode.as_str(), interval = interval, assets = %assets.join(","), continuous, poll_seconds, "postgres kr exchange kline backfill starting");

    let market_maps = EXCHANGES
        .iter()
        .map(|exchange| {
            let markets = fetch_markets(&http, exchange)?;
            Ok::<_, anyhow::Error>((exchange.name, markets))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;

    loop {
        let max_open_time_ms = current_closed_minute_open_ms();
        let mut cycle_rows = 0u64;

        for exchange in EXCHANGES {
            let markets = market_maps
                .get(exchange.name)
                .context("missing exchange market map")?;
            for asset in &assets {
                let market = resolve_market(markets, asset).with_context(|| {
                    format!("{} market missing for asset {}", exchange.name, asset)
                })?;
                let inserted = backfill_market(
                    &http,
                    &mut client,
                    mode,
                    exchange,
                    &interval,
                    market,
                    fallback_start_ms,
                    max_open_time_ms,
                    request_gap,
                    rate_limit_sleep,
                )?;
                cycle_rows += inserted;
                info!(
                    service = "postgres-kr-backfill",
                    mode = mode.as_str(),
                    exchange = exchange.name,
                    market = market,
                    inserted_rows = inserted,
                    "market backfill cycle completed"
                );
            }
        }

        info!(
            service = "postgres-kr-backfill",
            mode = mode.as_str(),
            inserted_rows = cycle_rows,
            "postgres kr exchange kline backfill cycle completed"
        );
        if !continuous {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(poll_seconds));
    }

    info!(
        service = "postgres-kr-backfill",
        mode = mode.as_str(),
        "postgres kr exchange kline backfill stopped"
    );
    Ok(())
}

fn fetch_markets(http: &Client, exchange: &ExchangeSpec) -> Result<Vec<String>> {
    let rows = http
        .get(exchange.market_all_url)
        .send()
        .with_context(|| format!("failed to fetch {} market list", exchange.name))?
        .error_for_status()
        .with_context(|| format!("{} market list HTTP status error", exchange.name))?
        .json::<Vec<MarketCode>>()
        .with_context(|| format!("failed to decode {} market list", exchange.name))?;
    Ok(rows.into_iter().map(|row| row.market).collect())
}

fn resolve_market<'a>(markets: &'a [String], asset: &str) -> Option<&'a str> {
    let target = format!("KRW-{}", asset.to_ascii_uppercase());
    markets
        .iter()
        .find(|market| market.as_str() == target)
        .map(String::as_str)
}

fn backfill_market(
    http: &Client,
    client: &mut postgres::Client,
    mode: BinanceMode,
    exchange: &ExchangeSpec,
    interval: &str,
    market: &str,
    fallback_start_ms: i64,
    max_open_time_ms: i64,
    request_gap: StdDuration,
    rate_limit_sleep: StdDuration,
) -> Result<u64> {
    let interval_ms = interval_millis(interval)?;
    let start_ms = latest_kline_open_time_ms(client, mode, exchange.product, market, interval)?
        .map(|value| value + interval_ms)
        .unwrap_or(fallback_start_ms);

    if start_ms > max_open_time_ms {
        return Ok(0);
    }

    let mut cursor_ms = max_open_time_ms + interval_ms;
    let mut inserted_rows = 0u64;

    while cursor_ms > start_ms {
        let url = format!(
            "{}/{unit}?market={market}&to={to}&count={count}",
            exchange.candles_base_url,
            unit = 1,
            to = format_cursor(cursor_ms, exchange.cursor_timezone)?,
            count = DEFAULT_COUNT
        );
        let mut rows = fetch_exchange_candles(
            http,
            &url,
            exchange.name,
            market,
            request_gap,
            rate_limit_sleep,
        )?;

        if rows.is_empty() {
            break;
        }

        rows.reverse();
        let mut oldest_open_time_ms = cursor_ms - interval_ms;
        for row in rows {
            let record = parse_candle_row(mode, exchange.product, interval, row)?;
            oldest_open_time_ms = record.open_time_ms;
            if record.open_time_ms < start_ms {
                continue;
            }
            if record.open_time_ms > max_open_time_ms {
                continue;
            }
            insert_kline(client, &record)?;
            inserted_rows += 1;
        }

        if oldest_open_time_ms <= start_ms {
            break;
        }
        cursor_ms = oldest_open_time_ms;
    }

    Ok(inserted_rows)
}

fn fetch_exchange_candles(
    http: &Client,
    url: &str,
    exchange_name: &str,
    market: &str,
    request_gap: StdDuration,
    rate_limit_sleep: StdDuration,
) -> Result<Vec<KrCandle>> {
    loop {
        let response = http
            .get(url)
            .send()
            .with_context(|| format!("failed to fetch {} candles for {}", exchange_name, market))?;
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            warn!(
                service = "postgres-kr-backfill",
                exchange = exchange_name,
                market = market,
                sleep_sec = rate_limit_sleep.as_secs(),
                "exchange rate limit hit"
            );
            std::thread::sleep(rate_limit_sleep);
            continue;
        }
        let rows = response
            .error_for_status()
            .with_context(|| format!("{} candle HTTP status error for {}", exchange_name, market))?
            .json::<Vec<KrCandle>>()
            .with_context(|| {
                format!("failed to decode {} candles for {}", exchange_name, market)
            })?;
        std::thread::sleep(request_gap);
        return Ok(rows);
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
        .context("failed to query latest kline open_time")
}

fn parse_candle_row(
    _mode: BinanceMode,
    product: &str,
    interval: &str,
    row: KrCandle,
) -> Result<PostgresKlineRecord> {
    let open_time = NaiveDateTime::parse_from_str(&row.candle_date_time_utc, "%Y-%m-%dT%H:%M:%S")
        .context("failed to parse candle_date_time_utc")?;
    let open_time_ms = Utc.from_utc_datetime(&open_time).timestamp_millis();
    let interval_ms = interval_millis(interval)?;

    Ok(PostgresKlineRecord {
        product: product.to_string(),
        symbol: row.market.clone(),
        interval_name: interval.to_string(),
        open_time_ms,
        close_time_ms: open_time_ms + interval_ms - 1,
        open: row.opening_price,
        high: row.high_price,
        low: row.low_price,
        close: row.trade_price,
        volume: row.candle_acc_trade_volume,
        quote_volume: row.candle_acc_trade_price,
        trade_count: 0,
        taker_buy_base_volume: None,
        taker_buy_quote_volume: None,
        raw_payload: serde_json::to_string(&row).context("failed to encode raw candle payload")?,
    })
}

fn configured_assets() -> Vec<String> {
    std::env::var("SANDBOX_QUANT_KR_BACKFILL_ASSETS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|asset| asset.trim().to_ascii_uppercase())
                .filter(|asset| !asset.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|assets| !assets.is_empty())
        .unwrap_or_else(|| {
            DEFAULT_ASSETS
                .iter()
                .map(|asset| asset.to_string())
                .collect()
        })
}

fn configured_mode() -> BinanceMode {
    match std::env::var("BINANCE_MODE").ok().as_deref() {
        Some("real") => BinanceMode::Real,
        _ => BinanceMode::Demo,
    }
}

fn configured_interval() -> String {
    std::env::var("SANDBOX_QUANT_KR_BACKFILL_INTERVAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_INTERVAL.to_string())
}

fn configured_continuous() -> bool {
    matches!(
        std::env::var("SANDBOX_QUANT_KR_BACKFILL_CONTINUOUS")
            .ok()
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn configured_poll_seconds() -> u64 {
    std::env::var("SANDBOX_QUANT_KR_BACKFILL_POLL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_POLL_SECONDS)
}

fn configured_request_gap() -> StdDuration {
    let millis = std::env::var("SANDBOX_QUANT_KR_BACKFILL_REQUEST_GAP_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_REQUEST_GAP_MS);
    StdDuration::from_millis(millis)
}

fn configured_rate_limit_sleep() -> StdDuration {
    let seconds = std::env::var("SANDBOX_QUANT_KR_BACKFILL_RATE_LIMIT_SLEEP_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_RATE_LIMIT_SLEEP_SECONDS);
    StdDuration::from_secs(seconds)
}

fn configured_fallback_start_ms() -> Result<i64> {
    let raw = std::env::var("SANDBOX_QUANT_KR_BACKFILL_FROM")
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
    anyhow::bail!("unsupported SANDBOX_QUANT_KR_BACKFILL_FROM format: {raw}")
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

fn format_cursor(cursor_ms: i64, timezone: CursorTimezone) -> Result<String> {
    let datetime = Utc
        .timestamp_millis_opt(cursor_ms)
        .single()
        .context("invalid cursor timestamp")?;
    match timezone {
        CursorTimezone::UtcZulu => Ok(datetime.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        CursorTimezone::KstNaive => {
            let kst = FixedOffset::east_opt(9 * 3600).context("invalid KST offset")?;
            Ok(datetime
                .with_timezone(&kst)
                .format("%Y-%m-%dT%H:%M:%S")
                .to_string())
        }
    }
}
