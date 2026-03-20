use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{DateTime, FixedOffset, Utc};
use postgres::Client;
use sandbox_quant::dataset::types::RecorderMetrics;
use sandbox_quant::error::storage_error::StorageError;
use sandbox_quant::storage::postgres_market_data::{connect, init_schema, postgres_url_from_env};
use serde::Serialize;
use serde_json::Value;

const EXIT_HEALTHY: u8 = 0;
const EXIT_STALE: u8 = 20;
const EXIT_ERROR: u8 = 30;

fn main() -> ExitCode {
    dotenvy::dotenv().ok();

    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(EXIT_ERROR)
        }
    }
}

fn run() -> Result<u8, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("probe") => run_probe(&args[1..]),
        _ => Err(
            "usage: sandbox-quant-watchdog probe [--heartbeat-log <path>] [--postgres-url <url>] [--market-max-age-sec <sec>] [--heartbeat-max-age-sec <sec>] [--liquidation-max-age-sec <sec>]"
                .into(),
        ),
    }
}

fn run_probe(args: &[String]) -> Result<u8, Box<dyn std::error::Error>> {
    let config = ProbeConfig::parse(args)?;
    let checked_at = Utc::now();
    let heartbeat = read_log_snapshot(&config.heartbeat_log)?;
    let metrics = load_metrics(&config.postgres_url);

    let report = build_probe_report(config, checked_at, metrics, heartbeat);
    println!("{}", serde_json::to_string_pretty(&report)?);

    Ok(match report.status.as_str() {
        "healthy" => EXIT_HEALTHY,
        "stale" => EXIT_STALE,
        _ => EXIT_ERROR,
    })
}

#[derive(Debug, Clone)]
struct ProbeConfig {
    heartbeat_log: PathBuf,
    postgres_url: String,
    market_max_age_sec: i64,
    heartbeat_max_age_sec: i64,
    liquidation_max_age_sec: Option<i64>,
}

impl ProbeConfig {
    fn parse(args: &[String]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut heartbeat_log: Option<PathBuf> = None;
        let mut postgres_url: Option<String> = None;
        let mut market_max_age_sec = 240i64;
        let mut heartbeat_max_age_sec = 240i64;
        let mut liquidation_max_age_sec = None;

        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--heartbeat-log" => {
                    let value = args
                        .get(index + 1)
                        .ok_or("missing value for --heartbeat-log")?;
                    heartbeat_log = Some(PathBuf::from(value));
                    index += 2;
                }
                "--postgres-url" => {
                    let value = args
                        .get(index + 1)
                        .ok_or("missing value for --postgres-url")?;
                    postgres_url = Some(value.clone());
                    index += 2;
                }
                "--market-max-age-sec" => {
                    let value = args
                        .get(index + 1)
                        .ok_or("missing value for --market-max-age-sec")?;
                    market_max_age_sec = value.parse()?;
                    index += 2;
                }
                "--heartbeat-max-age-sec" => {
                    let value = args
                        .get(index + 1)
                        .ok_or("missing value for --heartbeat-max-age-sec")?;
                    heartbeat_max_age_sec = value.parse()?;
                    index += 2;
                }
                "--liquidation-max-age-sec" => {
                    let value = args
                        .get(index + 1)
                        .ok_or("missing value for --liquidation-max-age-sec")?;
                    let parsed: i64 = value.parse()?;
                    liquidation_max_age_sec = if parsed <= 0 { None } else { Some(parsed) };
                    index += 2;
                }
                other => return Err(format!("unsupported arg: {other}").into()),
            }
        }

        let heartbeat_log =
            heartbeat_log.unwrap_or_else(|| PathBuf::from("var/log/recorder.jsonl"));
        let postgres_url =
            postgres_url.unwrap_or_else(|| postgres_url_from_env().unwrap_or_default());
        if postgres_url.trim().is_empty() {
            return Err(
                "postgres URL missing; set SANDBOX_QUANT_POSTGRES_URL or pass --postgres-url"
                    .into(),
            );
        }

        Ok(Self {
            heartbeat_log,
            postgres_url,
            market_max_age_sec,
            heartbeat_max_age_sec,
            liquidation_max_age_sec,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
struct ProbeThresholds {
    market_max_age_sec: i64,
    heartbeat_max_age_sec: i64,
    liquidation_max_age_sec: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct ProbeReport {
    status: String,
    checked_at: String,
    heartbeat_log: String,
    thresholds: ProbeThresholds,
    metrics: Option<MetricSnapshot>,
    heartbeat: Option<HeartbeatSnapshot>,
    staleness: StalenessReport,
    diagnosis: Diagnosis,
}

#[derive(Debug, Clone, Serialize)]
struct MetricSnapshot {
    liquidation_events: u64,
    book_ticker_events: u64,
    agg_trade_events: u64,
    last_liquidation_event_time: Option<String>,
    last_book_ticker_event_time: Option<String>,
    last_agg_trade_event_time: Option<String>,
    top_liquidation_symbols: Vec<String>,
    top_book_ticker_symbols: Vec<String>,
    top_agg_trade_symbols: Vec<String>,
    ages_sec: MetricAges,
}

#[derive(Debug, Clone, Serialize)]
struct MetricAges {
    liquidation: Option<i64>,
    book_ticker: Option<i64>,
    agg_trade: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct HeartbeatSnapshot {
    ping_at: String,
    pong_at: String,
    reader_alive: bool,
    writer_alive: bool,
    worker_alive: bool,
    watched_symbols: Vec<String>,
    heartbeat_age_sec: Option<i64>,
}

#[derive(Debug, Clone)]
struct LogSnapshot {
    last_heartbeat: Option<HeartbeatSnapshot>,
    recent_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct StalenessReport {
    market_data_stale: bool,
    heartbeat_stale: bool,
    liquidation_stale: bool,
    reasons: Vec<String>,
    recent_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Diagnosis {
    category: String,
    summary: String,
}

fn build_probe_report(
    config: ProbeConfig,
    checked_at: DateTime<Utc>,
    metrics: Result<RecorderMetrics, StorageError>,
    heartbeat: LogSnapshot,
) -> ProbeReport {
    let checked_at_text = checked_at.to_rfc3339();
    let thresholds = ProbeThresholds {
        market_max_age_sec: config.market_max_age_sec,
        heartbeat_max_age_sec: config.heartbeat_max_age_sec,
        liquidation_max_age_sec: config.liquidation_max_age_sec,
    };
    match metrics {
        Ok(metrics) => {
            let ages = MetricAges {
                liquidation: age_from_text(
                    metrics.last_liquidation_event_time.as_deref(),
                    checked_at,
                ),
                book_ticker: age_from_text(
                    metrics.last_book_ticker_event_time.as_deref(),
                    checked_at,
                ),
                agg_trade: age_from_text(metrics.last_agg_trade_event_time.as_deref(), checked_at),
            };
            let metric_snapshot = MetricSnapshot {
                liquidation_events: metrics.liquidation_events,
                book_ticker_events: metrics.book_ticker_events,
                agg_trade_events: metrics.agg_trade_events,
                last_liquidation_event_time: metrics.last_liquidation_event_time.clone(),
                last_book_ticker_event_time: metrics.last_book_ticker_event_time.clone(),
                last_agg_trade_event_time: metrics.last_agg_trade_event_time.clone(),
                top_liquidation_symbols: metrics.top_liquidation_symbols,
                top_book_ticker_symbols: metrics.top_book_ticker_symbols,
                top_agg_trade_symbols: metrics.top_agg_trade_symbols,
                ages_sec: ages.clone(),
            };
            let market_age = youngest_age(ages.book_ticker, ages.agg_trade);
            let market_data_stale = market_age.is_none_or(|age| age > config.market_max_age_sec);
            let heartbeat_age = heartbeat
                .last_heartbeat
                .as_ref()
                .and_then(|snapshot| snapshot.heartbeat_age_sec);
            let heartbeat_stale = heartbeat
                .last_heartbeat
                .as_ref()
                .map(|snapshot| {
                    !snapshot.worker_alive || !snapshot.reader_alive || !snapshot.writer_alive
                })
                .unwrap_or(true)
                || heartbeat_age.is_none_or(|age| age > config.heartbeat_max_age_sec);
            let liquidation_stale = match config.liquidation_max_age_sec {
                Some(limit) => ages.liquidation.is_none_or(|age| age > limit),
                None => false,
            };

            let mut reasons = Vec::new();
            if market_data_stale {
                reasons.push(format!(
                    "market_data_age_exceeded max={} observed={}",
                    config.market_max_age_sec,
                    market_age
                        .map(|age| age.to_string())
                        .unwrap_or_else(|| "missing".to_string())
                ));
            }
            if heartbeat_stale {
                reasons.push(format!(
                    "heartbeat_age_exceeded max={} observed={} worker_alive={} reader_alive={} writer_alive={}",
                    config.heartbeat_max_age_sec,
                    heartbeat_age
                        .map(|age| age.to_string())
                        .unwrap_or_else(|| "missing".to_string()),
                    heartbeat
                        .last_heartbeat
                        .as_ref()
                        .map(|snapshot| snapshot.worker_alive)
                        .unwrap_or(false),
                    heartbeat
                        .last_heartbeat
                        .as_ref()
                        .map(|snapshot| snapshot.reader_alive)
                        .unwrap_or(false),
                    heartbeat
                        .last_heartbeat
                        .as_ref()
                        .map(|snapshot| snapshot.writer_alive)
                        .unwrap_or(false)
                ));
                if let Some(snapshot) = heartbeat.last_heartbeat.as_ref() {
                    reasons.push(format!(
                        "heartbeat_components reader_alive={} writer_alive={}",
                        snapshot.reader_alive, snapshot.writer_alive
                    ));
                }
            }
            if liquidation_stale {
                reasons.push(format!(
                    "liquidation_age_exceeded max={} observed={}",
                    config.liquidation_max_age_sec.unwrap_or_default(),
                    ages.liquidation
                        .map(|age| age.to_string())
                        .unwrap_or_else(|| "missing".to_string())
                ));
            }

            let staleness = StalenessReport {
                market_data_stale,
                heartbeat_stale,
                liquidation_stale,
                reasons,
                recent_errors: heartbeat.recent_errors.clone(),
            };
            let diagnosis = infer_diagnosis(None, &staleness);
            let status = if staleness.market_data_stale
                || staleness.heartbeat_stale
                || staleness.liquidation_stale
            {
                "stale".to_string()
            } else {
                "healthy".to_string()
            };

            ProbeReport {
                status,
                checked_at: checked_at_text,
                heartbeat_log: config.heartbeat_log.display().to_string(),
                thresholds,
                metrics: Some(metric_snapshot),
                heartbeat: heartbeat.last_heartbeat,
                staleness,
                diagnosis,
            }
        }
        Err(error) => {
            let storage_error = error.to_string();
            let staleness = StalenessReport {
                market_data_stale: true,
                heartbeat_stale: heartbeat.last_heartbeat.is_none(),
                liquidation_stale: false,
                reasons: vec![format!("postgres_probe_failed error={storage_error}")],
                recent_errors: heartbeat.recent_errors.clone(),
            };
            let diagnosis = infer_diagnosis(Some(&storage_error), &staleness);
            ProbeReport {
                status: "error".to_string(),
                checked_at: checked_at_text,
                heartbeat_log: config.heartbeat_log.display().to_string(),
                thresholds,
                metrics: None,
                heartbeat: heartbeat.last_heartbeat,
                staleness,
                diagnosis,
            }
        }
    }
}

fn load_metrics(url: &str) -> Result<RecorderMetrics, StorageError> {
    let mut client = connect(url)?;
    let _ = init_schema(&mut client, url)?;
    Ok(RecorderMetrics {
        liquidation_events: query_count_for_table(&mut client, "raw_liquidation_events")?,
        book_ticker_events: query_count_for_table(&mut client, "raw_book_ticker")?,
        agg_trade_events: query_count_for_table(&mut client, "raw_agg_trades")?,
        derived_kline_1s_bars: 0,
        schema_version: query_schema_version(&mut client)?,
        last_liquidation_event_time: query_latest_timestamp_for_table(
            &mut client,
            "raw_liquidation_events",
            "event_time",
        )?,
        last_book_ticker_event_time: query_latest_timestamp_for_table(
            &mut client,
            "raw_book_ticker",
            "event_time",
        )?,
        last_agg_trade_event_time: query_latest_timestamp_for_table(
            &mut client,
            "raw_agg_trades",
            "event_time",
        )?,
        top_liquidation_symbols: query_top_symbols_for_table(
            &mut client,
            "raw_liquidation_events",
        )?,
        top_book_ticker_symbols: query_top_symbols_for_table(&mut client, "raw_book_ticker")?,
        top_agg_trade_symbols: query_top_symbols_for_table(&mut client, "raw_agg_trades")?,
    })
}

fn query_schema_version(client: &mut Client) -> Result<Option<String>, StorageError> {
    client
        .query_opt(
            "SELECT value FROM schema_metadata WHERE key = 'market_data_schema_version'",
            &[],
        )
        .map_err(storage_err)
        .map(|row| row.map(|row| row.get(0)))
}

fn query_count_for_table(client: &mut Client, table: &str) -> Result<u64, StorageError> {
    client
        .query_one(&format!("SELECT COUNT(*) FROM {table}"), &[])
        .map_err(storage_err)
        .map(|row| row.get::<_, i64>(0).max(0) as u64)
}

fn query_latest_timestamp_for_table(
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

fn query_top_symbols_for_table(
    client: &mut Client,
    table: &str,
) -> Result<Vec<String>, StorageError> {
    client
        .query(
            &format!(
                "SELECT symbol, COUNT(*) AS row_count
                 FROM {table}
                 GROUP BY symbol
                 ORDER BY row_count DESC, symbol ASC
                 LIMIT 5"
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

fn storage_err(error: postgres::Error) -> StorageError {
    StorageError::WriteFailedWithContext {
        message: error.to_string(),
    }
}

fn youngest_age(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn infer_diagnosis(storage_error: Option<&str>, staleness: &StalenessReport) -> Diagnosis {
    let recent_error = staleness.recent_errors.first().map(String::as_str);
    let category = if let Some(error) = storage_error {
        if contains_any(error, &["missing", "postgres URL"]) {
            "postgres_config"
        } else if contains_any(
            error,
            &[
                "Connection refused",
                "connection refused",
                "error connecting to server",
                "failed to lookup address information",
                "timeout",
            ],
        ) {
            "postgres_unreachable"
        } else {
            "postgres_probe_failed"
        }
    } else if !staleness.market_data_stale
        && !staleness.heartbeat_stale
        && !staleness.liquidation_stale
    {
        "healthy"
    } else if let Some(error) = recent_error {
        if contains_any(error, &["failed to lookup address information"]) {
            "dns_failure"
        } else if contains_any(error, &["failed to connect forceOrder stream"]) {
            "liquidation_stream_connect"
        } else if contains_any(error, &["failed to connect symbol streams"]) {
            "market_stream_connect"
        } else if contains_any(
            error,
            &[
                "postgres liquidation writer disconnected",
                "postgres book_ticker writer disconnected",
                "postgres agg_trade writer disconnected",
                "postgres recorder backend missing URL",
            ],
        ) {
            "postgres_writer"
        } else if contains_any(error, &["process failed", "process panicked"]) {
            "process_exit"
        } else {
            "runtime_error"
        }
    } else if staleness.heartbeat_stale && staleness.market_data_stale {
        "recorder_down"
    } else if staleness.market_data_stale {
        "market_data_stale"
    } else if staleness.liquidation_stale {
        "liquidation_stale"
    } else {
        "healthy"
    };

    let summary = match category {
        "healthy" => "market data and recorder heartbeat are fresh".to_string(),
        "postgres_config" => "postgres URL is missing or invalid; reload env before restarting".to_string(),
        "postgres_unreachable" => {
            "postgres probe failed because the database is unreachable; recover postgres first".to_string()
        }
        "dns_failure" => "recorder could not resolve an exchange endpoint; check network and restart the recorder".to_string(),
        "liquidation_stream_connect" => {
            "forceOrder stream reconnects are failing; restart the recorder and verify Binance futures connectivity".to_string()
        }
        "market_stream_connect" => {
            "symbol stream reconnects are failing; restart the recorder and verify watched symbols and network".to_string()
        }
        "postgres_writer" => {
            "the recorder postgres writer failed; reload postgres env and restart the recorder".to_string()
        }
        "process_exit" => "the recorder process exited; inspect recent recorder logs and restart it".to_string(),
        "recorder_down" => "heartbeat and market tables are stale; the recorder is likely stopped or hung".to_string(),
        "market_data_stale" => "market tables are stale even though the process still emits logs; restart the recorder".to_string(),
        "liquidation_stale" => "liquidation data exceeded the configured age threshold".to_string(),
        _ => "watchdog found a stale or failed recorder state; inspect the recent recorder errors".to_string(),
    };

    Diagnosis {
        category: category.to_string(),
        summary,
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn read_log_snapshot(path: &PathBuf) -> Result<LogSnapshot, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(LogSnapshot {
            last_heartbeat: None,
            recent_errors: vec![format!("heartbeat log missing: {}", path.display())],
        });
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut tail = VecDeque::with_capacity(256);
    for line in reader.lines() {
        let line = line?;
        if tail.len() == 256 {
            tail.pop_front();
        }
        tail.push_back(line);
    }

    let mut last_heartbeat = None;
    let mut last_heartbeat_at = None;
    let mut recent_errors = Vec::new();

    for line in tail.into_iter().rev() {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if last_heartbeat.is_none()
            && value
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "heartbeat")
        {
            if let Some((snapshot, heartbeat_at)) = parse_heartbeat(&value) {
                last_heartbeat = Some(snapshot);
                last_heartbeat_at = Some(heartbeat_at);
            }
        }
        let level = value
            .get("level")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let event_time = parse_log_entry_time(&value);
        if matches!(level, "ERROR" | "WARN") && error_is_relevant(event_time, last_heartbeat_at) {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown log message");
            let mut rendered = message.to_string();
            if let Some(error) = value.get("error").and_then(Value::as_str) {
                rendered.push_str(" error=");
                rendered.push_str(error);
            }
            if !recent_errors.contains(&rendered) {
                recent_errors.push(rendered);
            }
            if recent_errors.len() == 5 {
                break;
            }
        }
    }

    Ok(LogSnapshot {
        last_heartbeat,
        recent_errors,
    })
}

fn parse_heartbeat(value: &Value) -> Option<(HeartbeatSnapshot, DateTime<Utc>)> {
    let ping_at = value.get("ping_at")?.as_str()?.to_string();
    let pong_at = value.get("pong_at")?.as_str()?.to_string();
    let reader_alive = value
        .get("reader_alive")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            value
                .get("worker_alive")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
    let writer_alive = value
        .get("writer_alive")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            value
                .get("worker_alive")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
    let worker_alive = value
        .get("worker_alive")
        .and_then(Value::as_bool)
        .unwrap_or(reader_alive && writer_alive);
    let watched_symbols = value
        .get("watched_symbols")
        .and_then(Value::as_str)
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|symbol| !symbol.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let heartbeat_at = parse_datetime_text(&pong_at)?;
    let heartbeat_age_sec = Some((Utc::now() - heartbeat_at).num_seconds());
    Some((
        HeartbeatSnapshot {
            ping_at,
            pong_at,
            reader_alive,
            writer_alive,
            worker_alive,
            watched_symbols,
            heartbeat_age_sec,
        },
        heartbeat_at,
    ))
}

fn parse_log_entry_time(value: &Value) -> Option<DateTime<Utc>> {
    value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_datetime_text)
}

fn error_is_relevant(
    event_time: Option<DateTime<Utc>>,
    last_heartbeat_at: Option<DateTime<Utc>>,
) -> bool {
    match (event_time, last_heartbeat_at) {
        (Some(event_time), Some(last_heartbeat_at)) => event_time >= last_heartbeat_at,
        _ => true,
    }
}

fn age_from_text(value: Option<&str>, now: DateTime<Utc>) -> Option<i64> {
    let timestamp = parse_datetime_text(value?)?;
    Some((now - timestamp).num_seconds())
}

fn parse_datetime_text(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
        return Some(timestamp.with_timezone(&Utc));
    }

    let normalized = normalize_postgres_offset(value);
    DateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S%.f%z")
        .or_else(|_| DateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S%z"))
        .map(|timestamp: DateTime<FixedOffset>| timestamp.with_timezone(&Utc))
        .ok()
}

fn normalize_postgres_offset(value: &str) -> String {
    if value.len() >= 3 {
        let suffix = &value[value.len() - 3..];
        let bytes = suffix.as_bytes();
        if (bytes[0] == b'+' || bytes[0] == b'-')
            && bytes[1].is_ascii_digit()
            && bytes[2].is_ascii_digit()
        {
            return format!("{value}00");
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        error_is_relevant, infer_diagnosis, normalize_postgres_offset, parse_datetime_text,
        youngest_age, StalenessReport,
    };

    #[test]
    fn parses_postgres_timestamp_without_minutes_in_offset() {
        let parsed =
            parse_datetime_text("2026-03-18 01:48:35.447458+00").expect("timestamp should parse");
        assert_eq!(parsed.to_rfc3339(), "2026-03-18T01:48:35.447458+00:00");
    }

    #[test]
    fn appends_minutes_to_short_postgres_offset() {
        assert_eq!(
            normalize_postgres_offset("2026-03-18 01:48:35+09"),
            "2026-03-18 01:48:35+0900"
        );
    }

    #[test]
    fn chooses_youngest_market_age() {
        assert_eq!(youngest_age(Some(15), Some(9)), Some(9));
        assert_eq!(youngest_age(Some(15), None), Some(15));
        assert_eq!(youngest_age(None, None), None);
    }

    #[test]
    fn diagnosis_prefers_recent_runtime_error() {
        let staleness = StalenessReport {
            market_data_stale: true,
            heartbeat_stale: true,
            liquidation_stale: false,
            reasons: vec!["market_data_age_exceeded".to_string()],
            recent_errors: vec!["failed to connect symbol streams error=timeout".to_string()],
        };
        let diagnosis = infer_diagnosis(None, &staleness);
        assert_eq!(diagnosis.category, "market_stream_connect");
    }

    #[test]
    fn diagnosis_flags_missing_postgres_config() {
        let staleness = StalenessReport {
            market_data_stale: true,
            heartbeat_stale: false,
            liquidation_stale: false,
            reasons: vec![],
            recent_errors: vec![],
        };
        let diagnosis = infer_diagnosis(
            Some("postgres URL missing; set SANDBOX_QUANT_POSTGRES_URL"),
            &staleness,
        );
        assert_eq!(diagnosis.category, "postgres_config");
    }

    #[test]
    fn diagnosis_is_healthy_when_no_staleness_flags_are_set() {
        let staleness = StalenessReport {
            market_data_stale: false,
            heartbeat_stale: false,
            liquidation_stale: false,
            reasons: vec![],
            recent_errors: vec!["process failed error=old".to_string()],
        };
        let diagnosis = infer_diagnosis(None, &staleness);
        assert_eq!(diagnosis.category, "healthy");
    }

    #[test]
    fn relevant_errors_must_be_newer_than_latest_heartbeat() {
        let heartbeat_at =
            parse_datetime_text("2026-03-18T03:04:10.554027+00:00").expect("heartbeat time");
        let old_error =
            parse_datetime_text("2026-03-18T03:03:59.000000+00:00").expect("old error time");
        let new_error =
            parse_datetime_text("2026-03-18T03:04:11.000000+00:00").expect("new error time");
        assert!(!error_is_relevant(Some(old_error), Some(heartbeat_at)));
        assert!(error_is_relevant(Some(new_error), Some(heartbeat_at)));
    }
}
