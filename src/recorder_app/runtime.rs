use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;
use std::time::Duration;

use chrono::{DateTime, Utc};
use duckdb::{params, Connection};
use futures_util::StreamExt;
use serde::Deserialize;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::app::bootstrap::BinanceMode;
use crate::dataset::query::{backtest_summary_for_path, metrics_for_path};
use crate::dataset::schema::init_schema_for_path;
use crate::dataset::types::{BacktestDatasetSummary, RecorderMetrics};
use crate::error::storage_error::StorageError;
use crate::record::manager::format_mode;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecorderStatus {
    pub mode: BinanceMode,
    pub state: RecorderState,
    pub db_path: PathBuf,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub manual_symbols: Vec<String>,
    pub strategy_symbols: Vec<String>,
    pub watched_symbols: Vec<String>,
    pub worker_alive: bool,
    pub metrics: RecorderMetrics,
}

impl RecorderStatus {
    fn new(
        mode: BinanceMode,
        state: RecorderState,
        db_path: PathBuf,
        manual_symbols: Vec<String>,
        strategy_symbols: Vec<String>,
        watched_symbols: Vec<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            mode,
            state,
            db_path,
            started_at: if state == RecorderState::Running {
                Some(now)
            } else {
                None
            },
            updated_at: now,
            manual_symbols,
            strategy_symbols,
            watched_symbols,
            worker_alive: true,
            metrics: RecorderMetrics::default(),
        }
    }
}

struct ModeWorker {
    stop_flag: Arc<AtomicBool>,
    pub(crate) handle: JoinHandle<()>,
}

pub struct MarketDataRecorder {
    base_dir: PathBuf,
    network_enabled: bool,
    statuses: BTreeMap<BinanceMode, RecorderStatus>,
    workers: BTreeMap<BinanceMode, ModeWorker>,
}

impl std::fmt::Debug for MarketDataRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketDataRecorder")
            .field("base_dir", &self.base_dir)
            .field("network_enabled", &self.network_enabled)
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
        Self {
            base_dir: base_dir.into(),
            network_enabled: true,
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
                mode: format_mode(mode).to_string(),
            });
        }

        let db_path = self.db_path(mode);
        init_schema_for_path(&db_path)?;
        let manual_symbols = normalize_symbols(manual_symbols);
        let strategy_symbols = normalize_symbols(strategy_symbols);
        let watched_symbols = merge_symbol_sets(manual_symbols.clone(), strategy_symbols.clone());
        let status = RecorderStatus::new(
            mode,
            RecorderState::Running,
            db_path.clone(),
            manual_symbols,
            strategy_symbols,
            watched_symbols.clone(),
        );
        if self.network_enabled {
            self.spawn_worker(mode, db_path, watched_symbols)?;
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
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        });
        status.metrics = self.load_metrics(&status.db_path).unwrap_or_default();
        status.worker_alive = self.worker_alive(mode);
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
                mode: format_mode(mode).to_string(),
            });
        };
        if existing.state != RecorderState::Running {
            return Err(StorageError::RecorderNotRunning {
                mode: format_mode(mode).to_string(),
            });
        }

        if let Some(worker) = self.workers.remove(&mode) {
            worker.stop_flag.store(true, Ordering::Relaxed);
        }

        existing.state = RecorderState::Stopped;
        existing.updated_at = Utc::now();
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
            .is_some_and(|worker| !worker.handle.is_finished())
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
        if let Some(worker) = self.workers.remove(&mode) {
            worker.stop_flag.store(true, Ordering::Relaxed);
        }
        let db_path = self.db_path(mode);
        self.spawn_worker(mode, db_path, watched_symbols)
    }

    fn spawn_worker(
        &mut self,
        mode: BinanceMode,
        db_path: PathBuf,
        watched_symbols: Vec<String>,
    ) -> Result<(), StorageError> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let worker_stop_flag = stop_flag.clone();
        let handle = std::thread::Builder::new()
            .name(format!("market-recorder-{}", format_mode(mode)))
            .spawn(move || {
                let _ = rustls::crypto::ring::default_provider().install_default();
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let Ok(runtime) = runtime else {
                    return;
                };
                runtime.block_on(async move {
                    run_market_data_worker(mode, db_path, watched_symbols, worker_stop_flag).await;
                });
            })
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
        self.workers.insert(mode, ModeWorker { stop_flag, handle });
        Ok(())
    }

    fn load_metrics(&self, db_path: &Path) -> Result<RecorderMetrics, StorageError> {
        metrics_for_path(db_path)
    }

    fn db_path(&self, mode: BinanceMode) -> PathBuf {
        self.base_dir
            .join(format!("market-{}.duckdb", format_mode(mode)))
    }
}

impl Drop for MarketDataRecorder {
    fn drop(&mut self) {
        for worker in self.workers.values() {
            worker.stop_flag.store(true, Ordering::Relaxed);
        }
    }
}

async fn run_market_data_worker(
    mode: BinanceMode,
    db_path: PathBuf,
    watched_symbols: Vec<String>,
    stop_flag: Arc<AtomicBool>,
) {
    let Ok(connection) = Connection::open(&db_path) else {
        return;
    };

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        let force_order_url = format!("{}/ws/!forceOrder@arr", market_stream_base_url(mode));
        let symbol_stream_url = combined_symbol_stream_url(mode, &watched_symbols);

        let force_stream = connect_async(force_order_url).await;
        let mut force_stream = match force_stream {
            Ok((stream, _)) => stream,
            Err(error) => {
                eprintln!(
                    "market recorder: failed to connect forceOrder stream mode={} error={}",
                    format_mode(mode),
                    error
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        let mut symbol_stream = match symbol_stream_url {
            Some(url) => match connect_async(url).await {
                Ok((stream, _)) => Some(stream),
                Err(error) => {
                    eprintln!(
                        "market recorder: failed to connect symbol streams mode={} symbols={} error={}",
                        format_mode(mode),
                        watched_symbols.join(","),
                        error
                    );
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
                            if let Err(error) = handle_force_order_message(&connection, mode, &mut liquidation_seq, message) {
                                eprintln!(
                                    "market recorder: forceOrder stream handling failed mode={} error={}",
                                    format_mode(mode),
                                    error
                                );
                                break;
                            }
                        }
                        Some(Err(error)) => {
                            eprintln!(
                                "market recorder: forceOrder stream disconnected mode={} error={}",
                                format_mode(mode),
                                error
                            );
                            break
                        }
                        None => {
                            eprintln!(
                                "market recorder: forceOrder stream disconnected mode={} error=eof",
                                format_mode(mode)
                            );
                            break
                        }
                    }
                }
                message = next_symbol_message(&mut symbol_stream), if symbol_stream.is_some() => {
                    match message {
                        Some(Ok(message)) => {
                            if let Err(error) = handle_symbol_message(&connection, mode, &mut ticker_seq, &mut trade_seq, message) {
                                eprintln!(
                                    "market recorder: symbol stream handling failed mode={} error={}",
                                    format_mode(mode),
                                    error
                                );
                                break;
                            }
                        }
                        Some(Err(error)) => {
                            eprintln!(
                                "market recorder: symbol stream disconnected mode={} symbols={} error={}",
                                format_mode(mode),
                                watched_symbols.join(","),
                                error
                            );
                            break
                        }
                        None => {
                            eprintln!(
                                "market recorder: symbol stream disconnected mode={} symbols={} error=eof",
                                format_mode(mode),
                                watched_symbols.join(",")
                            );
                            break
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(250)) => {}
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
    connection: &Connection,
    mode: BinanceMode,
    sequence: &mut i64,
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
    connection
        .execute(
            "INSERT INTO raw_liquidation_events (
                event_id, mode, symbol, event_time, receive_time, force_side, price, qty, notional, raw_payload
             ) VALUES (
                ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0), ?, ?, ?, ?, ?
             )",
            params![
                *sequence,
                format_mode(mode),
                order.symbol,
                parsed.event_time,
                receive_time_ms,
                order.side,
                order.price,
                order.qty,
                order.price * order.qty,
                payload,
            ],
        )
        .map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    Ok(())
}

fn handle_symbol_message(
    connection: &Connection,
    mode: BinanceMode,
    ticker_sequence: &mut i64,
    trade_sequence: &mut i64,
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
        connection
            .execute(
                "INSERT INTO raw_book_ticker (
                    tick_id, mode, symbol, event_time, receive_time, bid, bid_qty, ask, ask_qty
                 ) VALUES (
                    ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0), ?, ?, ?, ?
                 )",
                params![
                    *ticker_sequence,
                    format_mode(mode),
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
        connection
            .execute(
                "INSERT INTO raw_agg_trades (
                    trade_id, mode, symbol, event_time, receive_time, price, qty, is_buyer_maker
                 ) VALUES (
                    ?, ?, ?, to_timestamp(? / 1000.0), to_timestamp(? / 1000.0), ?, ?, ?
                 )",
                params![
                    *trade_sequence,
                    format_mode(mode),
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
    }

    Ok(())
}

fn market_stream_base_url(_mode: BinanceMode) -> &'static str {
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
