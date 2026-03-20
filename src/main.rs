use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use reqwest::blocking::Client;
use sandbox_quant::app::bootstrap::AppBootstrap;
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::app::commands::AppCommand;
use sandbox_quant::app::cli::normalize_instrument_symbol;
use sandbox_quant::app::cli::parse_app_command;
use sandbox_quant::app::output::render_command_output;
use sandbox_quant::app::runtime::AppRuntime;
use sandbox_quant::app::shell::run_shell;
use sandbox_quant::dataset::query::metrics_for_path;
use sandbox_quant::domain::exposure::Exposure;
use sandbox_quant::domain::instrument::Instrument;
use sandbox_quant::domain::order_type::OrderType;
use sandbox_quant::exchange::binance::client::BinanceExchange;
use sandbox_quant::execution::command::{CommandSource, ExecutionCommand};
use sandbox_quant::observability::logging::init_logging;
use sandbox_quant::portfolio::store::PortfolioStateStore;
use sandbox_quant::record::coordination::RecorderCoordination;
use sandbox_quant::ui::operator_terminal::prompt_status_from_store;
use serde::Serialize;
use serde_json::json;
use tracing::{error, info};

const DEFAULT_TRADING_ENGINE_SERVER_ADDR: &str = "127.0.0.1:9782";

type TradingBootstrap = AppBootstrap<BinanceExchange>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut app = AppBootstrap::from_env(PortfolioStateStore::default())?;
    let init_mode = app.mode;
    init_logging("trading-engine", Some(app.mode.as_str()))?;
    info!(service = "trading-engine", mode = app.mode.as_str(), args = ?args, "process started");
    let mut runtime = AppRuntime::default();

    let result: Result<(), Box<dyn std::error::Error>> = match args.first().map(String::as_str) {
        None => run_interactive(&mut app, &mut runtime, "var"),
        Some("run") => {
            let (mode, base_dir) = parse_runtime_args(&args[1..])?;
            configure_runtime(&mut app, mode, &base_dir)?;
            run_interactive(&mut app, &mut runtime, &base_dir)
        }
        Some("serve") => serve_trading_engine(app, &args[1..]),
        Some("status") => {
            print!(
                "{}",
                request_trading_engine_server("GET", "/status", parse_server_addr(&args[1..])?)?
            );
            Ok(())
        }
        Some("health") => {
            print!(
                "{}",
                request_trading_engine_server("GET", "/health", parse_server_addr(&args[1..])?)?
            );
            Ok(())
        }
        Some("stop") => {
            print!(
                "{}",
                request_trading_engine_server("POST", "/stop", parse_server_addr(&args[1..])?)?
            );
            Ok(())
        }
        Some(_) => {
            let command = parse_app_command(&args)?;
            let rendered_command = command.clone();
            runtime.run(&mut app, command)?;
            println!(
                "{}",
                render_command_output(
                    &rendered_command,
                    &app.portfolio_store,
                    &app.price_store,
                    &app.event_log,
                    &app.strategy_store,
                    app.mode,
                )
            );
            Ok(())
        }
    };

    if let Err(error) = result {
        error!(service = "trading-engine", mode = init_mode.as_str(), error = %error, "process failed");
        return Err(error);
    }
    info!(
        service = "trading-engine",
        mode = init_mode.as_str(),
        "process completed"
    );
    Ok(())
}

struct TradingEngineDaemon {
    app: TradingBootstrap,
    runtime: AppRuntime,
    base_dir: String,
    last_heartbeat_log: Instant,
}

#[derive(Clone)]
struct TradingEngineServerState {
    daemon: Arc<Mutex<TradingEngineDaemon>>,
    shutdown: Arc<AtomicBool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TradingEngineServeConfig {
    mode: BinanceMode,
    base_dir: String,
    listen_addr: String,
}

#[derive(Debug, Serialize)]
struct TradingEngineHealthResponse {
    mode: String,
    state: String,
    heartbeat_age_sec: i64,
    portfolio_status: String,
    positions: usize,
    open_order_groups: usize,
    last_event_kind: Option<String>,
}

#[derive(Debug, Serialize)]
struct TradingEngineCommandResponse {
    status: String,
    rendered: String,
    mode: String,
    last_event_kind: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct CloseSymbolRequest {
    instrument: String,
}

#[derive(Debug, serde::Deserialize)]
struct SetTargetExposureRequest {
    instrument: String,
    target: f64,
    order_type: Option<String>,
    limit_price: Option<f64>,
}

fn serve_trading_engine(
    mut app: TradingBootstrap,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_serve_args(args)?;
    configure_runtime(&mut app, config.mode, &config.base_dir)?;
    let daemon = Arc::new(Mutex::new(TradingEngineDaemon {
        app,
        runtime: AppRuntime::default(),
        base_dir: config.base_dir.clone(),
        last_heartbeat_log: Instant::now()
            .checked_sub(Duration::from_secs(5))
            .unwrap_or_else(Instant::now),
    }));
    let shutdown = Arc::new(AtomicBool::new(false));
    let state = TradingEngineServerState {
        daemon: daemon.clone(),
        shutdown: shutdown.clone(),
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let app = Router::new()
            .route("/status", get(trading_engine_status_handler))
            .route("/health", get(trading_engine_health_handler))
            .route("/refresh", post(trading_engine_refresh_handler))
            .route("/close-all", post(trading_engine_close_all_handler))
            .route("/close-symbol", post(trading_engine_close_symbol_handler))
            .route(
                "/set-target-exposure",
                post(trading_engine_set_target_exposure_handler),
            )
            .route("/stop", post(trading_engine_stop_handler))
            .with_state(state.clone());

        let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
        info!(
            service = "trading-engine",
            mode = config.mode.as_str(),
            listen_addr = %config.listen_addr,
            "trading-engine control server listening"
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
                if daemon.last_heartbeat_log.elapsed() >= Duration::from_secs(5) {
                    let db_path =
                        RecorderCoordination::new(daemon.base_dir.clone()).db_path(daemon.app.mode);
                    let metrics = metrics_for_path(&db_path).ok();
                    let last_event_kind = daemon
                        .app
                        .event_log
                        .records
                        .last()
                        .map(|record| record.kind.clone())
                        .unwrap_or_else(|| "none".to_string());
                    info!(
                        service = "trading-engine",
                        kind = "heartbeat",
                        mode = daemon.app.mode.as_str(),
                        ping_at = %Utc::now().to_rfc3339(),
                        pong = "alive",
                        heartbeat_age_sec = 0,
                        portfolio_status = %prompt_status_from_store(&daemon.app.portfolio_store),
                        positions = daemon.app.portfolio_store.snapshot.positions.len(),
                        open_order_groups = daemon.app.portfolio_store.snapshot.open_orders.len(),
                        last_event_kind = last_event_kind,
                        duckdb_path = %db_path.display(),
                        duckdb_exists = db_path.exists(),
                        schema_version = metrics.as_ref().and_then(|value| value.schema_version.clone()).unwrap_or_else(|| "unknown".to_string()),
                        agg_trade_events = metrics.as_ref().map(|value| value.agg_trade_events).unwrap_or(0),
                        last_agg_trade_event_time = metrics.as_ref().and_then(|value| value.last_agg_trade_event_time.clone()).unwrap_or_else(|| "n/a".to_string()),
                        "heartbeat ping/pong"
                    );
                    daemon.last_heartbeat_log = Instant::now();
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
        info!(service = "trading-engine", "serve loop completed");
        result.map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
    })
}

async fn trading_engine_status_handler(
    State(state): State<TradingEngineServerState>,
) -> Result<String, axum::http::StatusCode> {
    let daemon = state
        .daemon
        .lock()
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(render_trading_engine_status(&daemon))
}

async fn trading_engine_health_handler(
    State(state): State<TradingEngineServerState>,
) -> Result<Json<TradingEngineHealthResponse>, axum::http::StatusCode> {
    let daemon = state
        .daemon
        .lock()
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(TradingEngineHealthResponse {
        mode: daemon.app.mode.as_str().to_string(),
        state: "running".to_string(),
        heartbeat_age_sec: 0,
        portfolio_status: prompt_status_from_store(&daemon.app.portfolio_store),
        positions: daemon
            .app
            .portfolio_store
            .snapshot
            .positions
            .values()
            .filter(|position| !position.is_flat())
            .count(),
        open_order_groups: daemon.app.portfolio_store.snapshot.open_orders.len(),
        last_event_kind: daemon
            .app
            .event_log
            .records
            .last()
            .map(|record| record.kind.clone()),
    }))
}

async fn trading_engine_stop_handler(
    State(state): State<TradingEngineServerState>,
) -> impl IntoResponse {
    state.shutdown.store(true, Ordering::Relaxed);
    Json(json!({ "status": "stopping" }))
}

async fn trading_engine_refresh_handler(
    State(state): State<TradingEngineServerState>,
) -> Result<Json<TradingEngineCommandResponse>, (axum::http::StatusCode, Json<serde_json::Value>)>
{
    execute_trading_engine_command(state, AppCommand::RefreshAuthoritativeState).await
}

async fn trading_engine_close_all_handler(
    State(state): State<TradingEngineServerState>,
) -> Result<Json<TradingEngineCommandResponse>, (axum::http::StatusCode, Json<serde_json::Value>)>
{
    execute_trading_engine_command(
        state,
        AppCommand::Execution(ExecutionCommand::CloseAll {
            source: CommandSource::User,
        }),
    )
    .await
}

async fn trading_engine_close_symbol_handler(
    State(state): State<TradingEngineServerState>,
    Json(request): Json<CloseSymbolRequest>,
) -> Result<Json<TradingEngineCommandResponse>, (axum::http::StatusCode, Json<serde_json::Value>)>
{
    execute_trading_engine_command(
        state,
        AppCommand::Execution(ExecutionCommand::CloseSymbol {
            instrument: Instrument::new(normalize_instrument_symbol(&request.instrument)),
            source: CommandSource::User,
        }),
    )
    .await
}

async fn trading_engine_set_target_exposure_handler(
    State(state): State<TradingEngineServerState>,
    Json(request): Json<SetTargetExposureRequest>,
) -> Result<Json<TradingEngineCommandResponse>, (axum::http::StatusCode, Json<serde_json::Value>)>
{
    let target = Exposure::new(request.target).ok_or_else(|| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": "target must be between -1.0 and 1.0" })),
        )
    })?;
    let order_type = parse_order_type(request.order_type.as_deref(), request.limit_price).ok_or_else(
        || {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "order_type must be market or limit; limit orders require limit_price"
                })),
            )
        },
    )?;
    execute_trading_engine_command(
        state,
        AppCommand::Execution(ExecutionCommand::SetTargetExposure {
            instrument: Instrument::new(normalize_instrument_symbol(&request.instrument)),
            target,
            order_type,
            source: CommandSource::User,
        }),
    )
    .await
}

async fn execute_trading_engine_command(
    state: TradingEngineServerState,
    command: AppCommand,
) -> Result<Json<TradingEngineCommandResponse>, (axum::http::StatusCode, Json<serde_json::Value>)>
{
    tokio::task::spawn_blocking(move || {
        let mut daemon = state.daemon.lock().map_err(|_| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to lock trading-engine daemon state" })),
            )
        })?;
        let rendered_command = command.clone();
        let daemon_ref = &mut *daemon;
        daemon_ref
            .runtime
            .run(&mut daemon_ref.app, command)
            .map_err(|error| {
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": error.to_string() })),
                )
            })?;
        let rendered = render_command_output(
            &rendered_command,
            &daemon_ref.app.portfolio_store,
            &daemon_ref.app.price_store,
            &daemon_ref.app.event_log,
            &daemon_ref.app.strategy_store,
            daemon_ref.app.mode,
        );
        Ok(Json(TradingEngineCommandResponse {
            status: "ok".to_string(),
            rendered,
            mode: daemon_ref.app.mode.as_str().to_string(),
            last_event_kind: daemon_ref
                .app
                .event_log
                .records
                .last()
                .map(|record| record.kind.clone()),
        }))
    })
    .await
    .map_err(|error| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string() })),
        )
    })?
}

fn parse_order_type(order_type: Option<&str>, limit_price: Option<f64>) -> Option<OrderType> {
    match order_type.unwrap_or("market") {
        "market" => Some(OrderType::Market),
        "limit" => limit_price.map(|price| OrderType::Limit { price }),
        _ => None,
    }
}

fn render_trading_engine_status(daemon: &TradingEngineDaemon) -> String {
    let positions = daemon
        .app
        .portfolio_store
        .snapshot
        .positions
        .values()
        .filter(|position| !position.is_flat())
        .count();
    let open_order_groups = daemon.app.portfolio_store.snapshot.open_orders.len();
    let last_event_kind = daemon
        .app
        .event_log
        .records
        .last()
        .map(|record| record.kind.clone())
        .unwrap_or_else(|| "none".to_string());

    [
        "trading-engine status".to_string(),
        format!("mode={}", daemon.app.mode.as_str()),
        "state=running".to_string(),
        "heartbeat_age_sec=0".to_string(),
        format!(
            "portfolio_status={}",
            prompt_status_from_store(&daemon.app.portfolio_store)
        ),
        format!("positions={positions}"),
        format!("open_order_groups={open_order_groups}"),
        format!(
            "strategy_watches={}",
            daemon.app.strategy_store.active_watches(daemon.app.mode).len()
        ),
        format!("event_count={}", daemon.app.event_log.records.len()),
        format!("last_event_kind={last_event_kind}"),
        format!("base_dir={}", daemon.base_dir),
        format!("binary_version={}", env!("CARGO_PKG_VERSION")),
    ]
    .join("\n")
}

fn configure_runtime(
    app: &mut TradingBootstrap,
    mode: BinanceMode,
    base_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if app.mode != mode {
        app.switch_mode(mode)?;
    }
    app.recorder_coordination = RecorderCoordination::new(base_dir.to_string());
    Ok(())
}

fn run_interactive(
    app: &mut TradingBootstrap,
    runtime: &mut AppRuntime,
    base_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let heartbeat_stop = Arc::new(AtomicBool::new(false));
    let heartbeat_handle = spawn_trading_engine_heartbeat(
        app.mode,
        RecorderCoordination::new(base_dir).db_path(app.mode),
        heartbeat_stop.clone(),
    );
    let result = run_shell(app, runtime);
    heartbeat_stop.store(true, Ordering::Relaxed);
    if let Err(error) = heartbeat_handle.join() {
        error!(service = "trading-engine", error = ?error, "heartbeat thread join failed");
    }
    result
}

fn parse_runtime_args(args: &[String]) -> Result<(BinanceMode, String), Box<dyn std::error::Error>> {
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
            other => return Err(format!("unsupported arg for run: {other}").into()),
        }
    }
    Ok((mode, base_dir))
}

fn parse_serve_args(args: &[String]) -> Result<TradingEngineServeConfig, Box<dyn std::error::Error>>
{
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut listen_addr = DEFAULT_TRADING_ENGINE_SERVER_ADDR.to_string();
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
            "--listen" => {
                listen_addr = args
                    .get(index + 1)
                    .ok_or("missing value for --listen")?
                    .clone();
                index += 2;
            }
            other => return Err(format!("unsupported arg for serve: {other}").into()),
        }
    }
    Ok(TradingEngineServeConfig {
        mode,
        base_dir,
        listen_addr,
    })
}

fn parse_server_addr(args: &[String]) -> Result<String, Box<dyn std::error::Error>> {
    let mut listen_addr = DEFAULT_TRADING_ENGINE_SERVER_ADDR.to_string();
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

fn request_trading_engine_server(
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

fn spawn_trading_engine_heartbeat(
    mode: BinanceMode,
    db_path: std::path::PathBuf,
    stop_flag: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    thread::spawn(move || {
        while !stop_flag.load(Ordering::Relaxed) {
            let metrics = metrics_for_path(&db_path).ok();
            info!(
                service = "trading-engine",
                kind = "heartbeat",
                mode = mode.as_str(),
                ping_at = %Utc::now().to_rfc3339(),
                pong = "alive",
                heartbeat_age_sec = 0,
                duckdb_path = %db_path.display(),
                duckdb_exists = db_path.exists(),
                schema_version = metrics.as_ref().and_then(|value| value.schema_version.clone()).unwrap_or_else(|| "unknown".to_string()),
                agg_trade_events = metrics.as_ref().map(|value| value.agg_trade_events).unwrap_or(0),
                last_agg_trade_event_time = metrics.as_ref().and_then(|value| value.last_agg_trade_event_time.clone()).unwrap_or_else(|| "n/a".to_string()),
                "heartbeat ping/pong"
            );
            thread::sleep(Duration::from_secs(5));
        }
    })
}
