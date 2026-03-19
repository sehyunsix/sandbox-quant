use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use reqwest::blocking::Client;
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::app::cli::normalize_instrument_symbol;
use sandbox_quant::market_data::binance_kline_backfill::{
    parse_start_time, run_binance_kline_backfill, BinanceKlineBackfillConfig,
    DEFAULT_BINANCE_BACKFILL_INTERVAL, DEFAULT_BINANCE_BACKFILL_PRODUCT,
    DEFAULT_BINANCE_BACKFILL_START_DATE,
};
use sandbox_quant::observability::logging::init_logging;
use sandbox_quant::record::coordination::RecorderCoordination;
use sandbox_quant::recorder_app::runtime::MarketDataRecorder;
use sandbox_quant::recorder_app::terminal::RecorderTerminal;
use sandbox_quant::storage::postgres_market_data::{
    market_freshness_for_postgres_url, postgres_url_from_env, PostgresMarketFreshness,
};
use sandbox_quant::terminal::loop_shell::run_terminal;
use sandbox_quant::ui::recorder_output::render_live_recorder_status;
use serde::Serialize;
use serde_json::json;
use std::path::Path;
use tracing::{error, info, warn};

const DEFAULT_RECORDER_SERVER_ADDR: &str = "127.0.0.1:9781";
const RECORDER_RUNTIME_MODE: BinanceMode = BinanceMode::Demo;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    bootstrap_recorder_env();
    let args: Vec<String> = std::env::args().skip(1).collect();
    init_logging("recorder", None)?;
    info!(service = "recorder", args = ?args, "process started");

    let result: Result<(), Box<dyn std::error::Error>> = if args.is_empty()
        || matches!(
            args.first().map(String::as_str),
            Some("--mode") | Some("--base-dir")
        ) {
        let (mode, base_dir) = parse_terminal_args(&args)?;
        let mut terminal = RecorderTerminal::new(mode, base_dir);
        run_terminal(&mut terminal)
    } else {
        match args.first().map(String::as_str) {
            Some("run") | Some("serve") => serve_recorder(&args[1..]),
            Some("status") => {
                print!("{}", request_recorder_server("GET", "/status", parse_server_addr(&args[1..])?)?);
                Ok(())
            }
            Some("health") => {
                print!("{}", request_recorder_server("GET", "/health", parse_server_addr(&args[1..])?)?);
                Ok(())
            }
            Some("freshness") => {
                print!(
                    "{}",
                    request_recorder_server("GET", "/freshness", parse_server_addr(&args[1..])?)?
                );
                Ok(())
            }
            Some("stop") => {
                print!("{}", request_recorder_server("POST", "/stop", parse_server_addr(&args[1..])?)?);
                Ok(())
            }
            _ => {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "usage: sandbox-quant-recorder [--mode <demo|real>] [--base-dir <path>]\n       sandbox-quant-recorder serve [symbols...] [--base-dir <path>] [--backfill] [--backfill-poll-seconds <n>] [--listen <addr>]\n       sandbox-quant-recorder run [symbols...] [--base-dir <path>] [--backfill] [--backfill-poll-seconds <n>] [--listen <addr>]\n       sandbox-quant-recorder status [--listen <addr>]\n       sandbox-quant-recorder health [--listen <addr>]\n       sandbox-quant-recorder freshness [--listen <addr>]\n       sandbox-quant-recorder stop [--listen <addr>]",
                )
                .into())
            }
        }
    };
    if let Err(error) = result {
        error!(service = "recorder", error = %error, "process failed");
        return Err(error);
    }
    info!(service = "recorder", "process completed");
    Ok(())
}

fn bootstrap_recorder_env() {
    if std::env::var_os("SANDBOX_QUANT_DISABLE_DOTENV").is_none() {
        dotenvy::dotenv().ok();
        for path in [
            Path::new("ops/grafana/.env").to_path_buf(),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("ops/grafana/.env"),
        ] {
            if path.exists() {
                let _ = dotenvy::from_path(&path);
            }
        }
    }

    if std::env::var_os("SANDBOX_QUANT_POSTGRES_URL").is_none()
        && std::env::var_os("DATABASE_URL").is_none()
    {
        if let Ok(url) = postgres_url_from_env() {
            std::env::set_var("SANDBOX_QUANT_POSTGRES_URL", url);
        }
    }

    if std::env::var_os("SANDBOX_QUANT_RECORDER_STORAGE").is_none()
        && std::env::var_os("SANDBOX_QUANT_POSTGRES_URL").is_some()
    {
        std::env::set_var("SANDBOX_QUANT_RECORDER_STORAGE", "postgres");
    }
}

#[derive(Debug)]
struct RecorderDaemon {
    recorder: MarketDataRecorder,
    backfill_worker: Option<BackfillWorker>,
    last_heartbeat_log: Instant,
}

#[derive(Debug)]
struct BackfillWorker {
    stop_flag: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

#[derive(Clone)]
struct RecorderServerState {
    daemon: Arc<Mutex<RecorderDaemon>>,
    shutdown: Arc<AtomicBool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecorderRunConfig {
    base_dir: String,
    symbols: Vec<String>,
    backfill: bool,
    backfill_poll_seconds: u64,
    listen_addr: String,
}

#[derive(Debug, Serialize)]
struct RecorderHealthResponse {
    state: String,
    reader_alive: bool,
    writer_alive: bool,
    worker_alive: bool,
    heartbeat_age_sec: i64,
    storage_backend: String,
    watched_symbols: Vec<String>,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct RecorderFreshnessResponse {
    state: String,
    storage_backend: String,
    reader_alive: bool,
    writer_alive: bool,
    worker_alive: bool,
    backfill_enabled: bool,
    backfill_alive: bool,
    watched_symbols: Vec<String>,
    postgres: Option<PostgresMarketFreshness>,
}

fn serve_recorder(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_run_args(args)?;
    let base_dir = config.base_dir.clone();
    let symbols = config.symbols.clone();
    let coordination = RecorderCoordination::new(base_dir.clone());
    let strategy_symbols = coordination.strategy_symbols(RECORDER_RUNTIME_MODE)?;
    let mut recorder = MarketDataRecorder::new(base_dir.clone());
    let status = recorder.start(RECORDER_RUNTIME_MODE, symbols, strategy_symbols)?;
    println!("{}", render_live_recorder_status("record running", &status));
    let backfill_worker = if config.backfill {
        Some(spawn_backfill_worker(&config)?)
    } else {
        None
    };
    let daemon = Arc::new(Mutex::new(RecorderDaemon {
        recorder,
        backfill_worker,
        last_heartbeat_log: Instant::now()
            .checked_sub(Duration::from_secs(5))
            .unwrap_or_else(Instant::now),
    }));
    let shutdown = Arc::new(AtomicBool::new(false));
    let state = RecorderServerState {
        daemon: daemon.clone(),
        shutdown: shutdown.clone(),
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let app = Router::new()
            .route("/status", get(recorder_status_handler))
            .route("/health", get(recorder_health_handler))
            .route("/freshness", get(recorder_freshness_handler))
            .route("/stop", post(recorder_stop_handler))
            .with_state(state.clone());

        let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
        info!(
            service = "recorder",
            listen_addr = %config.listen_addr,
            "recorder control server listening"
        );

        let supervisor_state = state.clone();
        let supervisor = tokio::spawn(async move {
            loop {
                if supervisor_state.shutdown.load(Ordering::Relaxed) {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                let mut daemon = match supervisor_state.daemon.lock() {
                    Ok(daemon) => daemon,
                    Err(_) => continue,
                };
                let status = daemon.recorder.status(RECORDER_RUNTIME_MODE);
                if daemon.last_heartbeat_log.elapsed() >= Duration::from_secs(5) {
                    info!(
                        service = "recorder",
                        kind = "heartbeat",
                        ping_at = %Utc::now().to_rfc3339(),
                        pong_at = %status.updated_at.to_rfc3339(),
                        heartbeat_age_sec = status.heartbeat_age_sec,
                        reader_alive = status.reader_alive,
                        writer_alive = status.writer_alive,
                        worker_alive = status.worker_alive,
                        storage_backend = status.storage_backend,
                        watched_symbols = %status.watched_symbols.join(","),
                        "heartbeat ping/pong"
                    );
                    daemon.last_heartbeat_log = Instant::now();
                }
                if !status.worker_alive {
                    let manual_symbols = status.manual_symbols.clone();
                    warn!(
                        service = "recorder",
                        "worker not alive; restarting streams"
                    );
                    let _ = daemon
                        .recorder
                        .update_manual_symbols(RECORDER_RUNTIME_MODE, manual_symbols);
                }
            }
        });

        let shutdown_signal = async move {
            while !shutdown.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        };

        let result = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await;
        let _ = supervisor.await;

        let mut daemon = daemon
            .lock()
            .map_err(|_| "failed to lock recorder daemon state")?;
        let _ = daemon.recorder.stop(RECORDER_RUNTIME_MODE);
        if let Some(worker) = daemon.backfill_worker.take() {
            worker.stop_flag.store(true, Ordering::Relaxed);
            let _ = worker.handle.join();
        }
        info!(service = "recorder", "serve loop completed");
        result.map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
    })
}

fn spawn_backfill_worker(
    config: &RecorderRunConfig,
) -> Result<BackfillWorker, Box<dyn std::error::Error>> {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let worker_stop_flag = stop_flag.clone();
    let backfill_config = BinanceKlineBackfillConfig {
        postgres_url: std::env::var("SANDBOX_QUANT_POSTGRES_URL")?,
        symbols: config.symbols.clone(),
        mode: RECORDER_RUNTIME_MODE,
        product: DEFAULT_BINANCE_BACKFILL_PRODUCT.to_string(),
        interval: DEFAULT_BINANCE_BACKFILL_INTERVAL.to_string(),
        fallback_start_ms: parse_start_time(DEFAULT_BINANCE_BACKFILL_START_DATE)?,
        continuous: true,
        poll_seconds: config.backfill_poll_seconds,
    };
    let handle = std::thread::Builder::new()
        .name("market-recorder-kline-backfill".to_string())
        .spawn(move || {
            if let Err(error) = run_binance_kline_backfill(&backfill_config, Some(worker_stop_flag))
            {
                error!(
                    service = "recorder",
                    backfill = "binance-kline",
                    error = %error,
                    "in-process kline backfill stopped with error"
                );
            }
        })?;
    info!(
        service = "recorder",
        backfill_symbols = %config.symbols.join(","),
        backfill_poll_seconds = config.backfill_poll_seconds,
        "spawned in-process kline backfill worker"
    );
    Ok(BackfillWorker { stop_flag, handle })
}

async fn recorder_status_handler(
    State(state): State<RecorderServerState>,
) -> Result<String, axum::http::StatusCode> {
    let daemon = state
        .daemon
        .lock()
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let status = daemon.recorder.status(RECORDER_RUNTIME_MODE);
    Ok(render_live_recorder_status("record status", &status))
}

async fn recorder_health_handler(
    State(state): State<RecorderServerState>,
) -> Result<Json<RecorderHealthResponse>, axum::http::StatusCode> {
    let daemon = state
        .daemon
        .lock()
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let status = daemon.recorder.status(RECORDER_RUNTIME_MODE);
    Ok(Json(RecorderHealthResponse {
        state: status.state.as_str().to_string(),
        reader_alive: status.reader_alive,
        writer_alive: status.writer_alive,
        worker_alive: status.worker_alive,
        heartbeat_age_sec: status.heartbeat_age_sec,
        storage_backend: status.storage_backend,
        watched_symbols: status.watched_symbols,
        last_error: status.last_error,
    }))
}

async fn recorder_freshness_handler(
    State(state): State<RecorderServerState>,
) -> Result<Json<RecorderFreshnessResponse>, axum::http::StatusCode> {
    tokio::task::spawn_blocking(move || {
        let daemon = state
            .daemon
            .lock()
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        let status = daemon.recorder.status(RECORDER_RUNTIME_MODE);
        let backfill_enabled = daemon.backfill_worker.is_some();
        let backfill_alive = daemon
            .backfill_worker
            .as_ref()
            .is_some_and(|worker| !worker.handle.is_finished());
        let postgres = if status.storage_backend == "postgres" {
            std::env::var("SANDBOX_QUANT_POSTGRES_URL")
                .ok()
                .and_then(|url| market_freshness_for_postgres_url(&url, "1m").ok())
        } else {
            None
        };
        Ok(Json(RecorderFreshnessResponse {
            state: status.state.as_str().to_string(),
            storage_backend: status.storage_backend,
            reader_alive: status.reader_alive,
            writer_alive: status.writer_alive,
            worker_alive: status.worker_alive,
            backfill_enabled,
            backfill_alive,
            watched_symbols: status.watched_symbols,
            postgres,
        }))
    })
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
}

async fn recorder_stop_handler(State(state): State<RecorderServerState>) -> impl IntoResponse {
    state.shutdown.store(true, Ordering::Relaxed);
    Json(json!({ "status": "stopping" }))
}

fn request_recorder_server(
    method: &str,
    path: &str,
    listen_addr: String,
) -> Result<String, Box<dyn std::error::Error>> {
    let client = Client::builder().build()?;
    let url = format!("http://{listen_addr}{path}");
    let response = match method {
        "GET" => client.get(url).send()?,
        "POST" => client.post(url).send()?,
        other => return Err(format!("unsupported method: {other}").into()),
    }
    .error_for_status()?;
    Ok(response.text()?)
}

fn parse_server_addr(args: &[String]) -> Result<String, Box<dyn std::error::Error>> {
    let mut listen_addr = DEFAULT_RECORDER_SERVER_ADDR.to_string();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--listen" => {
                listen_addr = args
                    .get(index + 1)
                    .ok_or("missing value for --listen")?
                    .clone();
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }
    Ok(listen_addr)
}

fn parse_terminal_args(
    args: &[String],
) -> Result<(BinanceMode, String), Box<dyn std::error::Error>> {
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--mode" => {
                let value = args.get(index + 1).ok_or("missing value for --mode")?;
                mode = match value.as_str() {
                    "demo" => BinanceMode::Demo,
                    "real" => BinanceMode::Real,
                    _ => return Err(format!("unsupported mode: {value}").into()),
                };
                index += 2;
            }
            "--base-dir" => {
                let value = args.get(index + 1).ok_or("missing value for --base-dir")?;
                base_dir = value.clone();
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }
    Ok((mode, base_dir))
}

fn parse_run_args(args: &[String]) -> Result<RecorderRunConfig, Box<dyn std::error::Error>> {
    let mut base_dir = "var".to_string();
    let mut symbols = Vec::new();
    let mut backfill = false;
    let mut backfill_poll_seconds = 30u64;
    let mut listen_addr = DEFAULT_RECORDER_SERVER_ADDR.to_string();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--base-dir" => {
                let value = args.get(index + 1).ok_or("missing value for --base-dir")?;
                base_dir = value.clone();
                index += 2;
            }
            "--backfill" => {
                backfill = true;
                index += 1;
            }
            "--backfill-poll-seconds" => {
                let value = args
                    .get(index + 1)
                    .ok_or("missing value for --backfill-poll-seconds")?;
                backfill_poll_seconds = value.parse::<u64>()?;
                index += 2;
            }
            "--listen" => {
                listen_addr = args
                    .get(index + 1)
                    .ok_or("missing value for --listen")?
                    .clone();
                index += 2;
            }
            raw if raw.starts_with("--") => {
                return Err(format!("unsupported arg: {raw}").into());
            }
            raw => {
                symbols.push(normalize_instrument_symbol(raw));
                index += 1;
            }
        }
    }
    Ok(RecorderRunConfig {
        base_dir,
        symbols,
        backfill,
        backfill_poll_seconds,
        listen_addr,
    })
}
