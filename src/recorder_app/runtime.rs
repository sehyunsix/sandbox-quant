use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, RecvTimeoutError, Sender},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use duckdb::{params, Connection};
use futures_util::StreamExt;
use serde::Deserialize;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, warn};

use crate::app::bootstrap::BinanceMode;
use crate::dataset::query::{backtest_summary_for_path, metrics_for_path};
use crate::dataset::schema::init_schema_for_path;
use crate::dataset::types::{BacktestDatasetSummary, RecorderMetrics};
use crate::error::storage_error::StorageError;
use crate::record::coordination::RecorderCoordination;
use crate::storage::postgres_market_data::{
    connect as connect_postgres, ensure_recorder_schema_ready, insert_agg_trade, insert_book_ticker,
    insert_liquidation, mask_postgres_url, metrics_for_postgres_url, postgres_url_from_env,
    CollectorStorageBackend, PostgresAggTradeRecord, PostgresBookTickerRecord,
    PostgresLiquidationRecord,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderState {
    Running,
    Stopped,
}

impl RecorderState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }

    pub fn is_running(self) -> bool {
        self == Self::Running
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecorderStatus {
    pub mode: BinanceMode,
    pub state: RecorderState,
    pub db_path: PathBuf,
    pub storage_backend: String,
    pub storage_target: String,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub manual_symbols: Vec<String>,
    pub strategy_symbols: Vec<String>,
    pub watched_symbols: Vec<String>,
    pub reader_alive: bool,
    pub writer_alive: bool,
    pub worker_alive: bool,
    pub heartbeat_age_sec: i64,
    pub last_error: Option<String>,
    pub metrics: RecorderMetrics,
}

impl RecorderStatus {
    fn new(
        mode: BinanceMode,
        state: RecorderState,
        db_path: PathBuf,
        storage_backend: String,
        storage_target: String,
        manual_symbols: Vec<String>,
        strategy_symbols: Vec<String>,
        watched_symbols: Vec<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            mode,
            state,
            db_path,
            storage_backend,
            storage_target,
            started_at: if state == RecorderState::Running {
                Some(now)
            } else {
                None
            },
            updated_at: now,
            manual_symbols,
            strategy_symbols,
            watched_symbols,
            reader_alive: true,
            writer_alive: true,
            worker_alive: true,
            heartbeat_age_sec: 0,
            last_error: None,
            metrics: RecorderMetrics::default(),
        }
    }
}

#[derive(Debug, Clone)]
struct WorkerSnapshot {
    updated_at: DateTime<Utc>,
    metrics: RecorderMetrics,
    last_error: Option<String>,
}

#[derive(Debug, Clone)]
enum PostgresWriteCommand {
    Liquidation(PostgresLiquidationRecord),
    BookTicker(PostgresBookTickerRecord),
    AggTrade(PostgresAggTradeRecord),
}

impl WorkerSnapshot {
    fn new(metrics: RecorderMetrics) -> Self {
        Self {
            updated_at: Utc::now(),
            metrics,
            last_error: None,
        }
    }
}

struct ModeWorker {
    stop_flag: Arc<AtomicBool>,
    snapshot: Arc<Mutex<WorkerSnapshot>>,
    pub(crate) reader_handle: JoinHandle<()>,
    pub(crate) writer_handle: Option<JoinHandle<()>>,
}

pub struct MarketDataRecorder {
    base_dir: PathBuf,
    network_enabled: bool,
    storage_backend: CollectorStorageBackend,
    postgres_url: Option<String>,
    statuses: BTreeMap<BinanceMode, RecorderStatus>,
    workers: BTreeMap<BinanceMode, ModeWorker>,
}

impl std::fmt::Debug for MarketDataRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketDataRecorder")
            .field("base_dir", &self.base_dir)
            .field("network_enabled", &self.network_enabled)
            .field("storage_backend", &self.storage_backend)
            .field("statuses", &self.statuses)
            .finish()
    }
}

impl Default for MarketDataRecorder {
    fn default() -> Self {
        Self::new("var")
    }
}

impl MarketDataRecorder {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        let storage_backend = std::env::var("SANDBOX_QUANT_RECORDER_STORAGE")
            .ok()
            .as_deref()
            .map(parse_storage_backend)
            .unwrap_or(CollectorStorageBackend::DuckDb);
        let postgres_url = if storage_backend == CollectorStorageBackend::Postgres {
            postgres_url_from_env().ok()
        } else {
            None
        };
        Self {
            base_dir: base_dir.into(),
            network_enabled: true,
            storage_backend,
            postgres_url,
            statuses: BTreeMap::new(),
            workers: BTreeMap::new(),
        }
    }

    pub fn without_network(mut self) -> Self {
        self.network_enabled = false;
        self
    }

    pub fn start(
        &mut self,
        mode: BinanceMode,
        manual_symbols: Vec<String>,
        strategy_symbols: Vec<String>,
    ) -> Result<RecorderStatus, StorageError> {
        if self
            .statuses
            .get(&mode)
            .is_some_and(|status| status.state == RecorderState::Running)
        {
            return Err(StorageError::RecorderAlreadyRunning {
                mode: mode.as_str().to_string(),
            });
        }

        let db_path = self.db_path(mode);
        if self.storage_backend == CollectorStorageBackend::DuckDb {
            init_schema_for_path(&db_path)?;
        } else if let Some(url) = self.postgres_url.as_deref() {
            let mut client = connect_postgres(url)?;
            ensure_recorder_schema_ready(&mut client, url)?;
        }
        let manual_symbols = normalize_symbols(manual_symbols);
        let strategy_symbols = normalize_symbols(strategy_symbols);
        let watched_symbols = merge_symbol_sets(manual_symbols.clone(), strategy_symbols.clone());
        let status = RecorderStatus::new(
            mode,
            RecorderState::Running,
            db_path.clone(),
            self.storage_backend.as_str().to_string(),
            self.storage_target(mode),
            manual_symbols,
            strategy_symbols,
            watched_symbols.clone(),
        );
        let initial_metrics = self.load_metrics(&db_path).unwrap_or_default();
        let mut status = status;
        status.metrics = initial_metrics.clone();
        if self.network_enabled {
            self.spawn_worker(mode, db_path, watched_symbols, initial_metrics)?;
        }
        self.statuses.insert(mode, status.clone());
        Ok(status)
    }

    pub fn status(&self, mode: BinanceMode) -> RecorderStatus {
        let mut status = self.statuses.get(&mode).cloned().unwrap_or_else(|| {
            RecorderStatus::new(
                mode,
                RecorderState::Stopped,
                self.db_path(mode),
                self.storage_backend.as_str().to_string(),
                self.storage_target(mode),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        });
        if let Some(worker) = self.workers.get(&mode) {
            status.reader_alive = !worker.reader_handle.is_finished();
            status.writer_alive = worker
                .writer_handle
                .as_ref()
                .is_none_or(|handle| !handle.is_finished());
            status.worker_alive = status.reader_alive && status.writer_alive;
            if let Ok(snapshot) = worker.snapshot.lock() {
                status.updated_at = snapshot.updated_at;
                status.heartbeat_age_sec = (Utc::now() - snapshot.updated_at).num_seconds();
                status.metrics = snapshot.metrics.clone();
                status.last_error = snapshot.last_error.clone();
            }
        } else {
            status.reader_alive = false;
            status.writer_alive = false;
            status.worker_alive = false;
            status.heartbeat_age_sec = (Utc::now() - status.updated_at).num_seconds();
            status.metrics = self.load_metrics(&status.db_path).unwrap_or_default();
        }
        status
    }

    pub fn update_strategy_symbols(
        &mut self,
        mode: BinanceMode,
        strategy_symbols: Vec<String>,
    ) -> Result<(), StorageError> {
        let strategy_symbols = normalize_symbols(strategy_symbols);
        let Some(status) = self.statuses.get_mut(&mode) else {
            return Ok(());
        };
        if status.strategy_symbols == strategy_symbols {
            return Ok(());
        }
        status.strategy_symbols = strategy_symbols.clone();
        status.watched_symbols =
            merge_symbol_sets(status.manual_symbols.clone(), strategy_symbols.clone());
        status.updated_at = Utc::now();
        let watched_symbols = status.watched_symbols.clone();
        let should_restart = status.state == RecorderState::Running && self.network_enabled;

        if !should_restart {
            return Ok(());
        }

        self.restart_worker(mode, watched_symbols)
    }

    pub fn update_manual_symbols(
        &mut self,
        mode: BinanceMode,
        manual_symbols: Vec<String>,
    ) -> Result<(), StorageError> {
        let manual_symbols = normalize_symbols(manual_symbols);
        let Some(status) = self.statuses.get_mut(&mode) else {
            return Ok(());
        };
        if status.manual_symbols == manual_symbols {
            return Ok(());
        }
        status.manual_symbols = manual_symbols.clone();
        status.watched_symbols =
            merge_symbol_sets(manual_symbols.clone(), status.strategy_symbols.clone());
        status.updated_at = Utc::now();
        let watched_symbols = status.watched_symbols.clone();
        let should_restart = status.state == RecorderState::Running && self.network_enabled;

        if !should_restart {
            return Ok(());
        }

        self.restart_worker(mode, watched_symbols)
    }

    pub fn stop(&mut self, mode: BinanceMode) -> Result<RecorderStatus, StorageError> {
        let Some(existing) = self.statuses.get_mut(&mode) else {
            return Err(StorageError::RecorderNotRunning {
                mode: mode.as_str().to_string(),
            });
        };
        if existing.state != RecorderState::Running {
            return Err(StorageError::RecorderNotRunning {
                mode: mode.as_str().to_string(),
            });
        }

        if let Some(worker) = self.workers.remove(&mode) {
            if let Ok(snapshot) = worker.snapshot.lock() {
                existing.updated_at = snapshot.updated_at;
                existing.metrics = snapshot.metrics.clone();
                existing.last_error = snapshot.last_error.clone();
            }
            worker.stop_flag.store(true, Ordering::Relaxed);
        }

        existing.state = RecorderState::Stopped;
        existing.updated_at = Utc::now();
        existing.reader_alive = false;
        existing.writer_alive = false;
        existing.worker_alive = false;
        Ok(existing.clone())
    }

    pub fn backtest_dataset_summary(
        &self,
        mode: BinanceMode,
        symbol: &str,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
    ) -> Result<BacktestDatasetSummary, StorageError> {
        backtest_summary_for_path(&self.db_path(mode), mode, symbol, from, to)
    }

    pub fn worker_alive(&self, mode: BinanceMode) -> bool {
        self.workers
            .get(&mode)
            .is_some_and(|worker| !worker.reader_handle.is_finished())
    }

    pub fn metrics_for_path(db_path: &Path) -> Result<RecorderMetrics, StorageError> {
        metrics_for_path(db_path)
    }

    pub fn backtest_summary_for_path(
        db_path: &Path,
        mode: BinanceMode,
        symbol: &str,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
    ) -> Result<BacktestDatasetSummary, StorageError> {
        backtest_summary_for_path(db_path, mode, symbol, from, to)
    }

    pub fn init_schema_for_path(db_path: &Path) -> Result<(), StorageError> {
        init_schema_for_path(db_path)
    }

    fn restart_worker(
        &mut self,
        mode: BinanceMode,
        watched_symbols: Vec<String>,
    ) -> Result<(), StorageError> {
        let initial_metrics = self.status(mode).metrics.clone();
        if let Some(worker) = self.workers.remove(&mode) {
            worker.stop_flag.store(true, Ordering::Relaxed);
        }
        let db_path = self.db_path(mode);
        self.spawn_worker(mode, db_path, watched_symbols, initial_metrics)
    }

    fn spawn_worker(
        &mut self,
        mode: BinanceMode,
        db_path: PathBuf,
        watched_symbols: Vec<String>,
        initial_metrics: RecorderMetrics,
    ) -> Result<(), StorageError> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let worker_stop_flag = stop_flag.clone();
        let snapshot = Arc::new(Mutex::new(WorkerSnapshot::new(initial_metrics)));
        let worker_snapshot = snapshot.clone();
        let storage_backend = self.storage_backend;
        let (postgres_writer, writer_handle) = initialize_postgres_writer(
            storage_backend,
            self.postgres_url.as_deref(),
            worker_stop_flag.clone(),
            worker_snapshot.clone(),
        )?;
        let reader_handle = std::thread::Builder::new()
            .name(format!("market-recorder-{}", mode.as_str()))
            .spawn(move || {
                let _ = rustls::crypto::ring::default_provider().install_default();
                let duck_connection = match initialize_duckdb_storage(&db_path, storage_backend) {
                    Ok(storage) => storage,
                    Err(error) => {
                        record_worker_error(&worker_snapshot, error.to_string());
                        return;
                    }
                };
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build();
                let Ok(runtime) = runtime else {
                    record_worker_error(
                        &worker_snapshot,
                        "failed to initialize tokio runtime".to_string(),
                    );
                    return;
                };
                runtime.block_on(async {
                    run_market_data_worker(
                        mode,
                        duck_connection.as_ref(),
                        postgres_writer.as_ref(),
                        watched_symbols,
                        worker_stop_flag,
                        worker_snapshot,
                    )
                    .await;
                });
            })
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
        self.workers.insert(
            mode,
            ModeWorker {
                stop_flag,
                snapshot,
                reader_handle,
                writer_handle,
            },
        );
        Ok(())
    }

    fn load_metrics(&self, db_path: &Path) -> Result<RecorderMetrics, StorageError> {
        match self.storage_backend {
            CollectorStorageBackend::DuckDb => metrics_for_path(db_path),
            CollectorStorageBackend::Postgres => self
                .postgres_url
                .as_deref()
                .ok_or_else(|| StorageError::WriteFailedWithContext {
                    message: "postgres recorder backend selected but postgres URL is missing"
                        .to_string(),
                })
                .and_then(metrics_for_postgres_url),
        }
    }

    fn db_path(&self, mode: BinanceMode) -> PathBuf {
        RecorderCoordination::new(self.base_dir.clone()).db_path(mode)
    }

    fn storage_target(&self, mode: BinanceMode) -> String {
        match self.storage_backend {
            CollectorStorageBackend::DuckDb => self.db_path(mode).display().to_string(),
            CollectorStorageBackend::Postgres => self
                .postgres_url
                .as_deref()
                .map(mask_postgres_url)
                .unwrap_or_else(|| "postgres://***".to_string()),
        }
    }
}

impl Drop for MarketDataRecorder {
    fn drop(&mut self) {
        for worker in self.workers.values() {
            worker.stop_flag.store(true, Ordering::Relaxed);
        }
    }
}

fn initialize_duckdb_storage(
    db_path: &Path,
    storage_backend: CollectorStorageBackend,
) -> Result<Option<Connection>, StorageError> {
    if storage_backend != CollectorStorageBackend::DuckDb {
        return Ok(None);
    }
    Connection::open(db_path)
        .map(Some)
        .map_err(|_| StorageError::WriteFailedWithContext {
            message: format!("failed to open duckdb at {}", db_path.display()),
        })
}

fn initialize_postgres_writer(
    storage_backend: CollectorStorageBackend,
    postgres_url: Option<&str>,
    stop_flag: Arc<AtomicBool>,
    snapshot: Arc<Mutex<WorkerSnapshot>>,
) -> Result<(Option<Sender<PostgresWriteCommand>>, Option<JoinHandle<()>>), StorageError> {
    if storage_backend != CollectorStorageBackend::Postgres {
        return Ok((None, None));
    }
    let url = postgres_url.ok_or_else(|| StorageError::WriteFailedWithContext {
        message: "postgres recorder backend missing URL".to_string(),
    })?;
    let mut client = connect_postgres(url)?;
    ensure_recorder_schema_ready(&mut client, url)?;
    let (sender, receiver) = mpsc::channel::<PostgresWriteCommand>();
    let handle = std::thread::Builder::new()
        .name("market-recorder-postgres-writer".to_string())
        .spawn(move || run_postgres_writer_loop(client, receiver, stop_flag, snapshot))
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    Ok((Some(sender), Some(handle)))
}

fn run_postgres_writer_loop(
    mut client: postgres::Client,
    receiver: mpsc::Receiver<PostgresWriteCommand>,
    stop_flag: Arc<AtomicBool>,
    snapshot: Arc<Mutex<WorkerSnapshot>>,
) {
    loop {
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(PostgresWriteCommand::Liquidation(record)) => {
                if let Err(error) = insert_liquidation(&mut client, &record) {
                    record_worker_error(&snapshot, error.to_string());
                    break;
                }
            }
            Ok(PostgresWriteCommand::BookTicker(record)) => {
                if let Err(error) = insert_book_ticker(&mut client, &record) {
                    record_worker_error(&snapshot, error.to_string());
                    break;
                }
            }
            Ok(PostgresWriteCommand::AggTrade(record)) => {
                if let Err(error) = insert_agg_trade(&mut client, &record) {
                    record_worker_error(&snapshot, error.to_string());
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) if stop_flag.load(Ordering::Relaxed) => break,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

async fn run_market_data_worker(
    mode: BinanceMode,
    duck_connection: Option<&Connection>,
    postgres_writer: Option<&Sender<PostgresWriteCommand>>,
    watched_symbols: Vec<String>,
    stop_flag: Arc<AtomicBool>,
    snapshot: Arc<Mutex<WorkerSnapshot>>,
) {
    let mut agg_trade_bar_seconds = BTreeMap::new();

    loop {
        touch_worker_snapshot(&snapshot);
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        let force_order_url = format!("{}/ws/!forceOrder@arr", market_stream_base_url(mode));
        let symbol_stream_url = combined_symbol_stream_url(mode, &watched_symbols);

        let force_stream = connect_async(force_order_url).await;
        let mut force_stream = match force_stream {
            Ok((stream, _)) => stream,
            Err(error) => {
                record_worker_error(&snapshot, format!("forceOrder connect failed: {error}"));
                warn!(service = "recorder", error = %error, "failed to connect forceOrder stream");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        let mut symbol_stream = match symbol_stream_url {
            Some(url) => match connect_async(url).await {
                Ok((stream, _)) => Some(stream),
                Err(error) => {
                    record_worker_error(
                        &snapshot,
                        format!("symbol stream connect failed: {error}"),
                    );
                    warn!(service = "recorder", symbols = %watched_symbols.join(","), error = %error, "failed to connect symbol streams");
                    None
                }
            },
            None => None,
        };

        let mut liquidation_seq = 0i64;
        let mut ticker_seq = 0i64;
        let mut trade_seq = 0i64;

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                return;
            }

            tokio::select! {
                message = force_stream.next() => {
                    match message {
                        Some(Ok(message)) => {
                            if let Err(error) = handle_force_order_message(
                                duck_connection,
                                postgres_writer,
                                mode,
                                &mut liquidation_seq,
                                &snapshot,
                                message
                            ) {
                                record_worker_error(&snapshot, error.to_string());
                                error!(service = "recorder", error = %error, "forceOrder stream handling failed");
                                break;
                            }
                        }
                        Some(Err(error)) => {
                            record_worker_error(&snapshot, format!("forceOrder stream disconnected: {error}"));
                            warn!(service = "recorder", error = %error, "forceOrder stream disconnected");
                            break
                        }
                        None => {
                            record_worker_error(&snapshot, "forceOrder stream disconnected: eof".to_string());
                            warn!(service = "recorder", "forceOrder stream disconnected: eof");
                            break
                        }
                    }
                }
                message = next_symbol_message(&mut symbol_stream), if symbol_stream.is_some() => {
                    match message {
                        Some(Ok(message)) => {
                            if let Err(error) = handle_symbol_message(
                                duck_connection,
                                postgres_writer,
                                mode,
                                &mut ticker_seq,
                                &mut trade_seq,
                                &mut agg_trade_bar_seconds,
                                &snapshot,
                                message,
                            ) {
                                record_worker_error(&snapshot, error.to_string());
                                error!(service = "recorder", error = %error, "symbol stream handling failed");
                                break;
                            }
                        }
                        Some(Err(error)) => {
                            record_worker_error(&snapshot, format!("symbol stream disconnected: {error}"));
                            warn!(service = "recorder", symbols = %watched_symbols.join(","), error = %error, "symbol stream disconnected");
                            break
                        }
                        None => {
                            record_worker_error(&snapshot, "symbol stream disconnected: eof".to_string());
                            warn!(service = "recorder", symbols = %watched_symbols.join(","), "symbol stream disconnected: eof");
                            break
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(250)) => {
                    touch_worker_snapshot(&snapshot);
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn next_symbol_message(
    stream: &mut Option<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) -> Option<Result<Message, tokio_tungstenite::tungstenite::Error>> {
    match stream {
        Some(stream) => stream.next().await,
        None => None,
    }
}

fn handle_force_order_message(
    duck_connection: Option<&Connection>,
    postgres_writer: Option<&Sender<PostgresWriteCommand>>,
    mode: BinanceMode,
    sequence: &mut i64,
    snapshot: &Arc<Mutex<WorkerSnapshot>>,
    message: Message,
) -> Result<(), StorageError> {
    let payload = match message {
        Message::Text(text) => text,
        Message::Ping(_) | Message::Pong(_) | Message::Binary(_) => return Ok(()),
        Message::Close(_) => {
            return Err(StorageError::WriteFailedWithContext {
                message: "forceOrder stream closed".to_string(),
            })
        }
        Message::Frame(_) => return Ok(()),
    };
    let parsed: ForceOrderEnvelope =
        serde_json::from_str(&payload).map_err(|error| StorageError::WriteFailedWithContext {
            message: format!("forceOrder parse failed: {error}; payload={payload}"),
        })?;
    let Some(order) = parsed.order else {
        return Ok(());
    };
    *sequence += 1;
    let receive_time_ms = Utc::now().timestamp_millis();
    let symbol = order.symbol.clone();
    let side = order.side.clone();
    if let Some(connection) = duck_connection {
        connection
            .execute(
                "INSERT INTO raw_liquidation_events (
                    event_id, mode, symbol, event_time, receive_time, force_side, price, qty, notional, raw_payload
                 ) VALUES (
                    ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0), ?, ?, ?, ?, ?
                 )",
                params![
                    *sequence,
                    mode.as_str(),
                    symbol,
                    parsed.event_time,
                    receive_time_ms,
                    side,
                    order.price,
                    order.qty,
                    order.price * order.qty,
                    payload,
                ],
            )
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
    } else if let Some(sender) = postgres_writer {
        sender
            .send(PostgresWriteCommand::Liquidation(
                PostgresLiquidationRecord {
                    product: "um".to_string(),
                    symbol: symbol.clone(),
                    event_time_ms: parsed.event_time,
                    receive_time_ms,
                    force_side: side,
                    price: order.price,
                    qty: order.qty,
                    notional: order.price * order.qty,
                    raw_payload: payload,
                },
            ))
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: format!("postgres liquidation writer disconnected: {error}"),
            })?;
    } else {
        return Err(StorageError::WriteFailedWithContext {
            message: "no recorder storage backend available".to_string(),
        });
    }
    record_force_order_event(snapshot, &symbol, parsed.event_time);
    Ok(())
}

fn handle_symbol_message(
    duck_connection: Option<&Connection>,
    postgres_writer: Option<&Sender<PostgresWriteCommand>>,
    mode: BinanceMode,
    ticker_sequence: &mut i64,
    trade_sequence: &mut i64,
    agg_trade_bar_seconds: &mut BTreeMap<String, i64>,
    snapshot: &Arc<Mutex<WorkerSnapshot>>,
    message: Message,
) -> Result<(), StorageError> {
    let payload = match message {
        Message::Text(text) => text,
        Message::Ping(_) | Message::Pong(_) | Message::Binary(_) => return Ok(()),
        Message::Close(_) => {
            return Err(StorageError::WriteFailedWithContext {
                message: "symbol stream closed".to_string(),
            })
        }
        Message::Frame(_) => return Ok(()),
    };
    let parsed: CombinedStreamEnvelope =
        serde_json::from_str(&payload).map_err(|error| StorageError::WriteFailedWithContext {
            message: format!("symbol stream parse failed: {error}; payload={payload}"),
        })?;
    let receive_time_ms = Utc::now().timestamp_millis();

    if parsed.data.event_type == "bookTicker" {
        let Some(symbol) = parsed.data.symbol else {
            return Ok(());
        };
        let Some(event_time) = parsed.data.event_time else {
            return Ok(());
        };
        let Some(bid) = parsed.data.bid else {
            return Ok(());
        };
        let Some(bid_qty) = parsed.data.bid_qty else {
            return Ok(());
        };
        let Some(ask) = parsed.data.ask else {
            return Ok(());
        };
        let Some(ask_qty) = parsed.data.ask_qty else {
            return Ok(());
        };
        *ticker_sequence += 1;
        if let Some(connection) = duck_connection {
            connection
                .execute(
                    "INSERT INTO raw_book_ticker (
                        tick_id, mode, symbol, event_time, receive_time, bid, bid_qty, ask, ask_qty
                     ) VALUES (
                        ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0), ?, ?, ?, ?
                     )",
                    params![
                        *ticker_sequence,
                        mode.as_str(),
                        symbol,
                        event_time,
                        receive_time_ms,
                        bid,
                        bid_qty,
                        ask,
                        ask_qty,
                    ],
                )
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: error.to_string(),
                })?;
        } else if let Some(sender) = postgres_writer {
            sender
                .send(PostgresWriteCommand::BookTicker(PostgresBookTickerRecord {
                    symbol: symbol.clone(),
                    event_time_ms: event_time,
                    receive_time_ms,
                    bid,
                    bid_qty,
                    ask,
                    ask_qty,
                }))
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: format!("postgres book_ticker writer disconnected: {error}"),
                })?;
        } else {
            return Err(StorageError::WriteFailedWithContext {
                message: "no recorder storage backend available".to_string(),
            });
        }
        record_book_ticker_event(snapshot, &symbol, event_time);
    } else if parsed.data.event_type == "aggTrade" {
        let Some(symbol) = parsed.data.symbol else {
            return Ok(());
        };
        let Some(event_time) = parsed.data.event_time else {
            return Ok(());
        };
        let Some(price) = parsed.data.price else {
            return Ok(());
        };
        let Some(qty) = parsed.data.qty else {
            return Ok(());
        };
        let Some(is_buyer_maker) = parsed.data.is_buyer_maker else {
            return Ok(());
        };
        *trade_sequence += 1;
        if let Some(connection) = duck_connection {
            connection
                .execute(
                    "INSERT INTO raw_agg_trades (
                        trade_id, mode, symbol, event_time, receive_time, price, qty, is_buyer_maker
                     ) VALUES (
                        ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0), ?, ?, ?
                     )",
                    params![
                        *trade_sequence,
                        mode.as_str(),
                        symbol,
                        event_time,
                        receive_time_ms,
                        price,
                        qty,
                        is_buyer_maker,
                    ],
                )
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: error.to_string(),
                })?;
        } else if let Some(sender) = postgres_writer {
            sender
                .send(PostgresWriteCommand::AggTrade(PostgresAggTradeRecord {
                    symbol: symbol.clone(),
                    event_time_ms: event_time,
                    receive_time_ms,
                    price,
                    qty,
                    is_buyer_maker,
                }))
                .map_err(|error| StorageError::WriteFailedWithContext {
                    message: format!("postgres agg_trade writer disconnected: {error}"),
                })?;
        } else {
            return Err(StorageError::WriteFailedWithContext {
                message: "no recorder storage backend available".to_string(),
            });
        }
        record_agg_trade_event(snapshot, &symbol, event_time, agg_trade_bar_seconds);
    }

    Ok(())
}

fn market_stream_base_url(mode: BinanceMode) -> &'static str {
    let _ = mode;
    "wss://fstream.binance.com"
}

fn combined_symbol_stream_url(mode: BinanceMode, watched_symbols: &[String]) -> Option<String> {
    if watched_symbols.is_empty() {
        return None;
    }

    let streams = watched_symbols
        .iter()
        .flat_map(|symbol| {
            let lower = symbol.to_ascii_lowercase();
            [format!("{lower}@bookTicker"), format!("{lower}@aggTrade")]
        })
        .collect::<Vec<_>>()
        .join("/");
    Some(format!(
        "{}/stream?streams={streams}",
        market_stream_base_url(mode)
    ))
}

fn normalize_symbols(symbols: Vec<String>) -> Vec<String> {
    let mut normalized = BTreeSet::new();
    for symbol in symbols {
        normalized.insert(symbol.trim().to_ascii_uppercase());
    }
    normalized.into_iter().collect()
}

fn merge_symbol_sets(left: Vec<String>, right: Vec<String>) -> Vec<String> {
    let mut merged = BTreeSet::new();
    for symbol in left.into_iter().chain(right.into_iter()) {
        merged.insert(symbol);
    }
    merged.into_iter().collect()
}

fn parse_storage_backend(value: &str) -> CollectorStorageBackend {
    match value {
        "postgres" => CollectorStorageBackend::Postgres,
        _ => CollectorStorageBackend::DuckDb,
    }
}

#[derive(Debug, Deserialize)]
struct ForceOrderEnvelope {
    #[serde(rename = "E")]
    event_time: i64,
    #[serde(rename = "o")]
    order: Option<ForceOrderData>,
}

#[derive(Debug, Deserialize)]
struct ForceOrderData {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "S")]
    side: String,
    #[serde(rename = "p", deserialize_with = "deserialize_string_number")]
    price: f64,
    #[serde(rename = "q", deserialize_with = "deserialize_string_number")]
    qty: f64,
}

#[derive(Debug, Deserialize)]
struct CombinedStreamEnvelope {
    data: CombinedStreamData,
}

#[derive(Debug, Deserialize)]
struct CombinedStreamData {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "E")]
    event_time: Option<i64>,
    #[serde(rename = "s")]
    symbol: Option<String>,
    #[serde(
        rename = "b",
        default,
        deserialize_with = "deserialize_optional_string_number"
    )]
    bid: Option<f64>,
    #[serde(
        rename = "B",
        default,
        deserialize_with = "deserialize_optional_string_number"
    )]
    bid_qty: Option<f64>,
    #[serde(
        rename = "a",
        default,
        deserialize_with = "deserialize_optional_string_number"
    )]
    ask: Option<f64>,
    #[serde(
        rename = "A",
        default,
        deserialize_with = "deserialize_optional_string_number"
    )]
    ask_qty: Option<f64>,
    #[serde(
        rename = "p",
        default,
        deserialize_with = "deserialize_optional_string_number"
    )]
    price: Option<f64>,
    #[serde(
        rename = "q",
        default,
        deserialize_with = "deserialize_optional_string_number"
    )]
    qty: Option<f64>,
    #[serde(rename = "m")]
    is_buyer_maker: Option<bool>,
}

fn deserialize_string_number<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    value.parse::<f64>().map_err(serde::de::Error::custom)
}

fn deserialize_optional_string_number<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) => value
            .parse::<f64>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        Some(serde_json::Value::Number(value)) => value
            .as_f64()
            .ok_or_else(|| serde::de::Error::custom("invalid numeric value"))
            .map(Some),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected string or number, got {other}"
        ))),
    }
}

fn touch_worker_snapshot(snapshot: &Arc<Mutex<WorkerSnapshot>>) {
    if let Ok(mut snapshot) = snapshot.lock() {
        snapshot.updated_at = Utc::now();
    }
}

fn record_worker_error(snapshot: &Arc<Mutex<WorkerSnapshot>>, error: String) {
    if let Ok(mut snapshot) = snapshot.lock() {
        snapshot.updated_at = Utc::now();
        snapshot.last_error = Some(error);
    }
}

fn record_force_order_event(
    snapshot: &Arc<Mutex<WorkerSnapshot>>,
    symbol: &str,
    event_time_ms: i64,
) {
    if let Ok(mut snapshot) = snapshot.lock() {
        snapshot.updated_at = Utc::now();
        snapshot.last_error = None;
        snapshot.metrics.liquidation_events += 1;
        snapshot.metrics.last_liquidation_event_time = timestamp_string(event_time_ms);
        increment_top_symbol(&mut snapshot.metrics.top_liquidation_symbols, symbol);
    }
}

fn record_book_ticker_event(
    snapshot: &Arc<Mutex<WorkerSnapshot>>,
    symbol: &str,
    event_time_ms: i64,
) {
    if let Ok(mut snapshot) = snapshot.lock() {
        snapshot.updated_at = Utc::now();
        snapshot.last_error = None;
        snapshot.metrics.book_ticker_events += 1;
        snapshot.metrics.last_book_ticker_event_time = timestamp_string(event_time_ms);
        increment_top_symbol(&mut snapshot.metrics.top_book_ticker_symbols, symbol);
    }
}

fn record_agg_trade_event(
    snapshot: &Arc<Mutex<WorkerSnapshot>>,
    symbol: &str,
    event_time_ms: i64,
    agg_trade_bar_seconds: &mut BTreeMap<String, i64>,
) {
    if let Ok(mut snapshot) = snapshot.lock() {
        snapshot.updated_at = Utc::now();
        snapshot.last_error = None;
        snapshot.metrics.agg_trade_events += 1;
        snapshot.metrics.last_agg_trade_event_time = timestamp_string(event_time_ms);
        increment_top_symbol(&mut snapshot.metrics.top_agg_trade_symbols, symbol);
        let bar_second = event_time_ms / 1_000;
        let should_increment_bar = agg_trade_bar_seconds
            .insert(symbol.to_string(), bar_second)
            .map(|previous| previous != bar_second)
            .unwrap_or(true);
        if should_increment_bar {
            snapshot.metrics.derived_kline_1s_bars += 1;
        }
    }
}

fn increment_top_symbol(top_symbols: &mut Vec<String>, symbol: &str) {
    let mut counts = top_symbols
        .iter()
        .filter_map(|entry| {
            let (symbol, count) = entry.split_once(':')?;
            let count = count.parse::<u64>().ok()?;
            Some((symbol.to_string(), count))
        })
        .collect::<BTreeMap<_, _>>();
    *counts.entry(symbol.to_string()).or_default() += 1;
    let mut sorted = counts.into_iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    *top_symbols = sorted
        .into_iter()
        .take(5)
        .map(|(symbol, count)| format!("{symbol}:{count}"))
        .collect();
}

fn timestamp_string(event_time_ms: i64) -> Option<String> {
    Utc.timestamp_millis_opt(event_time_ms)
        .single()
        .map(|value| {
            value
                .naive_utc()
                .format("%Y-%m-%d %H:%M:%S%.3f")
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::types::RecorderMetrics;

    #[test]
    fn handle_force_order_message_persists_liquidation_and_updates_metrics() {
        let connection = Connection::open_in_memory().expect("open duckdb");
        connection
            .execute_batch(
                "CREATE TABLE raw_liquidation_events (
                    event_id BIGINT,
                    mode TEXT,
                    symbol TEXT,
                    event_time TIMESTAMP,
                    receive_time TIMESTAMP,
                    force_side TEXT,
                    price DOUBLE,
                    qty DOUBLE,
                    notional DOUBLE,
                    raw_payload TEXT
                );",
            )
            .expect("create raw_liquidation_events");
        let snapshot = Arc::new(Mutex::new(WorkerSnapshot::new(RecorderMetrics::default())));
        let payload = serde_json::json!({
            "E": 1_710_000_000_123_i64,
            "o": {
                "s": "BTCUSDT",
                "S": "SELL",
                "p": "68250.5",
                "q": "0.125"
            }
        })
        .to_string();
        let mut sequence = 0i64;

        handle_force_order_message(
            Some(&connection),
            None,
            BinanceMode::Demo,
            &mut sequence,
            &snapshot,
            Message::Text(payload.clone().into()),
        )
        .expect("handle force order");

        let mut statement = connection
            .prepare(
                "SELECT event_id, mode, symbol, force_side, price, qty, notional, raw_payload
                 FROM raw_liquidation_events",
            )
            .expect("prepare query");
        let row = statement
            .query_row([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, f64>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .expect("fetch inserted liquidation");
        assert_eq!(row.0, 1);
        assert_eq!(row.1, "demo");
        assert_eq!(row.2, "BTCUSDT");
        assert_eq!(row.3, "SELL");
        assert_eq!(row.4, 68_250.5);
        assert_eq!(row.5, 0.125);
        assert_eq!(row.6, 8_531.3125);
        assert_eq!(row.7, payload);
        assert_eq!(sequence, 1);

        let snapshot = snapshot.lock().expect("snapshot lock");
        assert_eq!(snapshot.metrics.liquidation_events, 1);
        assert_eq!(
            snapshot.metrics.last_liquidation_event_time.as_deref(),
            Some("2024-03-09 16:00:00.123")
        );
        assert_eq!(
            snapshot.metrics.top_liquidation_symbols,
            vec!["BTCUSDT:1".to_string()]
        );
        assert!(snapshot.last_error.is_none());
    }
}
