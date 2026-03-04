use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode};
use futures_util::StreamExt;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio_tungstenite::tungstenite;

use app_helpers::*;
use sandbox_quant::binance::rest::BinanceRestClient;
use sandbox_quant::binance::types::{BinanceFuturesUserDataEvent, BinanceSpotUserDataEvent};
use sandbox_quant::binance::ws::BinanceWsClient;
use sandbox_quant::config::{parse_interval_ms, Config};
use sandbox_quant::doctor::maybe_run_doctor_from_args;
use sandbox_quant::error::AppError;
use sandbox_quant::event::{AppEvent, AssetPnlEntry, LogDomain, LogLevel, LogRecord};
use sandbox_quant::input::{
    parse_grid_command, parse_main_command, parse_popup_command, GridCommand, PopupCommand,
    PopupKind, UiCommand,
};
use sandbox_quant::lifecycle::{ExitOrchestrator, PositionLifecycleEngine};
use sandbox_quant::model::position::Position;
use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::order_manager::{MarketKind, OrderHistoryStats, OrderManager};
use sandbox_quant::order_store;
use sandbox_quant::predictor::{
    backfill_predictor_metrics_from_closes_volnorm, build_predictor_models,
    default_predictor_horizons, default_predictor_specs, parse_predictor_metrics_scope_key,
    predictor_metrics_scope_key, stride_closes, OnlinePredictorMetrics, PendingPrediction,
    PredictorBaseConfig, PREDICTOR_METRIC_WINDOW, PREDICTOR_WINDOW_MAX,
};
use sandbox_quant::runtime::alpha_portfolio::{
    decide_portfolio_action_from_alpha, PortfolioExecutionIntent,
};
use sandbox_quant::runtime::execution_intent_flow::process_execution_intent_for_instrument;
use sandbox_quant::runtime::internal_exit_flow::process_internal_exit_for_instrument;
use sandbox_quant::runtime::manage_state_flow::emit_portfolio_state_updates;
use sandbox_quant::runtime::order_history_sync_flow::process_periodic_sync_basic_for_instrument;
use sandbox_quant::runtime::portfolio_sync::{
    build_live_futures_position_deltas_from_account_update, build_live_futures_positions,
};
use sandbox_quant::runtime::predictor_eval::{
    observe_predictor_eval_volatility, predictor_eval_scale, PredictorEvalVolState,
};
use sandbox_quant::runtime::regime::{RegimeDetector, RegimeDetectorConfig};
use sandbox_quant::strategy_catalog::{
    strategy_kind_category_for_label, strategy_type_options_by_category, StrategyCatalog,
    StrategyKind, StrategyProfile,
};
use sandbox_quant::strategy_session;
use sandbox_quant::ui;
use sandbox_quant::ui::{AppState, GridTab};
use ui_handlers::{
    handle_account_popup_command, handle_focus_popup_command, handle_grid_key,
    handle_history_popup_command, handle_strategy_editor_key,
};

mod app_helpers;
mod ui_handlers;

const ORDER_HISTORY_LIMIT: usize = 20000;
const ORDER_HISTORY_PERIODIC_LIMIT: usize = 500;
const ORDER_HISTORY_SYNC_SECS: u64 = 5;
const ORDER_HISTORY_BACKGROUND_SYNC_SECS: u64 = 30;
const ORDER_HISTORY_BACKGROUND_SYNC_PER_TICK: usize = 4;
const FUTURES_POSITION_REST_FALLBACK_SECS: u64 = 90;
const FUTURES_USER_STREAM_KEEPALIVE_SECS: u64 = 30 * 60;
const SPOT_USER_STREAM_KEEPALIVE_SECS: u64 = 30 * 60;
const PORTFOLIO_REBALANCE_MIN_DELTA: f64 = 0.05;

enum SpotUserDataHint {
    Execution { instrument: String },
    AccountUpdate,
}

fn fallback_sigma_for_market(market: MarketKind, sigma_spot: f64, sigma_futures: f64) -> f64 {
    if market == MarketKind::Futures {
        sigma_futures.max(1e-9)
    } else {
        sigma_spot.max(1e-9)
    }
}

fn is_binance_invalid_signature_error(err: &anyhow::Error) -> bool {
    if let Some(AppError::BinanceApi { code, .. }) = err.downcast_ref::<AppError>() {
        return *code == -1022;
    }
    err.chain()
        .any(|cause| cause.to_string().contains("code -1022"))
}

fn credential_lens_hint(config: &Config) -> String {
    format!(
        "spot_key_len={} spot_secret_len={} futures_key_len={} futures_secret_len={}",
        config.binance.api_key.len(),
        config.binance.api_secret.len(),
        config.binance.futures_api_key.len(),
        config.binance.futures_api_secret.len(),
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if maybe_run_doctor_from_args(&args).await? {
        return Ok(());
    }

    // Install rustls crypto provider (required by rustls 0.23+)
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Load config
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config: {:#}", e);
            eprintln!("Make sure .env file exists with BINANCE_API_KEY and BINANCE_API_SECRET");
            std::process::exit(1);
        }
    };

    // Init tracing (log to file so it doesn't interfere with TUI)
    let log_file = std::fs::File::create("sandbox-quant.log")?;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                config
                    .logging
                    .level
                    .parse()
                    .unwrap_or_else(|_| "info".parse().unwrap())
            }),
        )
        .with_writer(log_file)
        .with_ansi(false)
        .json()
        .init();

    tracing::info!(
        symbol = %config.binance.symbol,
        rest_url = %config.binance.rest_base_url,
        ws_url = %config.binance.ws_base_url,
        "Starting sandbox-quant"
    );

    // Channels
    let (app_tx, mut app_rx) = mpsc::channel::<AppEvent>(256);
    let (tick_tx, mut tick_rx) = mpsc::channel::<Tick>(4096);
    let (manual_order_tx, mut manual_order_rx) = mpsc::channel::<Signal>(16);
    let (close_all_positions_tx, mut close_all_positions_rx) = mpsc::channel::<u64>(8);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (strategy_enabled_tx, strategy_enabled_rx) = watch::channel(true);
    let tradable_symbols = config.binance.tradable_instruments();
    let default_symbol = tradable_symbols
        .first()
        .cloned()
        .unwrap_or_else(|| config.binance.symbol.clone());
    let mut strategy_catalog = StrategyCatalog::new(
        &default_symbol,
        config.strategy.fast_period,
        config.strategy.slow_period,
        config.strategy.min_ticks_between_signals,
    );
    let mut restored_selected_source_tag: Option<String> = None;
    let mut restored_enabled_source_tags: HashSet<String> = HashSet::new();
    match strategy_session::load_strategy_session(
        &default_symbol,
        config.strategy.fast_period,
        config.strategy.slow_period,
        config.strategy.min_ticks_between_signals,
    ) {
        Ok(Some(restored)) => {
            strategy_catalog = restored.catalog;
            restored_selected_source_tag = restored.selected_source_tag;
            restored_enabled_source_tags = restored.enabled_source_tags;
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(error = %e, "Failed to load persisted strategy session");
        }
    }
    let initial_strategy_profile = restored_selected_source_tag
        .as_deref()
        .and_then(|source_tag| strategy_catalog.get_by_source_tag(source_tag).cloned())
        .or_else(|| strategy_catalog.get(0).cloned())
        .expect("strategy catalog must include default profile");
    let mut enabled_strategy_tags: HashSet<String> = strategy_catalog
        .profiles()
        .iter()
        .filter(|profile| restored_enabled_source_tags.contains(&profile.source_tag))
        .map(|profile| profile.source_tag.clone())
        .collect();
    if enabled_strategy_tags.is_empty() {
        enabled_strategy_tags.insert(initial_strategy_profile.source_tag.clone());
    }
    for source_tag in enabled_strategy_tags.clone() {
        mark_strategy_running(&mut strategy_catalog, &source_tag);
    }
    let initial_symbol = normalize_instrument_label(&initial_strategy_profile.symbol);
    let (ws_symbol_tx, _ws_symbol_rx) = watch::channel(initial_symbol.clone());
    let (strategy_profile_tx, mut strategy_profile_rx) =
        watch::channel(initial_strategy_profile.clone());
    let (strategy_profiles_tx, mut strategy_profiles_rx) =
        watch::channel(strategy_catalog.profiles().to_vec());
    let (enabled_strategy_tags_tx, mut enabled_strategy_tags_rx) =
        watch::channel(enabled_strategy_tags.clone());
    let initial_ws_instruments = enabled_instruments(&strategy_catalog, &enabled_strategy_tags);
    let (ws_instruments_tx, mut ws_instruments_rx) = watch::channel(initial_ws_instruments);

    // REST client
    let rest_client = Arc::new(BinanceRestClient::new(
        &config.binance.rest_base_url,
        &config.binance.futures_rest_base_url,
        &config.binance.api_key,
        &config.binance.api_secret,
        &config.binance.futures_api_key,
        &config.binance.futures_api_secret,
        config.binance.recv_window,
    ));
    let (initial_api_symbol, initial_market) = parse_instrument_label(&initial_symbol);

    // Verify connectivity and log to TUI
    let ping_app_tx = app_tx.clone();
    match rest_client.ping().await {
        Ok(()) => {
            tracing::info!("Binance demo ping OK");
            let _ = ping_app_tx
                .send(app_log(
                    LogLevel::Info,
                    LogDomain::System,
                    "rest.ping.ok",
                    "Binance demo ping OK",
                ))
                .await;
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to ping Binance demo");
            let _ = ping_app_tx
                .send(app_log(
                    LogLevel::Error,
                    LogDomain::System,
                    "rest.ping.fail",
                    format!("Ping failed: {}", e),
                ))
                .await;
        }
    }

    // Verify signed endpoint auth early; fail fast on invalid signature (-1022).
    let auth_check_result = if initial_market == MarketKind::Futures {
        rest_client
            .get_futures_account()
            .await
            .map(|_| "futures account auth OK")
    } else {
        rest_client
            .get_account()
            .await
            .map(|_| "spot account auth OK")
    };
    match auth_check_result {
        Ok(msg) => {
            let _ = app_tx
                .send(app_log(
                    LogLevel::Info,
                    LogDomain::System,
                    "rest.auth.ok",
                    msg,
                ))
                .await;
        }
        Err(e) => {
            let host_hint = format!(
                "rest_base_url={} futures_rest_base_url={}",
                config.binance.rest_base_url, config.binance.futures_rest_base_url
            );
            let cred_hint = credential_lens_hint(&config);
            let action_hint = if is_binance_invalid_signature_error(&e) {
                "Signed request auth failed (-1022). Check API key/secret pair for this endpoint environment (demo/testnet vs live), then restart."
            } else {
                "Signed request auth failed. Check API key permissions and endpoint environment."
            };
            let detail = format!(
                "{} [{}] [{}] cause={}",
                action_hint, host_hint, cred_hint, e
            );
            let _ = app_tx
                .send(app_log(
                    LogLevel::Error,
                    LogDomain::System,
                    "rest.auth.fail",
                    detail.clone(),
                ))
                .await;
            return Err(anyhow::anyhow!(detail));
        }
    }

    // Fetch historical klines to pre-fill chart
    let historical_candles = match rest_client
        .get_klines_for_market(
            &initial_api_symbol,
            &config.binance.kline_interval,
            config.ui.price_history_len,
            initial_market == MarketKind::Futures,
        )
        .await
    {
        Ok(candles) => {
            tracing::info!(count = candles.len(), "Fetched historical klines");
            let _ = app_tx
                .send(app_log(
                    LogLevel::Info,
                    LogDomain::System,
                    "kline.preload.ok",
                    format!("Loaded {} historical klines", candles.len()),
                ))
                .await;
            candles
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to fetch klines, starting with empty chart");
            let _ = app_tx
                .send(app_log(
                    LogLevel::Warn,
                    LogDomain::System,
                    "kline.preload.fail",
                    format!("Kline fetch failed: {}", e),
                ))
                .await;
            Vec::new()
        }
    };

    // Spawn WebSocket manager task (dynamic multi-symbol fanout)
    let ws_client = BinanceWsClient::new(
        &config.binance.ws_base_url,
        &config.binance.futures_ws_base_url,
    );
    let ws_tick_tx = tick_tx;
    let ws_app_tx = app_tx.clone();
    let mut ws_shutdown = shutdown_rx.clone();
    tokio::spawn(async move {
        let mut workers: HashMap<String, (watch::Sender<bool>, watch::Sender<String>)> =
            HashMap::new();
        loop {
            let target_instruments: HashSet<String> = ws_instruments_rx
                .borrow()
                .iter()
                .map(|s| normalize_instrument_label(s))
                .collect();

            let existing: Vec<String> = workers.keys().cloned().collect();
            for symbol in existing {
                if !target_instruments.contains(&symbol) {
                    if let Some((stop_tx, _symbol_tx)) = workers.remove(&symbol) {
                        let _ = stop_tx.send(true);
                    }
                    let _ = ws_app_tx
                        .send(app_log(
                            LogLevel::Info,
                            LogDomain::Ws,
                            "worker.unsubscribed",
                            format!("WS unsubscribed: {}", symbol),
                        ))
                        .await;
                }
            }

            for symbol in target_instruments {
                if workers.contains_key(&symbol) {
                    continue;
                }
                let worker_symbol = symbol.clone();
                let (symbol_tx, symbol_rx) = watch::channel(symbol.clone());
                let (worker_stop_tx, worker_stop_rx) = watch::channel(false);
                workers.insert(symbol.clone(), (worker_stop_tx, symbol_tx));
                let worker_client = ws_client.clone();
                let worker_tick_tx = ws_tick_tx.clone();
                let worker_app_tx = ws_app_tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = worker_client
                        .connect_and_run(
                            worker_tick_tx,
                            worker_app_tx.clone(),
                            symbol_rx,
                            worker_stop_rx,
                        )
                        .await
                    {
                        tracing::warn!(symbol = %worker_symbol, error = %e, "WS worker failed");
                        let _ = worker_app_tx
                            .send(app_log(
                                LogLevel::Warn,
                                LogDomain::Ws,
                                "worker.fail",
                                format!("WS worker failed ({}): {}", worker_symbol, e),
                            ))
                            .await;
                    }
                });
                let _ = ws_app_tx
                    .send(app_log(
                        LogLevel::Info,
                        LogDomain::Ws,
                        "worker.subscribed",
                        format!("WS subscribed: {}", symbol),
                    ))
                    .await;
            }

            tokio::select! {
                _ = ws_instruments_rx.changed() => {}
                _ = ws_shutdown.changed() => {
                    for (_, (stop_tx, _symbol_tx)) in workers {
                        let _ = stop_tx.send(true);
                    }
                    break;
                }
            }
        }
    });

    let (futures_position_delta_tx, mut futures_position_delta_rx) =
        mpsc::channel::<Vec<(String, Option<AssetPnlEntry>)>>(64);
    let futures_stream_rest = rest_client.clone();
    let futures_stream_app_tx = app_tx.clone();
    let futures_stream_ws_base = config.binance.futures_ws_base_url.clone();
    let mut futures_stream_shutdown = shutdown_rx.clone();
    tokio::spawn(async move {
        let mut reconnect_delay_secs = 1u64;
        loop {
            if *futures_stream_shutdown.borrow() {
                break;
            }
            let listen_key = match futures_stream_rest.start_futures_user_data_stream().await {
                Ok(key) => key,
                Err(e) => {
                    let _ = futures_stream_app_tx
                        .send(app_log(
                            LogLevel::Warn,
                            LogDomain::Ws,
                            "futures.user_stream.listen_key.fail",
                            format!("Failed to start futures user stream: {}", e),
                        ))
                        .await;
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(reconnect_delay_secs)) => {},
                        _ = futures_stream_shutdown.changed() => break,
                    }
                    reconnect_delay_secs = (reconnect_delay_secs.saturating_mul(2)).min(60);
                    continue;
                }
            };
            reconnect_delay_secs = 1;
            let ws_url = format!(
                "{}/{}",
                futures_stream_ws_base.trim_end_matches('/'),
                listen_key
            );
            let _ = futures_stream_app_tx
                .send(app_log(
                    LogLevel::Info,
                    LogDomain::Ws,
                    "futures.user_stream.connect",
                    format!("Connecting futures user stream: {}", ws_url),
                ))
                .await;

            let (ws_stream, _) = match tokio_tungstenite::connect_async(&ws_url).await {
                Ok(v) => v,
                Err(e) => {
                    let _ = futures_stream_rest
                        .close_futures_user_data_stream(&listen_key)
                        .await;
                    let _ = futures_stream_app_tx
                        .send(app_log(
                            LogLevel::Warn,
                            LogDomain::Ws,
                            "futures.user_stream.connect.fail",
                            format!("Futures user stream connect failed: {}", e),
                        ))
                        .await;
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(reconnect_delay_secs)) => {},
                        _ = futures_stream_shutdown.changed() => break,
                    }
                    reconnect_delay_secs = (reconnect_delay_secs.saturating_mul(2)).min(60);
                    continue;
                }
            };

            let (_write, mut read) = ws_stream.split();
            let mut keepalive =
                tokio::time::interval(Duration::from_secs(FUTURES_USER_STREAM_KEEPALIVE_SECS));
            loop {
                tokio::select! {
                    msg = read.next() => {
                        match msg {
                            Some(Ok(tungstenite::Message::Text(text))) => {
                                if let Ok(BinanceFuturesUserDataEvent::AccountUpdate(evt)) =
                                    serde_json::from_str::<BinanceFuturesUserDataEvent>(&text)
                                {
                                    let deltas = build_live_futures_position_deltas_from_account_update(
                                        &evt.account.positions,
                                        |symbol| normalize_instrument_label(&format!("{} (FUT)", symbol)),
                                    );
                                    if !deltas.is_empty() {
                                        let _ = futures_position_delta_tx.send(deltas).await;
                                    }
                                }
                            }
                            Some(Ok(tungstenite::Message::Ping(_))) => {}
                            Some(Ok(tungstenite::Message::Pong(_))) => {}
                            Some(Ok(tungstenite::Message::Close(_))) | Some(Err(_)) | None => {
                                break;
                            }
                            Some(Ok(_)) => {}
                        }
                    }
                    _ = keepalive.tick() => {
                        if let Err(e) = futures_stream_rest.keepalive_futures_user_data_stream(&listen_key).await {
                            let _ = futures_stream_app_tx
                                .send(app_log(
                                    LogLevel::Warn,
                                    LogDomain::Ws,
                                    "futures.user_stream.keepalive.fail",
                                    format!("Futures user stream keepalive failed: {}", e),
                                ))
                                .await;
                            break;
                        }
                    }
                    _ = futures_stream_shutdown.changed() => {
                        let _ = futures_stream_rest.close_futures_user_data_stream(&listen_key).await;
                        return;
                    }
                }
            }

            let _ = futures_stream_rest
                .close_futures_user_data_stream(&listen_key)
                .await;
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(reconnect_delay_secs)) => {},
                _ = futures_stream_shutdown.changed() => break,
            }
            reconnect_delay_secs = (reconnect_delay_secs.saturating_mul(2)).min(60);
        }
    });

    let (spot_user_hint_tx, mut spot_user_hint_rx) = mpsc::channel::<SpotUserDataHint>(64);
    if config.binance.api_key.trim().is_empty() {
        let _ = app_tx
            .send(app_log(
                LogLevel::Info,
                LogDomain::Ws,
                "spot.user_stream.disabled",
                "Spot user stream disabled: empty spot API key",
            ))
            .await;
    } else {
        let spot_stream_rest = rest_client.clone();
        let spot_stream_app_tx = app_tx.clone();
        let spot_stream_ws_base = config.binance.ws_base_url.clone();
        let mut spot_stream_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let mut reconnect_delay_secs = 1u64;
            loop {
                if *spot_stream_shutdown.borrow() {
                    break;
                }
                let listen_key = match spot_stream_rest.start_spot_user_data_stream().await {
                    Ok(key) => key,
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("404") || msg.contains("<html>") {
                            let _ = spot_stream_app_tx
                            .send(app_log(
                                LogLevel::Warn,
                                LogDomain::Ws,
                                "spot.user_stream.disabled",
                                format!(
                                    "Spot user stream disabled: endpoint unavailable on current rest_base_url ({})",
                                    msg
                                ),
                            ))
                            .await;
                            break;
                        }
                        let _ = spot_stream_app_tx
                            .send(app_log(
                                LogLevel::Warn,
                                LogDomain::Ws,
                                "spot.user_stream.listen_key.fail",
                                format!("Failed to start spot user stream: {}", e),
                            ))
                            .await;
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(reconnect_delay_secs)) => {},
                            _ = spot_stream_shutdown.changed() => break,
                        }
                        reconnect_delay_secs = (reconnect_delay_secs.saturating_mul(2)).min(60);
                        continue;
                    }
                };
                reconnect_delay_secs = 1;
                let ws_url = format!(
                    "{}/{}",
                    spot_stream_ws_base.trim_end_matches('/'),
                    listen_key
                );
                let (ws_stream, _) = match tokio_tungstenite::connect_async(&ws_url).await {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = spot_stream_rest
                            .close_spot_user_data_stream(&listen_key)
                            .await;
                        let _ = spot_stream_app_tx
                            .send(app_log(
                                LogLevel::Warn,
                                LogDomain::Ws,
                                "spot.user_stream.connect.fail",
                                format!("Spot user stream connect failed: {}", e),
                            ))
                            .await;
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(reconnect_delay_secs)) => {},
                            _ = spot_stream_shutdown.changed() => break,
                        }
                        reconnect_delay_secs = (reconnect_delay_secs.saturating_mul(2)).min(60);
                        continue;
                    }
                };

                let (_write, mut read) = ws_stream.split();
                let mut keepalive =
                    tokio::time::interval(Duration::from_secs(SPOT_USER_STREAM_KEEPALIVE_SECS));
                loop {
                    tokio::select! {
                        msg = read.next() => {
                            match msg {
                                Some(Ok(tungstenite::Message::Text(text))) => {
                                    if let Ok(event) = serde_json::from_str::<BinanceSpotUserDataEvent>(&text) {
                                        match event {
                                            BinanceSpotUserDataEvent::ExecutionReport(evt) => {
                                                let _ = spot_user_hint_tx
                                                    .send(SpotUserDataHint::Execution {
                                                        instrument: normalize_instrument_label(&evt.symbol),
                                                    })
                                                    .await;
                                            }
                                            BinanceSpotUserDataEvent::OutboundAccountPosition(_) => {
                                                let _ = spot_user_hint_tx.send(SpotUserDataHint::AccountUpdate).await;
                                            }
                                            BinanceSpotUserDataEvent::Unknown => {}
                                        }
                                    }
                                }
                                Some(Ok(tungstenite::Message::Ping(_))) => {}
                                Some(Ok(tungstenite::Message::Pong(_))) => {}
                                Some(Ok(tungstenite::Message::Close(_))) | Some(Err(_)) | None => {
                                    break;
                                }
                                Some(Ok(_)) => {}
                            }
                        }
                        _ = keepalive.tick() => {
                            if let Err(e) = spot_stream_rest.keepalive_spot_user_data_stream(&listen_key).await {
                                let _ = spot_stream_app_tx
                                    .send(app_log(
                                        LogLevel::Warn,
                                        LogDomain::Ws,
                                        "spot.user_stream.keepalive.fail",
                                        format!("Spot user stream keepalive failed: {}", e),
                                    ))
                                    .await;
                                break;
                            }
                        }
                        _ = spot_stream_shutdown.changed() => {
                            let _ = spot_stream_rest.close_spot_user_data_stream(&listen_key).await;
                            return;
                        }
                    }
                }
                let _ = spot_stream_rest
                    .close_spot_user_data_stream(&listen_key)
                    .await;
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(reconnect_delay_secs)) => {},
                    _ = spot_stream_shutdown.changed() => break,
                }
                reconnect_delay_secs = (reconnect_delay_secs.saturating_mul(2)).min(60);
            }
        });
    }

    // Spawn strategy + order manager task
    let strat_app_tx = app_tx.clone();
    let strat_rest = rest_client.clone();
    let strat_config = config.clone();
    let mut strat_shutdown = shutdown_rx.clone();
    let strat_historical_closes: Vec<f64> = historical_candles.iter().map(|c| c.close).collect();
    let strat_enabled_rx = strategy_enabled_rx;
    let mut strat_symbol_rx = ws_symbol_tx.subscribe();
    tokio::spawn(async move {
        let mut selected_symbol = normalize_instrument_label(strat_symbol_rx.borrow().as_str());
        let mut profiles_by_tag: HashMap<String, StrategyProfile> = strategy_profiles_rx
            .borrow()
            .iter()
            .map(|profile| (profile.source_tag.clone(), profile.clone()))
            .collect();
        let mut enabled_strategy_tags = enabled_strategy_tags_rx.borrow().clone();
        let mut order_managers: HashMap<String, OrderManager> = HashMap::new();
        let mut realized_pnl_by_symbol: HashMap<String, f64> = HashMap::new();
        let mut live_futures_positions: HashMap<String, AssetPnlEntry> = HashMap::new();
        let mut strategy_stats_by_instrument: HashMap<String, HashMap<String, OrderHistoryStats>> =
            HashMap::new();
        let mut regime_detectors: HashMap<String, RegimeDetector> = HashMap::new();
        let predictor_horizons = default_predictor_horizons();
        let base_predictor_cfg = PredictorBaseConfig {
            alpha_mean: strat_config.alpha.predictor_ewma_alpha_mean,
            alpha_var: strat_config.alpha.predictor_ewma_alpha_var,
            min_sigma: strat_config.alpha.predictor_min_sigma,
        };
        let predictor_specs = default_predictor_specs(base_predictor_cfg);
        let predictor_cfg_by_id: HashMap<String, _> = predictor_specs
            .iter()
            .map(|(id, cfg)| (id.clone(), *cfg))
            .collect();
        let mut predictor_models = build_predictor_models(&predictor_specs);
        let mut pending_predictor_eval: HashMap<String, VecDeque<PendingPrediction>> =
            HashMap::new();
        let mut predictor_vol_by_instrument: HashMap<String, PredictorEvalVolState> =
            HashMap::new();
        let mut predictor_metrics_by_scope: HashMap<String, OnlinePredictorMetrics> =
            HashMap::new();
        let mut announced_predictor_scopes: HashSet<String> = HashSet::new();
        let mut alpha_mu_by_instrument: HashMap<String, f64> = HashMap::new();
        let mut lifecycle_engine = PositionLifecycleEngine::default();
        let mut lifecycle_triggered_once: HashSet<String> = HashSet::new();
        let mut close_all_jobs: HashMap<u64, (usize, usize, usize)> = HashMap::new();
        let (execution_intent_tx, mut execution_intent_rx) =
            mpsc::channel::<PortfolioExecutionIntent>(64);
        let (internal_exit_tx, mut internal_exit_rx) = mpsc::channel::<(String, String)>(64);
        let mut order_history_sync =
            tokio::time::interval(Duration::from_secs(ORDER_HISTORY_SYNC_SECS));
        let mut last_history_sync_ms_by_instrument: HashMap<String, u64> = HashMap::new();
        let tradable_instruments: Vec<String> = strat_config
            .binance
            .tradable_instruments()
            .into_iter()
            .map(|item| normalize_instrument_label(&item))
            .collect();
        let mut background_history_sync_cursor: usize = 0;
        let mut last_futures_stream_update_ms: Option<u64> = None;
        let mut last_asset_pnl_emit_ms: u64 = 0;

        let emit_rate_snapshot = |tx: &mpsc::Sender<AppEvent>, mgr: &OrderManager| {
            let tx = tx.clone();
            let global = mgr.rate_budget_snapshot();
            let orders = mgr.orders_rate_budget_snapshot();
            let account = mgr.account_rate_budget_snapshot();
            let market_data = mgr.market_data_rate_budget_snapshot();
            tokio::spawn(async move {
                let _ = tx
                    .send(AppEvent::RiskRateSnapshot {
                        global,
                        orders,
                        account,
                        market_data,
                    })
                    .await;
            });
        };

        let bootstrap_instruments =
            strategy_instruments_from_profiles(&profiles_by_tag, &selected_symbol);
        for instrument in bootstrap_instruments {
            if !order_managers.contains_key(&instrument) {
                let (api_symbol, market) = parse_instrument_label(&instrument);
                order_managers.insert(
                    instrument.clone(),
                    OrderManager::new(
                        strat_rest.clone(),
                        &api_symbol,
                        market,
                        strat_config.strategy.order_amount_usdt,
                        &strat_config.risk,
                    ),
                );
            }
            if let Some(mgr) = order_managers.get_mut(&instrument) {
                if instrument == selected_symbol {
                    match mgr.refresh_balances().await {
                        Ok(balances) => {
                            let _ = strat_app_tx.send(AppEvent::BalanceUpdate(balances)).await;
                        }
                        Err(e) => {
                            let _ = strat_app_tx
                                .send(app_log(
                                    LogLevel::Warn,
                                    LogDomain::Portfolio,
                                    "balance.fetch.fail",
                                    format!("Balance fetch failed: {}", e),
                                ))
                                .await;
                        }
                    }
                }
                match mgr.refresh_order_history(ORDER_HISTORY_LIMIT).await {
                    Ok(history) => {
                        strategy_stats_by_instrument
                            .insert(instrument.clone(), history.strategy_stats.clone());
                        realized_pnl_by_symbol
                            .insert(instrument.clone(), history.stats.realized_pnl);
                        if instrument == selected_symbol {
                            let _ = strat_app_tx
                                .send(AppEvent::OrderHistoryUpdate(history))
                                .await;
                        }
                    }
                    Err(e) => {
                        let _ = strat_app_tx
                            .send(app_log(
                                LogLevel::Warn,
                                LogDomain::Order,
                                "history.fetch.fail",
                                format!("Order history fetch failed ({}): {}", instrument, e),
                            ))
                            .await;
                    }
                }
                if let Some(stop_price) =
                    derived_stop_price(mgr.position(), strat_config.exit.stop_loss_pct)
                {
                    let _ = strat_app_tx
                        .send(AppEvent::ExitPolicyUpdate {
                            symbol: instrument.clone(),
                            source_tag: "sys".to_string(),
                            stop_price: Some(stop_price),
                            expected_holding_ms: None,
                            protective_stop_ok: None,
                        })
                        .await;
                }
                emit_rate_snapshot(&strat_app_tx, mgr);
            }
        }
        match strat_rest.get_futures_position_risk().await {
            Ok(rows) => {
                live_futures_positions = build_live_futures_positions(&rows, |symbol| {
                    normalize_instrument_label(&format!("{} (FUT)", symbol))
                });
            }
            Err(e) => {
                let _ = strat_app_tx
                    .send(app_log(
                        LogLevel::Warn,
                        LogDomain::Portfolio,
                        "futures.positions.bootstrap.fail",
                        format!("Futures position bootstrap failed: {}", e),
                    ))
                    .await;
            }
        }
        let _ = strat_app_tx
            .send(AppEvent::StrategyStatsUpdate {
                strategy_stats: build_scoped_strategy_stats(&strategy_stats_by_instrument),
            })
            .await;
        emit_portfolio_state_updates(
            &strat_app_tx,
            &order_managers,
            &realized_pnl_by_symbol,
            &live_futures_positions,
        )
        .await;

        let _ = strat_app_tx
            .send(app_log(
                LogLevel::Info,
                LogDomain::Strategy,
                "catalog.loaded",
                format!(
                    "Strategies loaded: {} | usdt={}",
                    profiles_by_tag.len(),
                    strat_config.strategy.order_amount_usdt,
                ),
            ))
            .await;
        let _ = strat_app_tx
            .send(app_log(
                LogLevel::Info,
                LogDomain::Strategy,
                "alpha.config",
                format!(
                    "Alpha config: predictor_only=true rebalance_min_delta={:.2} sigma_spot={:.4} sigma_fut={:.4} ewma(a_mu={:.3},a_var={:.3},min_sig={:.4})",
                    PORTFOLIO_REBALANCE_MIN_DELTA,
                    strat_config.alpha.predictor_sigma_spot,
                    strat_config.alpha.predictor_sigma_futures,
                    strat_config.alpha.predictor_ewma_alpha_mean,
                    strat_config.alpha.predictor_ewma_alpha_var,
                    strat_config.alpha.predictor_min_sigma
                ),
            ))
            .await;

        // Bootstrap predictor rows for all tradable instruments so R2 is
        // measurable even when an instrument is not currently selected/traded.
        let base_interval_ms =
            parse_interval_ms(&strat_config.binance.kline_interval).unwrap_or(60_000);
        let predictor_bootstrap_limit = strat_config.ui.price_history_len.max(1200);
        let mut predictor_history_by_instrument: HashMap<String, Vec<f64>> = HashMap::new();
        predictor_history_by_instrument
            .insert(selected_symbol.clone(), strat_historical_closes.clone());
        for instrument in &tradable_instruments {
            if predictor_history_by_instrument.contains_key(instrument) {
                continue;
            }
            let (api_symbol, market) = parse_instrument_label(instrument);
            match strat_rest
                .get_klines_for_market(
                    &api_symbol,
                    &strat_config.binance.kline_interval,
                    predictor_bootstrap_limit,
                    market == MarketKind::Futures,
                )
                .await
            {
                Ok(candles) => {
                    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
                    predictor_history_by_instrument.insert(instrument.clone(), closes);
                }
                Err(e) => {
                    let _ = strat_app_tx
                        .send(app_log(
                            LogLevel::Warn,
                            LogDomain::Strategy,
                            "predictor.backfill.fail",
                            format!(
                                "Predictor backfill failed: {} get_klines_for_market failed: {}",
                                instrument, e
                            ),
                        ))
                        .await;
                    predictor_history_by_instrument.insert(instrument.clone(), Vec::new());
                }
            }
        }
        for instrument in &tradable_instruments {
            let (_api_symbol, market) = parse_instrument_label(&instrument);
            let market_label = if market == MarketKind::Futures {
                "futures".to_string()
            } else {
                "spot".to_string()
            };
            let instrument_closes = predictor_history_by_instrument
                .get(instrument)
                .cloned()
                .unwrap_or_default();
            for (predictor_id, predictor_cfg) in &predictor_specs {
                for (horizon, horizon_ms) in &predictor_horizons {
                    let scope_key =
                        predictor_metrics_scope_key(&instrument, market, predictor_id, horizon);
                    announced_predictor_scopes.insert(scope_key.clone());
                    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                    let stride =
                        ((*horizon_ms).max(base_interval_ms) / base_interval_ms).max(1) as usize;
                    let sampled = stride_closes(&instrument_closes, stride);
                    let metrics = backfill_predictor_metrics_from_closes_volnorm(
                        &sampled,
                        predictor_cfg.alpha_mean,
                        predictor_cfg.alpha_var,
                        predictor_cfg.min_sigma,
                        PREDICTOR_METRIC_WINDOW,
                    );
                    predictor_metrics_by_scope.insert(scope_key, metrics.clone());
                    let _ = strat_app_tx
                        .send(AppEvent::PredictorMetricsUpdate {
                            symbol: instrument.clone(),
                            market: market_label.clone(),
                            predictor: predictor_id.clone(),
                            horizon: horizon.clone(),
                            r2: metrics.r2(),
                            hit_rate: metrics.hit_rate(),
                            mae: metrics.mae(),
                            sample_count: metrics.sample_count(),
                            updated_at_ms: now_ms,
                        })
                        .await;
                }
            }
        }

        if !strat_historical_closes.is_empty() {
            let _ = strat_app_tx
                .send(AppEvent::StrategyState {
                    fast_sma: None,
                    slow_sma: None,
                })
                .await;
        }

        loop {
            tokio::select! {
                result = tick_rx.recv() => {
                    let tick = match result {
                        Some(t) => t,
                        None => {
                            tracing::info!("Tick channel closed, strategy task exiting");
                            break;
                        }
                    };
                    let tick_symbol = normalize_instrument_label(&tick.symbol);
                    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                    let regime_signal = {
                        let detector = regime_detectors
                            .entry(tick_symbol.clone())
                            .or_insert_with(|| RegimeDetector::new(RegimeDetectorConfig::default()));
                        detector.update(tick.price, now_ms)
                    };
                    let _ = strat_app_tx
                        .send(AppEvent::RegimeUpdate {
                            symbol: tick_symbol.clone(),
                            regime: regime_signal,
                        })
                        .await;
                    for model in predictor_models.values_mut() {
                        model.observe_price(&tick_symbol, tick.price);
                    }
                    let vol_state = predictor_vol_by_instrument
                        .entry(tick_symbol.clone())
                        .or_default();
                    observe_predictor_eval_volatility(
                        vol_state,
                        tick.price,
                        strat_config.alpha.predictor_ewma_alpha_var,
                    );
                    if tick.price > f64::EPSILON {
                        let (_, tick_market) = parse_instrument_label(&tick_symbol);
                        let fallback_sigma = fallback_sigma_for_market(
                            tick_market,
                            strat_config.alpha.predictor_sigma_spot,
                            strat_config.alpha.predictor_sigma_futures,
                        );
                        let mut alpha_mu = 0.0_f64;
                        let mut alpha_abs = 0.0_f64;
                        for (predictor_id, predictor_model) in predictor_models.iter() {
                            let y_for_predictor = predictor_model.estimate_base(
                                &tick_symbol,
                                strat_config.alpha.predictor_mu,
                                fallback_sigma,
                            );
                            if y_for_predictor.mu.abs() > alpha_abs {
                                alpha_abs = y_for_predictor.mu.abs();
                                alpha_mu = y_for_predictor.mu;
                            }
                            for (horizon, horizon_ms) in &predictor_horizons {
                                let scope_key = predictor_metrics_scope_key(
                                    &tick_symbol,
                                    tick_market,
                                    predictor_id,
                                    horizon,
                                );
                                let predictor_min_sigma = predictor_cfg_by_id
                                    .get(predictor_id)
                                    .map(|c| c.min_sigma)
                                    .unwrap_or(strat_config.alpha.predictor_min_sigma);
                                let norm_scale = predictor_eval_scale(vol_state, predictor_min_sigma);
                                let queue = pending_predictor_eval.entry(scope_key).or_default();
                                queue.push_back(PendingPrediction {
                                    due_ms: now_ms.saturating_add(*horizon_ms),
                                    base_price: tick.price,
                                    mu: y_for_predictor.mu,
                                    norm_scale,
                                });
                                if queue.len() > PREDICTOR_WINDOW_MAX {
                                    let drop_n = queue.len() - PREDICTOR_WINDOW_MAX;
                                    queue.drain(..drop_n);
                                }
                                let announced_key = predictor_metrics_scope_key(
                                    &tick_symbol,
                                    tick_market,
                                    predictor_id,
                                    horizon,
                                );
                                if announced_predictor_scopes.insert(announced_key) {
                                    let market_label = if tick_market == MarketKind::Futures {
                                        "futures".to_string()
                                    } else {
                                        "spot".to_string()
                                    };
                                    let _ = strat_app_tx
                                        .send(AppEvent::PredictorMetricsUpdate {
                                            symbol: tick_symbol.clone(),
                                            market: market_label,
                                            predictor: predictor_id.clone(),
                                            horizon: horizon.clone(),
                                            r2: None,
                                            hit_rate: None,
                                            mae: None,
                                            sample_count: 0,
                                            updated_at_ms: now_ms,
                                        })
                                        .await;
                                }
                            }
                        }
                        alpha_mu_by_instrument.insert(tick_symbol.clone(), alpha_mu);
                    }
                    let symbol_prefix = format!("{}::", tick_symbol);
                    let mut resolved_metric_updates: Vec<(String, String, String, String, OnlinePredictorMetrics)> = Vec::new();
                    for (scope_key, queue) in pending_predictor_eval.iter_mut() {
                        if !scope_key.starts_with(&symbol_prefix) {
                            continue;
                        }
                        let mut resolved_any = false;
                        while let Some(front) = queue.front() {
                            if now_ms < front.due_ms {
                                break;
                            }
                            let item = queue.pop_front().expect("front checked");
                            if item.base_price <= f64::EPSILON || tick.price <= f64::EPSILON {
                                continue;
                            }
                            if !item.norm_scale.is_finite() || item.norm_scale <= f64::EPSILON {
                                continue;
                            }
                            let y_real = (tick.price / item.base_price).ln();
                            let y_real_norm = y_real / item.norm_scale;
                            let y_pred_norm = item.mu / item.norm_scale;
                            let metric = predictor_metrics_by_scope
                                .entry(scope_key.clone())
                                .or_default();
                            metric.observe(y_real_norm, y_pred_norm);
                            resolved_any = true;
                        }
                        if resolved_any {
                            if let Some((symbol, market, predictor, horizon)) =
                                parse_predictor_metrics_scope_key(scope_key)
                            {
                                if let Some(metric) = predictor_metrics_by_scope.get(scope_key) {
                                    resolved_metric_updates.push((
                                        symbol,
                                        market,
                                        predictor,
                                        horizon,
                                        metric.clone(),
                                    ));
                                }
                            }
                        }
                    }
                    for (symbol, market, predictor, horizon, metric) in resolved_metric_updates {
                        let _ = strat_app_tx
                            .send(AppEvent::PredictorMetricsUpdate {
                                symbol,
                                market,
                                predictor,
                                horizon,
                                r2: metric.r2(),
                                hit_rate: metric.hit_rate(),
                                mae: metric.mae(),
                                sample_count: metric.sample_count(),
                                updated_at_ms: now_ms,
                            })
                            .await;
                    }

                    if tick_symbol == selected_symbol {
                        let _ = strat_app_tx
                            .send(AppEvent::MarketTick(tick.clone()))
                            .await;
                    }

                    if let Some(mgr) = order_managers.get_mut(&tick_symbol) {
                        mgr.update_unrealized_pnl(tick.price);
                        if let Some(trigger) =
                            lifecycle_engine.on_tick(&tick_symbol, tick.price, now_ms)
                        {
                            if !lifecycle_triggered_once.contains(&tick_symbol) {
                                lifecycle_triggered_once.insert(tick_symbol.clone());
                                let reason_code = ExitOrchestrator::decide(trigger).to_string();
                                if let Err(e) = internal_exit_tx
                                    .send((tick_symbol.clone(), reason_code.clone()))
                                    .await
                                {
                                    tracing::error!(error = %e, "Failed to enqueue internal exit");
                                } else {
                                    let _ = strat_app_tx
                                        .send(app_log(
                                            LogLevel::Info,
                                            LogDomain::Risk,
                                            "lifecycle.exit.trigger",
                                            format!(
                                                "Lifecycle exit trigger queued: {} ({})",
                                                tick_symbol, reason_code
                                            ),
                                        ))
                                        .await;
                                }
                            }
                        }
                        if tick_symbol == selected_symbol {
                            emit_rate_snapshot(&strat_app_tx, mgr);
                        }
                    }
                    if now_ms.saturating_sub(last_asset_pnl_emit_ms) >= 300 {
                        last_asset_pnl_emit_ms = now_ms;
                        emit_portfolio_state_updates(
                            &strat_app_tx,
                            &order_managers,
                            &realized_pnl_by_symbol,
                            &live_futures_positions,
                        )
                        .await;
                    }

                    let portfolio_source_tag = profiles_by_tag
                        .values()
                        .find(|profile| {
                            normalize_instrument_label(&profile.symbol) == tick_symbol
                                && enabled_strategy_tags.contains(&profile.source_tag)
                        })
                        .map(|p| p.source_tag.to_ascii_lowercase())
                        .unwrap_or_else(|| "alp".to_string());
                    let portfolio_enabled = *strat_enabled_rx.borrow()
                        && profiles_by_tag.values().any(|profile| {
                            normalize_instrument_label(&profile.symbol) == tick_symbol
                                && enabled_strategy_tags.contains(&profile.source_tag)
                        });
                    if portfolio_enabled {
                        let queue_instrument = tick_symbol.clone();
                        let (is_flat, pos_qty, pos_entry) = order_managers
                            .get(&queue_instrument)
                            .map(|m| {
                                (
                                    m.position().is_flat(),
                                    m.position().qty,
                                    m.position().entry_price,
                                )
                            })
                            .unwrap_or((true, 0.0, 0.0));
                        let current_position_ratio = order_managers
                            .get(&queue_instrument)
                            .map(|m| {
                                let px = m
                                    .last_price()
                                    .or_else(|| {
                                        (tick.price > f64::EPSILON).then_some(tick.price)
                                    })
                                    .or_else(|| (pos_entry > f64::EPSILON).then_some(pos_entry))
                                    .unwrap_or(0.0);
                                if px <= f64::EPSILON {
                                    return 0.0;
                                }
                                let current_notional = pos_qty.abs() * px;
                                let base_notional =
                                    strat_config.strategy.order_amount_usdt.max(f64::EPSILON);
                                (current_notional / base_notional).clamp(0.0, 1.0)
                            })
                            .unwrap_or(0.0);
                        let alpha_mu = alpha_mu_by_instrument
                            .get(&queue_instrument)
                            .copied()
                            .unwrap_or(0.0);
                        let queue_regime = regime_signal;
                        let portfolio = decide_portfolio_action_from_alpha(
                            &queue_instrument,
                            now_ms,
                            is_flat,
                            alpha_mu,
                            strat_config.strategy.order_amount_usdt,
                            queue_regime,
                        );
                        let intent = portfolio.to_intent(
                            &portfolio_source_tag,
                            strat_config.strategy.order_amount_usdt,
                            current_position_ratio,
                        );
                        let signal = intent.effective_signal(PORTFOLIO_REBALANCE_MIN_DELTA);
                        for predictor_model in predictor_models.values_mut() {
                            predictor_model.observe_signal_price(
                                &tick_symbol,
                                &portfolio_source_tag,
                                &signal,
                                tick.price,
                            );
                        }

                        if signal != Signal::Hold {
                            let _ = strat_app_tx
                                .send(AppEvent::StrategySignal {
                                    signal: signal.clone(),
                                    symbol: tick_symbol.clone(),
                                    source_tag: portfolio_source_tag.clone(),
                                    price: Some(tick.price),
                                    timestamp_ms: tick.timestamp_ms,
                                })
                                .await;
                            let _ = strat_app_tx
                                .send(app_log(
                                    LogLevel::Info,
                                    LogDomain::Strategy,
                                    "alpha.signal",
                                    format!(
                                        "alpha signal [{}] src={} mu={:+.6} side={:?} regime={:?} conf={:.3} exp={:+.4} strength={:.3} target={:.2}",
                                        queue_instrument,
                                        portfolio_source_tag,
                                        alpha_mu,
                                        portfolio.alpha.side_bias,
                                        portfolio.regime,
                                        portfolio.regime_confidence,
                                        portfolio.alpha.expected_return_usdt,
                                        portfolio.alpha.strength,
                                        portfolio.target_position_ratio
                                    ),
                                ))
                                .await;
                            let _ = strat_app_tx
                                .send(app_log(
                                    LogLevel::Info,
                                    LogDomain::Strategy,
                                    "portfolio.decision",
                                    format!(
                                        "portfolio decision [{}] src={} sig={:?} regime={:?} target={:.2} current={:.2} delta={:+.2} ev={:+.4} strength={:.3} reason={}",
                                        queue_instrument,
                                        portfolio_source_tag,
                                        signal,
                                        portfolio.regime,
                                        intent.target_position_ratio,
                                        current_position_ratio,
                                        intent.position_delta_ratio,
                                        portfolio.alpha.expected_return_usdt,
                                        portfolio.alpha.strength,
                                        portfolio.reason
                                    ),
                                ))
                                .await;
                            if let Err(e) = execution_intent_tx
                                .send(intent)
                                .await
                            {
                                let _ = strat_app_tx
                                    .send(app_log(
                                        LogLevel::Warn,
                                        LogDomain::Strategy,
                                        "execution.intent.enqueue.fail",
                                        format!(
                                            "Failed to enqueue portfolio signal [{}|{}]: {}",
                                            queue_instrument, portfolio_source_tag, e
                                        ),
                                    ))
                                    .await;
                            }
                        }
                    }
                }
                Some(signal) = manual_order_rx.recv() => {
                    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                    let source_tag = "mnl".to_string();
                    let target_position_ratio = if matches!(signal, Signal::Buy) {
                        1.0
                    } else {
                        0.0
                    };
                    let current_position_ratio = order_managers
                        .get(&selected_symbol)
                        .map(|m| {
                            let px = m
                                .last_price()
                                .or_else(|| {
                                    (m.position().entry_price > f64::EPSILON)
                                        .then_some(m.position().entry_price)
                                })
                                .unwrap_or(0.0);
                            if px <= f64::EPSILON {
                                return 0.0;
                            }
                            let base_notional = strat_config.strategy.order_amount_usdt.max(f64::EPSILON);
                            ((m.position().qty.abs() * px) / base_notional).clamp(0.0, 1.0)
                        })
                        .unwrap_or(0.0);
                    let _ = strat_app_tx
                        .send(AppEvent::StrategySignal {
                            signal: signal.clone(),
                            symbol: selected_symbol.clone(),
                            source_tag: source_tag.clone(),
                            price: None,
                            timestamp_ms: now_ms,
                        })
                        .await;
                    if let Err(e) = execution_intent_tx
                        .send(PortfolioExecutionIntent {
                            symbol: selected_symbol.clone(),
                            source_tag: source_tag.clone(),
                            target_position_ratio,
                            position_delta_ratio: target_position_ratio - current_position_ratio,
                            desired_notional_usdt: strat_config.strategy.order_amount_usdt
                                * target_position_ratio,
                            expected_return_usdt: 0.0,
                            strength: 0.0,
                            reason: "manual.intent",
                            timestamp_ms: now_ms,
                        })
                        .await
                    {
                        tracing::error!(error = %e, "Failed to enqueue manual signal");
                    }
                }
                Some(job_id) = close_all_positions_rx.recv() => {
                    let close_targets: Vec<String> = order_managers
                        .iter()
                        .filter(|(_, mgr)| !mgr.position().is_flat())
                        .map(|(instrument, _)| instrument.clone())
                        .collect();
                    let total = close_targets.len();
                    close_all_jobs.insert(job_id, (total, 0, 0));
                    let _ = strat_app_tx
                        .send(AppEvent::CloseAllRequested {
                            job_id,
                            total,
                            symbols: close_targets.clone(),
                        })
                        .await;
                    let _ = strat_app_tx
                        .send(app_log(
                            LogLevel::Warn,
                            LogDomain::Risk,
                            "position.close_all.request",
                            format!(
                                "Close-all requested: {} open positions (job #{})",
                                total, job_id
                            ),
                        ))
                        .await;
                    for instrument in close_targets {
                        if let Err(e) = internal_exit_tx
                            .send((instrument.clone(), close_all_reason_code(job_id)))
                            .await
                        {
                            let mut completed = 0usize;
                            let mut failed = 0usize;
                            if let Some(state) = close_all_jobs.get_mut(&job_id) {
                                state.1 = state.1.saturating_add(1);
                                state.2 = state.2.saturating_add(1);
                                completed = state.1;
                                failed = state.2;
                            }
                            let _ = strat_app_tx
                                .send(app_log(
                                    LogLevel::Warn,
                                    LogDomain::Risk,
                                    "position.close_all.enqueue.fail",
                                    format!("Close-all enqueue failed ({}): {}", instrument, e),
                                ))
                                .await;
                            let _ = strat_app_tx
                                .send(AppEvent::CloseAllProgress {
                                    job_id,
                                    symbol: instrument.clone(),
                                    completed,
                                    total,
                                    failed,
                                    reason: Some(e.to_string()),
                                })
                                .await;
                            if completed >= total {
                                let _ = strat_app_tx
                                    .send(AppEvent::CloseAllFinished {
                                        job_id,
                                        completed,
                                        total,
                                        failed,
                                    })
                                    .await;
                                close_all_jobs.remove(&job_id);
                            }
                        }
                    }
                    if total == 0 {
                        let _ = strat_app_tx
                            .send(AppEvent::CloseAllFinished {
                                job_id,
                                completed: 0,
                                total: 0,
                                failed: 0,
                            })
                            .await;
                        close_all_jobs.remove(&job_id);
                    }
                }
                Some(intent) = execution_intent_rx.recv() => {
                    let source_tag = intent.source_tag.clone();
                    let instrument = intent.symbol.clone();
                    let _ = strat_app_tx
                        .send(app_log(
                            LogLevel::Debug,
                            LogDomain::Strategy,
                            "execution.intent.recv",
                            format!(
                                "intent recv [{}|{}] target={:.2} delta={:+.2} desired_notional={:.2} ev={:+.4} strength={:.3} reason={}",
                                instrument,
                                source_tag,
                                intent.target_position_ratio,
                                intent.position_delta_ratio,
                                intent.desired_notional_usdt,
                                intent.expected_return_usdt,
                                intent.strength,
                                intent.reason
                            ),
                        ))
                        .await;
                    if !order_managers.contains_key(&instrument) {
                        let (api_symbol, market) = parse_instrument_label(&instrument);
                        order_managers.insert(
                            instrument.clone(),
                            OrderManager::new(
                                strat_rest.clone(),
                                &api_symbol,
                                market,
                                strat_config.strategy.order_amount_usdt,
                                &strat_config.risk,
                            ),
                        );
                    }
                    let mut emit_asset_snapshot = false;
                    if let Some(mgr) = order_managers.get_mut(&instrument) {
                        let signal = intent.effective_signal(PORTFOLIO_REBALANCE_MIN_DELTA);
                        let result = process_execution_intent_for_instrument(
                            &strat_app_tx,
                            mgr,
                            &instrument,
                            &source_tag,
                            signal,
                            &selected_symbol,
                            ORDER_HISTORY_LIMIT,
                            &mut strategy_stats_by_instrument,
                            &mut realized_pnl_by_symbol,
                            build_scoped_strategy_stats,
                        )
                        .await;
                        if result.emit_asset_snapshot {
                            emit_asset_snapshot = true;
                        }
                        if result.emit_rate_snapshot {
                            emit_rate_snapshot(&strat_app_tx, mgr);
                        }
                    }
                    if emit_asset_snapshot {
                        emit_portfolio_state_updates(
                            &strat_app_tx,
                            &order_managers,
                            &realized_pnl_by_symbol,
                            &live_futures_positions,
                        )
                        .await;
                    }
                }
                Some((instrument, reason_code)) = internal_exit_rx.recv() => {
                    let source_tag_lc = "sys".to_string();
                    let close_all_job_id = parse_close_all_job_id(&reason_code);
                    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                    let _ = strat_app_tx
                        .send(AppEvent::StrategySignal {
                            signal: Signal::Sell,
                            symbol: instrument.clone(),
                            source_tag: source_tag_lc.clone(),
                            price: None,
                            timestamp_ms: now_ms,
                        })
                        .await;
                    if !order_managers.contains_key(&instrument) {
                        let (api_symbol, market) = parse_instrument_label(&instrument);
                        order_managers.insert(
                            instrument.clone(),
                            OrderManager::new(
                                strat_rest.clone(),
                                &api_symbol,
                                market,
                                strat_config.strategy.order_amount_usdt,
                                &strat_config.risk,
                            ),
                        );
                    }
                    let mut emit_asset_snapshot = false;
                    if let Some(mgr) = order_managers.get_mut(&instrument) {
                        let result = process_internal_exit_for_instrument(
                            &strat_app_tx,
                            mgr,
                            &instrument,
                            &source_tag_lc,
                            &reason_code,
                            &selected_symbol,
                            ORDER_HISTORY_LIMIT,
                            close_all_job_id,
                            &mut close_all_jobs,
                            &mut strategy_stats_by_instrument,
                            &mut realized_pnl_by_symbol,
                            &mut lifecycle_triggered_once,
                            &mut lifecycle_engine,
                            close_all_soft_skip_reason,
                            build_scoped_strategy_stats,
                        )
                        .await;
                        if result.emit_asset_snapshot {
                            emit_asset_snapshot = true;
                        }
                        if result.emit_rate_snapshot {
                            emit_rate_snapshot(&strat_app_tx, mgr);
                        }
                    }
                    if emit_asset_snapshot {
                        emit_portfolio_state_updates(
                            &strat_app_tx,
                            &order_managers,
                            &realized_pnl_by_symbol,
                            &live_futures_positions,
                        )
                        .await;
                    }
                }
                Some(deltas) = futures_position_delta_rx.recv() => {
                    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                    for (instrument, entry) in deltas {
                        if let Some(snapshot) = entry {
                            live_futures_positions.insert(instrument, snapshot);
                        } else {
                            live_futures_positions.remove(&instrument);
                        }
                    }
                    last_futures_stream_update_ms = Some(now_ms);
                    emit_portfolio_state_updates(
                        &strat_app_tx,
                        &order_managers,
                        &realized_pnl_by_symbol,
                        &live_futures_positions,
                    )
                    .await;
                }
                Some(hint) = spot_user_hint_rx.recv() => {
                    match hint {
                        SpotUserDataHint::Execution { instrument } => {
                            if !order_managers.contains_key(&instrument) {
                                let (api_symbol, market) = parse_instrument_label(&instrument);
                                order_managers.insert(
                                    instrument.clone(),
                                    OrderManager::new(
                                        strat_rest.clone(),
                                        &api_symbol,
                                        market,
                                        strat_config.strategy.order_amount_usdt,
                                        &strat_config.risk,
                                    ),
                                );
                            }
                            let mut emit_asset_snapshot = false;
                            if let Some(mgr) = order_managers.get_mut(&instrument) {
                                let last_sync_ms = last_history_sync_ms_by_instrument
                                    .get(&instrument)
                                    .copied()
                                    .unwrap_or(0);
                                let history_limit = if last_sync_ms == 0 {
                                    ORDER_HISTORY_LIMIT
                                } else {
                                    ORDER_HISTORY_PERIODIC_LIMIT
                                };
                                if let Ok(history) = mgr.refresh_order_history(history_limit).await {
                                    strategy_stats_by_instrument
                                        .insert(instrument.clone(), history.strategy_stats.clone());
                                    realized_pnl_by_symbol
                                        .insert(instrument.clone(), history.stats.realized_pnl);
                                    last_history_sync_ms_by_instrument.insert(
                                        instrument.clone(),
                                        chrono::Utc::now().timestamp_millis() as u64,
                                    );
                                    if instrument == selected_symbol {
                                        let _ = strat_app_tx.send(AppEvent::OrderHistoryUpdate(history)).await;
                                        if let Ok(balances) = mgr.refresh_balances().await {
                                            let _ = strat_app_tx.send(AppEvent::BalanceUpdate(balances)).await;
                                        }
                                    }
                                    emit_asset_snapshot = true;
                                }
                                emit_rate_snapshot(&strat_app_tx, mgr);
                            }
                            if emit_asset_snapshot {
                                let _ = strat_app_tx
                                    .send(AppEvent::StrategyStatsUpdate {
                                        strategy_stats: build_scoped_strategy_stats(&strategy_stats_by_instrument),
                                    })
                                    .await;
                                emit_portfolio_state_updates(
                                    &strat_app_tx,
                                    &order_managers,
                                    &realized_pnl_by_symbol,
                                    &live_futures_positions,
                                )
                                .await;
                            }
                        }
                        SpotUserDataHint::AccountUpdate => {
                            let (_, selected_market) = parse_instrument_label(&selected_symbol);
                            if selected_market == MarketKind::Spot {
                                if let Some(mgr) = order_managers.get_mut(&selected_symbol) {
                                    if let Ok(balances) = mgr.refresh_balances().await {
                                        let _ = strat_app_tx.send(AppEvent::BalanceUpdate(balances)).await;
                                        emit_portfolio_state_updates(
                                            &strat_app_tx,
                                            &order_managers,
                                            &realized_pnl_by_symbol,
                                            &live_futures_positions,
                                        )
                                        .await;
                                    }
                                }
                            }
                        }
                    }
                }
                _ = order_history_sync.tick() => {
                    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                    let stream_stale = last_futures_stream_update_ms
                        .map(|ts| now_ms.saturating_sub(ts) >= FUTURES_POSITION_REST_FALLBACK_SECS * 1000)
                        .unwrap_or(true);
                    if stream_stale {
                        match strat_rest.get_futures_position_risk().await {
                            Ok(rows) => {
                                live_futures_positions =
                                    build_live_futures_positions(&rows, |symbol| {
                                        normalize_instrument_label(&format!("{} (FUT)", symbol))
                                    });
                            }
                            Err(e) => {
                                let _ = strat_app_tx
                                    .send(app_log(
                                        LogLevel::Warn,
                                        LogDomain::Portfolio,
                                        "futures.positions.sync.fail",
                                        format!("Futures position sync failed: {}", e),
                                    ))
                                    .await;
                            }
                        }
                    }
                    if !order_managers.contains_key(&selected_symbol) {
                        let (api_symbol, market) = parse_instrument_label(&selected_symbol);
                        order_managers.insert(
                            selected_symbol.clone(),
                            OrderManager::new(
                                strat_rest.clone(),
                                &api_symbol,
                                market,
                                strat_config.strategy.order_amount_usdt,
                                &strat_config.risk,
                            ),
                        );
                    }
                    if let Some(mgr) = order_managers.get_mut(&selected_symbol) {
                        let last_sync_ms = last_history_sync_ms_by_instrument
                            .get(&selected_symbol)
                            .copied()
                            .unwrap_or(0);
                        if now_ms.saturating_sub(last_sync_ms) >= ORDER_HISTORY_SYNC_SECS * 1000 {
                            let history_limit = if last_sync_ms == 0 {
                                ORDER_HISTORY_LIMIT
                            } else {
                                ORDER_HISTORY_PERIODIC_LIMIT
                            };
                            process_periodic_sync_basic_for_instrument(
                                &strat_app_tx,
                                mgr,
                                &selected_symbol,
                                &selected_symbol,
                                history_limit,
                                &mut strategy_stats_by_instrument,
                                &mut realized_pnl_by_symbol,
                                strat_config.exit.stop_loss_pct,
                            )
                            .await;
                            last_history_sync_ms_by_instrument
                                .insert(selected_symbol.clone(), now_ms);
                            emit_rate_snapshot(&strat_app_tx, mgr);
                        }
                    }

                    if !tradable_instruments.is_empty() {
                        let total = tradable_instruments.len();
                        let mut due_symbols: Vec<(String, usize)> = Vec::new();
                        for _ in 0..total {
                            if due_symbols.len() >= ORDER_HISTORY_BACKGROUND_SYNC_PER_TICK {
                                break;
                            }
                            let idx = background_history_sync_cursor % total;
                            background_history_sync_cursor = (background_history_sync_cursor + 1) % total;
                            let instrument = tradable_instruments[idx].clone();
                            if instrument == selected_symbol {
                                continue;
                            }
                            let last_sync_ms = last_history_sync_ms_by_instrument
                                .get(&instrument)
                                .copied()
                                .unwrap_or(0);
                            if now_ms.saturating_sub(last_sync_ms)
                                < ORDER_HISTORY_BACKGROUND_SYNC_SECS * 1000
                            {
                                continue;
                            }
                            if !order_managers.contains_key(&instrument) {
                                let (api_symbol, market) = parse_instrument_label(&instrument);
                                order_managers.insert(
                                    instrument.clone(),
                                    OrderManager::new(
                                        strat_rest.clone(),
                                        &api_symbol,
                                        market,
                                        strat_config.strategy.order_amount_usdt,
                                        &strat_config.risk,
                                    ),
                                );
                            }
                            let history_limit = if last_sync_ms == 0 {
                                ORDER_HISTORY_LIMIT
                            } else {
                                ORDER_HISTORY_PERIODIC_LIMIT
                            };
                            due_symbols.push((instrument, history_limit));
                        }
                        if !due_symbols.is_empty() {
                            let mut join_set = JoinSet::new();
                            for (instrument, history_limit) in due_symbols {
                                let Some(mgr) = order_managers.remove(&instrument) else {
                                    continue;
                                };
                                let stop_loss_pct = strat_config.exit.stop_loss_pct;
                                join_set.spawn(async move {
                                    let mut mgr = mgr;
                                    let history = mgr.refresh_order_history(history_limit).await;
                                    let stop_price = derived_stop_price(mgr.position(), stop_loss_pct);
                                    (instrument, mgr, history, stop_price)
                                });
                            }
                            while let Some(joined) = join_set.join_next().await {
                                match joined {
                                    Ok((instrument, mgr, history_result, stop_price)) => {
                                        order_managers.insert(instrument.clone(), mgr);
                                        match history_result {
                                            Ok(history) => {
                                                strategy_stats_by_instrument
                                                    .insert(instrument.clone(), history.strategy_stats.clone());
                                                realized_pnl_by_symbol
                                                    .insert(instrument.clone(), history.stats.realized_pnl);
                                                if instrument == selected_symbol {
                                                    let _ = strat_app_tx.send(AppEvent::OrderHistoryUpdate(history)).await;
                                                }
                                                last_history_sync_ms_by_instrument
                                                    .insert(instrument.clone(), now_ms);
                                            }
                                            Err(e) => {
                                                let _ = strat_app_tx
                                                    .send(app_log(
                                                        LogLevel::Warn,
                                                        LogDomain::Order,
                                                        "history.sync.fail",
                                                        format!(
                                                            "Periodic order history sync failed ({}): {}",
                                                            instrument, e
                                                        ),
                                                    ))
                                                    .await;
                                            }
                                        }
                                        if let Some(stop_price) = stop_price {
                                            let _ = strat_app_tx
                                                .send(AppEvent::ExitPolicyUpdate {
                                                    symbol: instrument.clone(),
                                                    source_tag: "sys".to_string(),
                                                    stop_price: Some(stop_price),
                                                    expected_holding_ms: None,
                                                    protective_stop_ok: None,
                                                })
                                                .await;
                                        }
                                        if let Some(mgr_ref) = order_managers.get(&instrument) {
                                            emit_rate_snapshot(&strat_app_tx, mgr_ref);
                                        }
                                    }
                                    Err(e) => {
                                        let _ = strat_app_tx
                                            .send(app_log(
                                                LogLevel::Warn,
                                                LogDomain::Order,
                                                "history.sync.task.fail",
                                                format!("Periodic sync task join failed: {}", e),
                                            ))
                                            .await;
                                    }
                                }
                            }
                        }
                    }
                    let _ = strat_app_tx
                        .send(AppEvent::StrategyStatsUpdate {
                            strategy_stats: build_scoped_strategy_stats(&strategy_stats_by_instrument),
                        })
                        .await;
                    emit_portfolio_state_updates(
                        &strat_app_tx,
                        &order_managers,
                        &realized_pnl_by_symbol,
                        &live_futures_positions,
                    )
                    .await;
                }
                _ = strat_symbol_rx.changed() => {
                    selected_symbol = normalize_instrument_label(strat_symbol_rx.borrow().as_str());
                    if !order_managers.contains_key(&selected_symbol) {
                        let (api_symbol, market) = parse_instrument_label(&selected_symbol);
                        order_managers.insert(
                            selected_symbol.clone(),
                            OrderManager::new(
                                strat_rest.clone(),
                                &api_symbol,
                                market,
                                strat_config.strategy.order_amount_usdt,
                                &strat_config.risk,
                            ),
                        );
                    }
                    let _ = strat_app_tx
                        .send(app_log(
                            LogLevel::Info,
                            LogDomain::Ui,
                            "symbol.switch",
                            format!("Switched symbol to {}", selected_symbol),
                        ))
                        .await;
                    let mut emit_asset_snapshot = false;
                    if let Some(mgr) = order_managers.get_mut(&selected_symbol) {
                        if let Ok(history) = mgr.refresh_order_history(ORDER_HISTORY_LIMIT).await {
                            strategy_stats_by_instrument
                                .insert(selected_symbol.clone(), history.strategy_stats.clone());
                            realized_pnl_by_symbol
                                .insert(selected_symbol.clone(), history.stats.realized_pnl);
                            let _ = strat_app_tx.send(AppEvent::OrderHistoryUpdate(history)).await;
                            let _ = strat_app_tx
                                .send(AppEvent::StrategyStatsUpdate {
                                    strategy_stats: build_scoped_strategy_stats(
                                        &strategy_stats_by_instrument,
                                    ),
                                })
                                .await;
                            emit_asset_snapshot = true;
                        }
                        if let Some(stop_price) = derived_stop_price(mgr.position(), strat_config.exit.stop_loss_pct) {
                            let _ = strat_app_tx
                                .send(AppEvent::ExitPolicyUpdate {
                                    symbol: selected_symbol.clone(),
                                    source_tag: "sys".to_string(),
                                    stop_price: Some(stop_price),
                                    expected_holding_ms: None,
                                    protective_stop_ok: None,
                                })
                                .await;
                        }
                        if let Ok(balances) = mgr.refresh_balances().await {
                            let _ = strat_app_tx.send(AppEvent::BalanceUpdate(balances)).await;
                            emit_asset_snapshot = true;
                        }
                        emit_rate_snapshot(&strat_app_tx, mgr);
                    }
                    if emit_asset_snapshot {
                        emit_portfolio_state_updates(
                            &strat_app_tx,
                            &order_managers,
                            &realized_pnl_by_symbol,
                            &live_futures_positions,
                        )
                        .await;
                    }
                }
                _ = strategy_profile_rx.changed() => {
                    let selected_profile = strategy_profile_rx.borrow().clone();
                    let _ = strat_app_tx
                        .send(app_log(
                            LogLevel::Info,
                            LogDomain::Strategy,
                            "strategy.switch",
                            format!("Strategy switched: {}", selected_profile.label),
                        ))
                        .await;
                }
                _ = strategy_profiles_rx.changed() => {
                    let next_profiles = strategy_profiles_rx.borrow().clone();
                    profiles_by_tag = next_profiles
                        .iter()
                        .map(|profile| (profile.source_tag.clone(), profile.clone()))
                        .collect();
                    let bootstrap_instruments =
                        strategy_instruments_from_profiles(&profiles_by_tag, &selected_symbol);
                    for instrument in bootstrap_instruments {
                        if !order_managers.contains_key(&instrument) {
                            let (api_symbol, market) = parse_instrument_label(&instrument);
                            order_managers.insert(
                                instrument.clone(),
                                OrderManager::new(
                                    strat_rest.clone(),
                                    &api_symbol,
                                    market,
                                    strat_config.strategy.order_amount_usdt,
                                    &strat_config.risk,
                                ),
                            );
                        }
                        if let Some(mgr) = order_managers.get_mut(&instrument) {
                            if let Ok(history) = mgr.refresh_order_history(ORDER_HISTORY_LIMIT).await {
                                strategy_stats_by_instrument
                                    .insert(instrument.clone(), history.strategy_stats.clone());
                                realized_pnl_by_symbol
                                    .insert(instrument.clone(), history.stats.realized_pnl);
                                if instrument == selected_symbol {
                                    let _ = strat_app_tx
                                        .send(AppEvent::OrderHistoryUpdate(history))
                                    .await;
                                }
                            }
                            if let Some(stop_price) = derived_stop_price(mgr.position(), strat_config.exit.stop_loss_pct) {
                                let _ = strat_app_tx
                                    .send(AppEvent::ExitPolicyUpdate {
                                        symbol: instrument.clone(),
                                        source_tag: "sys".to_string(),
                                        stop_price: Some(stop_price),
                                        expected_holding_ms: None,
                                        protective_stop_ok: None,
                                    })
                                    .await;
                            }
                            emit_rate_snapshot(&strat_app_tx, mgr);
                        }
                    }
                    let _ = strat_app_tx
                        .send(AppEvent::StrategyStatsUpdate {
                            strategy_stats: build_scoped_strategy_stats(
                                &strategy_stats_by_instrument,
                            ),
                        })
                        .await;
                    emit_portfolio_state_updates(
                        &strat_app_tx,
                        &order_managers,
                        &realized_pnl_by_symbol,
                        &live_futures_positions,
                    )
                    .await;
                }
                _ = enabled_strategy_tags_rx.changed() => {
                    enabled_strategy_tags = enabled_strategy_tags_rx.borrow().clone();
                }
                _ = strat_shutdown.changed() => {
                    tracing::info!("Strategy task shutting down");
                    break;
                }
            }
        }
    });

    // Ctrl+C handler
    let ctrl_c_shutdown = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Ctrl+C received");
        let _ = ctrl_c_shutdown.send(true);
    });

    // TUI main loop
    let mut terminal = ratatui::init();
    let candle_interval_ms = config
        .binance
        .kline_interval_ms()
        .context("validated binance.kline_interval became invalid at runtime")?;
    let mut app_state = AppState::new(
        &initial_symbol,
        &initial_strategy_profile.label,
        config.ui.price_history_len,
        candle_interval_ms,
        &config.binance.kline_interval,
    );
    app_state.refresh_history_rows();
    app_state.refresh_today_realized_pnl_usdt();
    app_state.symbol_items = tradable_symbols.clone();
    refresh_strategy_lists(&mut app_state, &strategy_catalog, &enabled_strategy_tags);
    app_state.set_selected_grid_strategy_index(
        app_state
            .strategy_items
            .iter()
            .position(|item| item == &initial_strategy_profile.label)
            .unwrap_or(0),
    );
    app_state.set_selected_grid_symbol_index(
        app_state
            .symbol_items
            .iter()
            .position(|item| item == &initial_strategy_profile.symbol)
            .unwrap_or(0),
    );
    sync_on_panel_selection(&mut app_state);
    app_state.set_grid_open(true);

    // Pre-fill chart with historical candles
    if !historical_candles.is_empty() {
        app_state.candles = historical_candles;
        if app_state.candles.len() > app_state.price_history_len {
            let excess = app_state.candles.len() - app_state.price_history_len;
            app_state.candles.drain(..excess);
        }
    }

    app_state.push_log(format!("sandbox-quant started | {} | demo", initial_symbol));
    let mut current_symbol = initial_symbol.clone();
    let mut current_strategy_profile = initial_strategy_profile;
    let mut close_all_job_seq: u64 = 0;

    loop {
        refresh_strategy_lists(&mut app_state, &strategy_catalog, &enabled_strategy_tags);
        // Draw
        terminal.draw(|frame| ui::render(frame, &app_state))?;

        // Handle input (non-blocking with timeout)
        if crossterm::event::poll(Duration::from_millis(config.ui.refresh_rate_ms))? {
            if let Event::Key(key) = crossterm::event::read()? {
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q')) {
                    tracing::info!("User quit");
                    let _ = shutdown_tx.send(true);
                    break;
                }
                if app_state.is_close_all_confirm_open() {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                            app_state.set_close_all_confirm_open(false);
                            if app_state.is_close_all_running() {
                                app_state.push_log("[WARN] close-all already running".to_string());
                            } else {
                                close_all_job_seq = close_all_job_seq.saturating_add(1);
                                app_state.close_all_running = true;
                                app_state.close_all_job_id = Some(close_all_job_seq);
                                app_state.close_all_total = 0;
                                app_state.close_all_completed = 0;
                                app_state.close_all_failed = 0;
                                app_state.close_all_current_symbol = None;
                                app_state.close_all_status_expire_at_ms = None;
                                app_state.push_log(format!(
                                    "Close ALL positions confirmed (job #{})",
                                    close_all_job_seq
                                ));
                                if close_all_positions_tx.try_send(close_all_job_seq).is_err() {
                                    app_state.push_log(
                                        "[WARN] Close-all queue busy; retry in a moment"
                                            .to_string(),
                                    );
                                }
                            }
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            app_state.set_close_all_confirm_open(false);
                            app_state.push_log("Close ALL positions canceled".to_string());
                        }
                        _ => {}
                    }
                    continue;
                }
                if matches!(
                    parse_main_command(&key.code),
                    Some(UiCommand::CloseAllPositions)
                ) {
                    if app_state.is_close_all_running() {
                        app_state.push_log("[WARN] close-all already running".to_string());
                        continue;
                    }
                    app_state.set_close_all_confirm_open(true);
                    app_state.push_log("Confirm close-all: [Y]es / [N]o".to_string());
                    continue;
                }
                if app_state.is_symbol_selector_open() {
                    if let Some(cmd) = parse_popup_command(PopupKind::SymbolSelector, &key.code) {
                        handle_symbol_selector_popup_command(
                            cmd,
                            &mut app_state,
                            &mut current_symbol,
                            &ws_symbol_tx,
                            &rest_client,
                            &config,
                            &app_tx,
                        );
                    }
                    continue;
                }
                if app_state.is_strategy_selector_open() {
                    if let Some(cmd) = parse_popup_command(PopupKind::StrategySelector, &key.code) {
                        handle_strategy_selector_popup_command(
                            cmd,
                            &mut app_state,
                            &mut strategy_catalog,
                            &mut enabled_strategy_tags,
                            &mut current_symbol,
                            &mut current_strategy_profile,
                            &ws_symbol_tx,
                            &rest_client,
                            &config,
                            &app_tx,
                            &strategy_profile_tx,
                            &strategy_profiles_tx,
                            &enabled_strategy_tags_tx,
                            &ws_instruments_tx,
                        );
                    }
                    continue;
                }
                if app_state.is_account_popup_open() {
                    if let Some(cmd) = parse_popup_command(PopupKind::Account, &key.code) {
                        handle_account_popup_command(cmd, &mut app_state);
                    }
                    continue;
                }
                if app_state.is_history_popup_open() {
                    if let Some(cmd) = parse_popup_command(PopupKind::History, &key.code) {
                        handle_history_popup_command(cmd, &mut app_state);
                    }
                    continue;
                }
                if app_state.is_focus_popup_open() {
                    if let Some(cmd) = parse_popup_command(PopupKind::Focus, &key.code) {
                        handle_focus_popup_command(cmd, &mut app_state);
                    }
                    continue;
                }
                if app_state.is_strategy_editor_open() {
                    handle_strategy_editor_key(
                        &key.code,
                        &mut app_state,
                        &mut strategy_catalog,
                        &mut enabled_strategy_tags,
                        &mut current_symbol,
                        &mut current_strategy_profile,
                        &ws_symbol_tx,
                        &rest_client,
                        &config,
                        &app_tx,
                        &strategy_profile_tx,
                        &strategy_profiles_tx,
                        &enabled_strategy_tags_tx,
                        &ws_instruments_tx,
                    );
                    continue;
                }
                if app_state.is_grid_open() {
                    handle_grid_key(
                        &key.code,
                        &mut app_state,
                        &mut strategy_catalog,
                        &mut enabled_strategy_tags,
                        &mut current_symbol,
                        &mut current_strategy_profile,
                        &ws_symbol_tx,
                        &rest_client,
                        &config,
                        &app_tx,
                        &strategy_profile_tx,
                        &strategy_profiles_tx,
                        &enabled_strategy_tags_tx,
                        &strategy_enabled_tx,
                        &ws_instruments_tx,
                    );
                    continue;
                }
                if let Some(cmd) = parse_main_command(&key.code) {
                    match cmd {
                        UiCommand::Pause => {
                            if !app_state.paused {
                                for source_tag in enabled_strategy_tags.clone() {
                                    mark_strategy_stopped(&mut strategy_catalog, &source_tag);
                                }
                                app_state.paused = true;
                                let _ = strategy_enabled_tx.send(false);
                                app_state.push_log("Strategy OFF".to_string());
                                let _ =
                                    strategy_profiles_tx.send(strategy_catalog.profiles().to_vec());
                            }
                        }
                        UiCommand::Resume => {
                            if app_state.paused {
                                for source_tag in enabled_strategy_tags.clone() {
                                    mark_strategy_running(&mut strategy_catalog, &source_tag);
                                }
                                app_state.paused = false;
                                let _ = strategy_enabled_tx.send(true);
                                app_state.push_log("Strategy ON".to_string());
                                let _ =
                                    strategy_profiles_tx.send(strategy_catalog.profiles().to_vec());
                            }
                        }
                        UiCommand::ManualBuy => {
                            app_state.push_log(format!(
                                "Manual BUY ({:.2} USDT)",
                                config.strategy.order_amount_usdt
                            ));
                            let _ = manual_order_tx.try_send(Signal::Buy);
                        }
                        UiCommand::ManualSell => {
                            app_state.push_log("Manual SELL (position)".to_string());
                            let _ = manual_order_tx.try_send(Signal::Sell);
                        }
                        UiCommand::CloseAllPositions => {}
                        UiCommand::SwitchTimeframe(interval) => {
                            switch_timeframe(
                                &current_symbol,
                                interval,
                                &rest_client,
                                &config,
                                &app_tx,
                            );
                        }
                        UiCommand::OpenSymbolSelector => {
                            app_state.set_symbol_selector_index(
                                app_state
                                    .symbol_items
                                    .iter()
                                    .position(|s| s == &current_symbol)
                                    .unwrap_or(0),
                            );
                            app_state.set_symbol_selector_open(true);
                        }
                        UiCommand::OpenStrategySelector => {
                            app_state.set_strategy_selector_index(
                                strategy_catalog
                                    .index_of_label(&current_strategy_profile.label)
                                    .unwrap_or(0),
                            );
                            app_state.set_strategy_selector_open(true);
                        }
                        UiCommand::OpenAccountPopup => {
                            app_state.set_account_popup_open(true);
                        }
                        UiCommand::OpenHistoryPopup => {
                            app_state.refresh_history_rows();
                            app_state.set_history_popup_open(true);
                        }
                        UiCommand::OpenGrid => {
                            open_grid_from_current_selection(&mut app_state, &current_symbol);
                        }
                    }
                }
            }
        }

        // Drain events from channel
        while let Ok(evt) = app_rx.try_recv() {
            app_state.apply(evt);
        }

        // Check shutdown
        if *shutdown_rx.borrow() {
            break;
        }
    }

    strategy_catalog.stop_all_running(chrono::Utc::now().timestamp_millis());
    if let Err(e) = strategy_session::persist_strategy_session(
        &strategy_catalog,
        &current_strategy_profile.source_tag,
        &enabled_strategy_tags,
    ) {
        tracing::warn!(error = %e, "Failed to persist strategy session during shutdown");
    }

    ratatui::restore();
    tracing::info!("Shutdown complete");
    println!("Goodbye! Check sandbox-quant.log for details.");
    Ok(())
}
