use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode};
use tokio::sync::{mpsc, watch};

use sandbox_quant::binance::rest::BinanceRestClient;
use sandbox_quant::binance::ws::BinanceWsClient;
use sandbox_quant::config::{parse_interval_ms, Config};
use sandbox_quant::event::{AppEvent, AssetPnlEntry, LogDomain, LogLevel, LogRecord};
use sandbox_quant::model::position::Position;
use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::order_manager::{MarketKind, OrderHistoryStats, OrderManager};
use sandbox_quant::order_store;
use sandbox_quant::strategy::ma_crossover::MaCrossover;
use sandbox_quant::strategy_catalog::{StrategyCatalog, StrategyProfile};
use sandbox_quant::strategy_session;
use sandbox_quant::ui;
use sandbox_quant::ui::{AppState, GridTab};

const ORDER_HISTORY_LIMIT: usize = 20000;
const ORDER_HISTORY_SYNC_SECS: u64 = 5;

fn switch_timeframe(
    instrument: &str,
    interval: &str,
    rest_client: &Arc<BinanceRestClient>,
    config: &Config,
    app_tx: &mpsc::Sender<AppEvent>,
) {
    let interval = interval.to_string();
    let rest = rest_client.clone();
    let instrument = instrument.to_string();
    let limit = config.ui.price_history_len;
    let tx = app_tx.clone();
    let interval_ms = match parse_interval_ms(&interval) {
        Ok(ms) => ms,
        Err(e) => {
            let err_tx = tx.clone();
            tokio::spawn(async move {
                let _ = err_tx
                    .send(AppEvent::Error(format!(
                        "Invalid timeframe '{}': {}",
                        interval, e
                    )))
                    .await;
            });
            return;
        }
    };
    let iv = interval.clone();
    tokio::spawn(async move {
        let (symbol, market) = parse_instrument_label(&instrument);
        match rest
            .get_klines_for_market(&symbol, &iv, limit, market == MarketKind::Futures)
            .await
        {
            Ok(candles) => {
                let _ = tx
                    .send(AppEvent::HistoricalCandles {
                        candles,
                        interval_ms,
                        interval,
                    })
                    .await;
            }
            Err(e) => {
                let _ = tx
                    .send(AppEvent::Error(format!("Kline fetch failed: {}", e)))
                    .await;
            }
        }
    });
}

fn parse_instrument_label(label: &str) -> (String, MarketKind) {
    let trimmed = label.trim();
    if let Some(sym) = trimmed.strip_suffix(" (FUT)") {
        return (sym.to_ascii_uppercase(), MarketKind::Futures);
    }
    (trimmed.to_ascii_uppercase(), MarketKind::Spot)
}

fn normalize_instrument_label(label: &str) -> String {
    let (symbol, market) = parse_instrument_label(label);
    if market == MarketKind::Futures {
        format!("{} (FUT)", symbol)
    } else {
        symbol
    }
}

fn build_asset_pnl_snapshot(
    order_managers: &HashMap<String, OrderManager>,
    realized_pnl_by_symbol: &HashMap<String, f64>,
) -> HashMap<String, AssetPnlEntry> {
    order_managers
        .iter()
        .map(|(symbol, mgr)| {
            (
                symbol.clone(),
                AssetPnlEntry {
                    position_qty: mgr.position().qty,
                    realized_pnl_usdt: realized_pnl_by_symbol.get(symbol).copied().unwrap_or(0.0),
                    unrealized_pnl_usdt: mgr.position().unrealized_pnl,
                },
            )
        })
        .collect()
}

fn app_log(level: LogLevel, domain: LogDomain, event: &'static str, msg: impl Into<String>) -> AppEvent {
    AppEvent::LogRecord(LogRecord::new(level, domain, event, msg))
}

fn enabled_instruments(
    strategy_catalog: &StrategyCatalog,
    enabled_strategy_tags: &HashSet<String>,
) -> Vec<String> {
    let mut instruments: Vec<String> = strategy_catalog
        .profiles()
        .iter()
        .filter(|profile| enabled_strategy_tags.contains(&profile.source_tag))
        .map(|profile| normalize_instrument_label(&profile.symbol))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    instruments.sort();
    instruments
}

fn persist_strategy_session_state(
    app_state: &mut AppState,
    strategy_catalog: &StrategyCatalog,
    current_strategy_profile: &StrategyProfile,
    enabled_strategy_tags: &HashSet<String>,
) {
    if let Err(e) = strategy_session::persist_strategy_session(
        strategy_catalog,
        &current_strategy_profile.source_tag,
        enabled_strategy_tags,
    ) {
        app_state.push_log(format!("[WARN] Failed to save strategy session: {}", e));
    }
}

fn refresh_strategy_lists(
    app_state: &mut AppState,
    strategy_catalog: &StrategyCatalog,
    enabled_strategy_tags: &HashSet<String>,
) {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let lifecycle_rows = strategy_catalog.lifecycle_rows(now_ms);
    app_state.strategy_items = strategy_catalog.labels();
    app_state.strategy_item_symbols = strategy_catalog.symbols();
    app_state.strategy_item_active = strategy_catalog
        .profiles()
        .iter()
        .map(|row| enabled_strategy_tags.contains(&row.source_tag) && !app_state.paused)
        .collect();
    app_state.strategy_item_created_at_ms =
        lifecycle_rows.iter().map(|row| row.created_at_ms).collect();
    app_state.strategy_item_total_running_ms = lifecycle_rows
        .iter()
        .map(|row| row.total_running_ms)
        .collect();
}

fn mark_strategy_running(strategy_catalog: &mut StrategyCatalog, source_tag: &str) {
    let _ = strategy_catalog.mark_running(source_tag, chrono::Utc::now().timestamp_millis());
}

fn mark_strategy_stopped(strategy_catalog: &mut StrategyCatalog, source_tag: &str) {
    let _ = strategy_catalog.mark_stopped(source_tag, chrono::Utc::now().timestamp_millis());
}

fn set_strategy_enabled(
    strategy_catalog: &mut StrategyCatalog,
    enabled_strategy_tags: &mut HashSet<String>,
    source_tag: &str,
    enabled: bool,
    paused: bool,
) {
    if enabled {
        enabled_strategy_tags.insert(source_tag.to_string());
        if !paused {
            mark_strategy_running(strategy_catalog, source_tag);
        }
    } else if enabled_strategy_tags.remove(source_tag) {
        mark_strategy_stopped(strategy_catalog, source_tag);
    }
}

fn apply_symbol_selection(
    next_symbol: &str,
    current_symbol: &mut String,
    app_state: &mut AppState,
    ws_symbol_tx: &watch::Sender<String>,
    rest_client: &Arc<BinanceRestClient>,
    config: &Config,
    app_tx: &mpsc::Sender<AppEvent>,
) {
    if next_symbol == current_symbol {
        return;
    }
    *current_symbol = next_symbol.to_string();
    app_state.symbol = current_symbol.clone();
    app_state.position = Position::new(current_symbol.clone());
    app_state.candles.clear();
    app_state.current_candle = None;
    app_state.fill_markers.clear();
    app_state.open_order_history.clear();
    app_state.filled_order_history.clear();
    app_state.history_fills.clear();
    app_state.last_applied_fee = "---".to_string();
    app_state.trade_stats_reset_warned = false;
    let _ = ws_symbol_tx.send(current_symbol.clone());
    switch_timeframe(
        current_symbol,
        &app_state.timeframe,
        rest_client,
        config,
        app_tx,
    );
    app_state.push_log(format!("Symbol switched to {}", current_symbol));
    let (_, market) = parse_instrument_label(current_symbol);
    if market == MarketKind::Futures {
        app_state.push_log("Futures mode enabled (orders + chart)".to_string());
    }
}

#[tokio::main]
async fn main() -> Result<()> {
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
    let (tick_tx, mut tick_rx) = mpsc::channel::<Tick>(256);
    let (manual_order_tx, mut manual_order_rx) = mpsc::channel::<Signal>(16);
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

    // Fetch historical klines to pre-fill chart
    let (initial_api_symbol, initial_market) = parse_instrument_label(&initial_symbol);
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

    // Spawn strategy + order manager task
    let strat_app_tx = app_tx.clone();
    let strat_rest = rest_client.clone();
    let strat_config = config.clone();
    let mut strat_shutdown = shutdown_rx.clone();
    let strat_historical_closes: Vec<f64> = historical_candles.iter().map(|c| c.close).collect();
    let strat_enabled_rx = strategy_enabled_rx;
    let mut strat_symbol_rx = ws_symbol_tx.subscribe();
    tokio::spawn(async move {
        let mut selected_profile = strategy_profile_rx.borrow().clone();
        let mut selected_symbol = normalize_instrument_label(strat_symbol_rx.borrow().as_str());
        let mut profiles_by_tag: HashMap<String, StrategyProfile> = strategy_profiles_rx
            .borrow()
            .iter()
            .map(|profile| (profile.source_tag.clone(), profile.clone()))
            .collect();
        let mut enabled_strategy_tags = enabled_strategy_tags_rx.borrow().clone();
        let mut strategies: HashMap<String, MaCrossover> = profiles_by_tag
            .iter()
            .map(|(source_tag, profile)| {
                let (fast, slow, min_ticks) = profile.periods_tuple();
                (source_tag.clone(), MaCrossover::new(fast, slow, min_ticks))
            })
            .collect();
        let mut order_managers: HashMap<String, OrderManager> = HashMap::new();
        let mut realized_pnl_by_symbol: HashMap<String, f64> = HashMap::new();
        let (risk_eval_tx, mut risk_eval_rx) = mpsc::channel::<(Signal, String, String)>(64);
        let mut order_history_sync =
            tokio::time::interval(Duration::from_secs(ORDER_HISTORY_SYNC_SECS));
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
            match mgr.refresh_order_history(ORDER_HISTORY_LIMIT).await {
                Ok(history) => {
                    realized_pnl_by_symbol.insert(selected_symbol.clone(), history.stats.realized_pnl);
                    let _ = strat_app_tx
                        .send(AppEvent::OrderHistoryUpdate(history))
                        .await;
                }
                Err(e) => {
                    let _ = strat_app_tx
                        .send(app_log(
                            LogLevel::Warn,
                            LogDomain::Order,
                            "history.fetch.fail",
                            format!("Order history fetch failed: {}", e),
                        ))
                        .await;
                }
            }
            emit_rate_snapshot(&strat_app_tx, mgr);
        }
        let _ = strat_app_tx
            .send(AppEvent::AssetPnlUpdate {
                by_symbol: build_asset_pnl_snapshot(&order_managers, &realized_pnl_by_symbol),
            })
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

        for price in &strat_historical_closes {
            let tick = Tick::from_price(*price);
            for strategy in strategies.values_mut() {
                strategy.on_tick(&tick);
            }
        }
        if !strat_historical_closes.is_empty() {
            let selected_state = strategies.get(&selected_profile.source_tag);
            let _ = strat_app_tx
                .send(AppEvent::StrategyState {
                    fast_sma: selected_state.and_then(MaCrossover::fast_sma_value),
                    slow_sma: selected_state.and_then(MaCrossover::slow_sma_value),
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

                    if tick_symbol == selected_symbol {
                        let _ = strat_app_tx
                            .send(AppEvent::MarketTick(tick.clone()))
                            .await;
                    }

                    if let Some(mgr) = order_managers.get_mut(&tick_symbol) {
                        mgr.update_unrealized_pnl(tick.price);
                        if tick_symbol == selected_symbol {
                            emit_rate_snapshot(&strat_app_tx, mgr);
                        }
                    }
                    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                    if now_ms.saturating_sub(last_asset_pnl_emit_ms) >= 300 {
                        last_asset_pnl_emit_ms = now_ms;
                        let _ = strat_app_tx
                            .send(AppEvent::AssetPnlUpdate {
                                by_symbol: build_asset_pnl_snapshot(
                                    &order_managers,
                                    &realized_pnl_by_symbol,
                                ),
                            })
                            .await;
                    }

                    for (source_tag, profile) in &profiles_by_tag {
                        if !enabled_strategy_tags.contains(source_tag) {
                            continue;
                        }
                        if normalize_instrument_label(&profile.symbol) != tick_symbol {
                            continue;
                        }
                        let strategy = strategies.entry(source_tag.clone()).or_insert_with(|| {
                            let (fast, slow, min_ticks) = profile.periods_tuple();
                            MaCrossover::new(fast, slow, min_ticks)
                        });
                        let signal = strategy.on_tick(&tick);

                        if selected_profile.source_tag == *source_tag {
                            let _ = strat_app_tx
                                .send(AppEvent::StrategyState {
                                    fast_sma: strategy.fast_sma_value(),
                                    slow_sma: strategy.slow_sma_value(),
                                })
                                .await;
                        }

                        let enabled = *strat_enabled_rx.borrow();
                        if signal != Signal::Hold && enabled {
                            let _ = strat_app_tx
                                .send(AppEvent::StrategySignal {
                                    signal: signal.clone(),
                                    source_tag: source_tag.clone(),
                                    price: Some(tick.price),
                                    timestamp_ms: tick.timestamp_ms,
                                })
                                .await;
                            if let Err(e) = risk_eval_tx
                                .send((signal, source_tag.clone(), normalize_instrument_label(&profile.symbol)))
                                .await
                            {
                                tracing::error!(error = %e, "Failed to enqueue strategy signal");
                            }
                        }
                    }
                }
                Some(signal) = manual_order_rx.recv() => {
                    let _ = strat_app_tx
                        .send(AppEvent::StrategySignal {
                            signal: signal.clone(),
                            source_tag: "mnl".to_string(),
                            price: None,
                            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                        })
                        .await;
                    if let Err(e) = risk_eval_tx
                        .send((signal, "mnl".to_string(), selected_symbol.clone()))
                        .await
                    {
                        tracing::error!(error = %e, "Failed to enqueue manual signal");
                    }
                }
                Some((signal, source_tag, instrument)) = risk_eval_rx.recv() => {
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
                        match mgr.submit_order(signal, &source_tag).await {
                            Ok(Some(ref update)) => {
                                if instrument == selected_symbol {
                                    let _ = strat_app_tx
                                        .send(AppEvent::OrderUpdate(update.clone()))
                                        .await;
                                    match mgr.refresh_order_history(ORDER_HISTORY_LIMIT).await {
                                        Ok(history) => {
                                            realized_pnl_by_symbol.insert(
                                                instrument.clone(),
                                                history.stats.realized_pnl,
                                            );
                                            let _ = strat_app_tx
                                                .send(AppEvent::OrderHistoryUpdate(history))
                                                .await;
                                        }
                                        Err(e) => {
                                            let _ = strat_app_tx
                                                .send(app_log(
                                                    LogLevel::Warn,
                                                    LogDomain::Order,
                                                    "history.refresh.fail",
                                                    format!("Order history refresh failed: {}", e),
                                                ))
                                                .await;
                                        }
                                    }
                                    if matches!(update, sandbox_quant::order_manager::OrderUpdate::Filled { .. }) {
                                        let _ = strat_app_tx
                                            .send(AppEvent::BalanceUpdate(mgr.balances().clone()))
                                            .await;
                                    }
                                    emit_asset_snapshot = true;
                                    emit_rate_snapshot(&strat_app_tx, mgr);
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                let _ = strat_app_tx.send(AppEvent::Error(e.to_string())).await;
                            }
                        }
                    }
                    if emit_asset_snapshot {
                        let _ = strat_app_tx
                            .send(AppEvent::AssetPnlUpdate {
                                by_symbol: build_asset_pnl_snapshot(
                                    &order_managers,
                                    &realized_pnl_by_symbol,
                                ),
                            })
                            .await;
                    }
                }
                _ = order_history_sync.tick() => {
                    let mut aggregated_stats: HashMap<String, OrderHistoryStats> = HashMap::new();
                    for (instrument, mgr) in order_managers.iter_mut() {
                        match mgr.refresh_order_history(ORDER_HISTORY_LIMIT).await {
                            Ok(history) => {
                                if instrument == &selected_symbol {
                                    let _ = strat_app_tx
                                        .send(AppEvent::OrderHistoryUpdate(history.clone()))
                                        .await;
                                }
                                realized_pnl_by_symbol
                                    .insert(instrument.clone(), history.stats.realized_pnl);
                                for (tag, s) in history.strategy_stats {
                                    let slot = aggregated_stats.entry(tag).or_default();
                                    slot.trade_count = slot.trade_count.saturating_add(s.trade_count);
                                    slot.win_count = slot.win_count.saturating_add(s.win_count);
                                    slot.lose_count = slot.lose_count.saturating_add(s.lose_count);
                                    slot.realized_pnl += s.realized_pnl;
                                }
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
                        emit_rate_snapshot(&strat_app_tx, mgr);
                    }
                    let _ = strat_app_tx
                        .send(AppEvent::StrategyStatsUpdate {
                            strategy_stats: aggregated_stats,
                        })
                        .await;
                    let _ = strat_app_tx
                        .send(AppEvent::AssetPnlUpdate {
                            by_symbol: build_asset_pnl_snapshot(
                                &order_managers,
                                &realized_pnl_by_symbol,
                            ),
                        })
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
                            realized_pnl_by_symbol
                                .insert(selected_symbol.clone(), history.stats.realized_pnl);
                            let _ = strat_app_tx.send(AppEvent::OrderHistoryUpdate(history)).await;
                            emit_asset_snapshot = true;
                        }
                        if let Ok(balances) = mgr.refresh_balances().await {
                            let _ = strat_app_tx.send(AppEvent::BalanceUpdate(balances)).await;
                            emit_asset_snapshot = true;
                        }
                        emit_rate_snapshot(&strat_app_tx, mgr);
                    }
                    if emit_asset_snapshot {
                        let _ = strat_app_tx
                            .send(AppEvent::AssetPnlUpdate {
                                by_symbol: build_asset_pnl_snapshot(
                                    &order_managers,
                                    &realized_pnl_by_symbol,
                                ),
                            })
                            .await;
                    }
                }
                _ = strategy_profile_rx.changed() => {
                    selected_profile = strategy_profile_rx.borrow().clone();
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
                    let existing: HashSet<String> = profiles_by_tag.keys().cloned().collect();
                    strategies.retain(|source_tag, _| existing.contains(source_tag));
                    for (source_tag, profile) in &profiles_by_tag {
                        strategies.entry(source_tag.clone()).or_insert_with(|| {
                            let (fast, slow, min_ticks) = profile.periods_tuple();
                            MaCrossover::new(fast, slow, min_ticks)
                        });
                    }
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
    app_state.set_on_panel_selected(
        app_state
        .strategy_item_active
        .get(app_state.selected_grid_strategy_index())
        .copied()
        .unwrap_or(false),
    );
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
                if app_state.is_symbol_selector_open() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('t') | KeyCode::Char('T') => {
                            app_state.set_symbol_selector_open(false);
                        }
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                            app_state.set_symbol_selector_index(
                                app_state.symbol_selector_index().saturating_sub(1),
                            );
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                            app_state.set_symbol_selector_index(
                                (app_state.symbol_selector_index() + 1)
                                    .min(app_state.symbol_items.len().saturating_sub(1)),
                            );
                        }
                        KeyCode::Enter => {
                            if let Some(next_symbol) = app_state
                                .symbol_items
                                .get(app_state.symbol_selector_index())
                                .cloned()
                            {
                                apply_symbol_selection(
                                    &next_symbol,
                                    &mut current_symbol,
                                    &mut app_state,
                                    &ws_symbol_tx,
                                    &rest_client,
                                    &config,
                                    &app_tx,
                                );
                            }
                            app_state.set_symbol_selector_open(false);
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.is_strategy_selector_open() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('y') | KeyCode::Char('Y') => {
                            app_state.set_strategy_selector_open(false);
                        }
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                            app_state.set_strategy_selector_index(
                                app_state.strategy_selector_index().saturating_sub(1),
                            );
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                            app_state.set_strategy_selector_index(
                                (app_state.strategy_selector_index() + 1)
                                    .min(app_state.strategy_items.len().saturating_sub(1)),
                            );
                        }
                        KeyCode::Enter => {
                            if let Some(next_profile) = strategy_catalog
                                .get(app_state.strategy_selector_index())
                                .cloned()
                            {
                                set_strategy_enabled(
                                    &mut strategy_catalog,
                                    &mut enabled_strategy_tags,
                                    &next_profile.source_tag,
                                    true,
                                    app_state.paused,
                                );
                                apply_symbol_selection(
                                    &next_profile.symbol,
                                    &mut current_symbol,
                                    &mut app_state,
                                    &ws_symbol_tx,
                                    &rest_client,
                                    &config,
                                    &app_tx,
                                );
                                current_strategy_profile = next_profile.clone();
                                app_state.strategy_label = next_profile.label.clone();
                                app_state.set_focus_strategy_id(Some(next_profile.label.clone()));
                                refresh_strategy_lists(
                                    &mut app_state,
                                    &strategy_catalog,
                                    &enabled_strategy_tags,
                                );
                                app_state.set_selected_grid_strategy_index(
                                    strategy_catalog
                                        .index_of_label(&next_profile.label)
                                        .unwrap_or(0),
                                );
                                app_state.fast_sma = None;
                                app_state.slow_sma = None;
                                let _ = strategy_profile_tx.send(next_profile.clone());
                                let _ =
                                    strategy_profiles_tx.send(strategy_catalog.profiles().to_vec());
                                let _ =
                                    enabled_strategy_tags_tx.send(enabled_strategy_tags.clone());
                                let _ = ws_instruments_tx.send(enabled_instruments(
                                    &strategy_catalog,
                                    &enabled_strategy_tags,
                                ));
                                app_state.push_log(format!(
                                    "Strategy selected: {} (ON)",
                                    next_profile.label
                                ));
                                persist_strategy_session_state(
                                    &mut app_state,
                                    &strategy_catalog,
                                    &current_strategy_profile,
                                    &enabled_strategy_tags,
                                );
                            }
                            app_state.set_strategy_selector_open(false);
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.is_account_popup_open() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Enter => {
                            app_state.set_account_popup_open(false);
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.is_history_popup_open() {
                    match key.code {
                        KeyCode::Char('d') | KeyCode::Char('D') => {
                            app_state.history_bucket = order_store::HistoryBucket::Day;
                            app_state.refresh_history_rows();
                        }
                        KeyCode::Char('h') | KeyCode::Char('H') => {
                            app_state.history_bucket = order_store::HistoryBucket::Hour;
                            app_state.refresh_history_rows();
                        }
                        KeyCode::Char('m') | KeyCode::Char('M') => {
                            app_state.history_bucket = order_store::HistoryBucket::Month;
                            app_state.refresh_history_rows();
                        }
                        KeyCode::Esc | KeyCode::Char('i') | KeyCode::Char('I') | KeyCode::Enter => {
                            app_state.set_history_popup_open(false);
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.is_focus_popup_open() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::Enter => {
                            app_state.set_focus_popup_open(false);
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.is_strategy_editor_open() {
                    match key.code {
                        KeyCode::Esc => {
                            app_state.set_strategy_editor_open(false);
                        }
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                            app_state.strategy_editor_field =
                                app_state.strategy_editor_field.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                            app_state.strategy_editor_field =
                                (app_state.strategy_editor_field + 1).min(3);
                        }
                        KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                            match app_state.strategy_editor_field {
                                0 => {
                                    app_state.strategy_editor_symbol_index =
                                        app_state.strategy_editor_symbol_index.saturating_sub(1)
                                }
                                1 => {
                                    app_state.strategy_editor_fast =
                                        app_state.strategy_editor_fast.saturating_sub(1).max(2)
                                }
                                2 => {
                                    app_state.strategy_editor_slow =
                                        app_state.strategy_editor_slow.saturating_sub(1).max(3)
                                }
                                _ => {
                                    app_state.strategy_editor_cooldown =
                                        app_state.strategy_editor_cooldown.saturating_sub(1).max(1)
                                }
                            }
                        }
                        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                            match app_state.strategy_editor_field {
                                0 => {
                                    app_state.strategy_editor_symbol_index =
                                        (app_state.strategy_editor_symbol_index + 1)
                                            .min(app_state.symbol_items.len().saturating_sub(1))
                                }
                                1 => app_state.strategy_editor_fast += 1,
                                2 => app_state.strategy_editor_slow += 1,
                                _ => app_state.strategy_editor_cooldown += 1,
                            }
                        }
                        KeyCode::Enter => {
                            let edited_profile = strategy_catalog
                                .get(app_state.strategy_editor_index)
                                .cloned();
                            let selected_symbol = app_state
                                .symbol_items
                                .get(app_state.strategy_editor_symbol_index)
                                .cloned()
                                .unwrap_or_else(|| app_state.symbol.clone());
                            let maybe_updated = strategy_catalog.fork_profile(
                                app_state.strategy_editor_index,
                                &selected_symbol,
                                app_state.strategy_editor_fast,
                                app_state.strategy_editor_slow,
                                app_state.strategy_editor_cooldown,
                            );
                            if let Some(updated) = maybe_updated {
                                refresh_strategy_lists(
                                    &mut app_state,
                                    &strategy_catalog,
                                    &enabled_strategy_tags,
                                );
                                app_state.set_selected_grid_strategy_index(
                                    strategy_catalog.index_of_label(&updated.label).unwrap_or(0),
                                );
                                if edited_profile.as_ref().map(|p| p.source_tag.as_str())
                                    == Some(current_strategy_profile.source_tag.as_str())
                                {
                                    set_strategy_enabled(
                                        &mut strategy_catalog,
                                        &mut enabled_strategy_tags,
                                        &current_strategy_profile.source_tag,
                                        false,
                                        app_state.paused,
                                    );
                                    set_strategy_enabled(
                                        &mut strategy_catalog,
                                        &mut enabled_strategy_tags,
                                        &updated.source_tag,
                                        true,
                                        app_state.paused,
                                    );
                                    current_strategy_profile = updated.clone();
                                    app_state.strategy_label = updated.label.clone();
                                    app_state.set_focus_strategy_id(Some(updated.label.clone()));
                                    apply_symbol_selection(
                                        &updated.symbol,
                                        &mut current_symbol,
                                        &mut app_state,
                                        &ws_symbol_tx,
                                        &rest_client,
                                        &config,
                                        &app_tx,
                                    );
                                    app_state.fast_sma = None;
                                    app_state.slow_sma = None;
                                    let _ = strategy_profile_tx.send(updated.clone());
                                }
                                if let Some(before) = edited_profile.as_ref() {
                                    app_state.push_log(format!(
                                        "Strategy forked: {} -> {}",
                                        before.label, updated.label
                                    ));
                                } else {
                                    app_state
                                        .push_log(format!("Strategy forked: {}", updated.label));
                                }
                                persist_strategy_session_state(
                                    &mut app_state,
                                    &strategy_catalog,
                                    &current_strategy_profile,
                                    &enabled_strategy_tags,
                                );
                                let _ =
                                    strategy_profiles_tx.send(strategy_catalog.profiles().to_vec());
                                let _ =
                                    enabled_strategy_tags_tx.send(enabled_strategy_tags.clone());
                                let _ = ws_instruments_tx.send(enabled_instruments(
                                    &strategy_catalog,
                                    &enabled_strategy_tags,
                                ));
                            }
                            app_state.set_strategy_editor_open(false);
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.is_grid_open() {
                    match key.code {
                        KeyCode::Char('1') => {
                            app_state.set_grid_tab(GridTab::Assets);
                        }
                        KeyCode::Char('2') => {
                            app_state.set_grid_tab(GridTab::Strategies);
                        }
                        KeyCode::Char('3') => {
                            app_state.set_grid_tab(GridTab::Risk);
                        }
                        KeyCode::Char('4') => {
                            app_state.set_grid_tab(GridTab::Network);
                        }
                        KeyCode::Char('5') => {
                            app_state.set_grid_tab(GridTab::SystemLog);
                        }
                        KeyCode::Tab => {
                            if app_state.grid_tab() != GridTab::Strategies {
                                continue;
                            }
                            app_state.set_on_panel_selected(!app_state.is_on_panel_selected());
                            let panel_indices: Vec<usize> = app_state
                                .strategy_item_active
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, active)| {
                                    if *active == app_state.is_on_panel_selected() {
                                        Some(idx)
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if !panel_indices.contains(&app_state.selected_grid_strategy_index()) {
                                if let Some(first) = panel_indices.first().copied() {
                                    app_state.set_selected_grid_strategy_index(first);
                                }
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                            if app_state.grid_tab() != GridTab::Strategies {
                                continue;
                            }
                            let panel_indices: Vec<usize> = app_state
                                .strategy_item_active
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, active)| {
                                    if *active == app_state.is_on_panel_selected() {
                                        Some(idx)
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if let Some(pos) = panel_indices
                                .iter()
                                .position(|idx| *idx == app_state.selected_grid_strategy_index())
                            {
                                let next_pos = pos.saturating_sub(1);
                                app_state.set_selected_grid_strategy_index(panel_indices[next_pos]);
                            } else if let Some(first) = panel_indices.first().copied() {
                                app_state.set_selected_grid_strategy_index(first);
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                            if app_state.grid_tab() != GridTab::Strategies {
                                continue;
                            }
                            let panel_indices: Vec<usize> = app_state
                                .strategy_item_active
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, active)| {
                                    if *active == app_state.is_on_panel_selected() {
                                        Some(idx)
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if let Some(pos) = panel_indices
                                .iter()
                                .position(|idx| *idx == app_state.selected_grid_strategy_index())
                            {
                                let next_pos = (pos + 1).min(panel_indices.len().saturating_sub(1));
                                app_state.set_selected_grid_strategy_index(panel_indices[next_pos]);
                            } else if let Some(first) = panel_indices.first().copied() {
                                app_state.set_selected_grid_strategy_index(first);
                            }
                        }
                        KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                            if app_state.grid_tab() != GridTab::Strategies {
                                continue;
                            }
                            app_state.set_selected_grid_symbol_index(
                                app_state.selected_grid_symbol_index().saturating_sub(1),
                            );
                        }
                        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                            if app_state.grid_tab() != GridTab::Strategies {
                                continue;
                            }
                            app_state.set_selected_grid_symbol_index(
                                (app_state.selected_grid_symbol_index() + 1)
                                    .min(app_state.symbol_items.len().saturating_sub(1)),
                            );
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            if app_state.grid_tab() != GridTab::Strategies {
                                continue;
                            }
                            let base_index = app_state
                                .selected_grid_strategy_index()
                                .min(strategy_catalog.len().saturating_sub(1));
                            let created = strategy_catalog.add_custom_from_index(base_index);
                            refresh_strategy_lists(
                                &mut app_state,
                                &strategy_catalog,
                                &enabled_strategy_tags,
                            );
                            app_state.set_selected_grid_strategy_index(
                                strategy_catalog.index_of_label(&created.label).unwrap_or(0),
                            );
                            app_state.set_strategy_editor_open(true);
                            app_state.strategy_editor_index = app_state.selected_grid_strategy_index();
                            app_state.strategy_editor_field = 0;
                            app_state.strategy_editor_symbol_index = app_state
                                .symbol_items
                                .iter()
                                .position(|item| item == &created.symbol)
                                .unwrap_or(0);
                            app_state.strategy_editor_fast = created.fast_period;
                            app_state.strategy_editor_slow = created.slow_period;
                            app_state.strategy_editor_cooldown = created.min_ticks_between_signals;
                            app_state
                                .push_log(format!("Grid strategy registered: {}", created.label));
                            let _ = strategy_profiles_tx.send(strategy_catalog.profiles().to_vec());
                            let _ = enabled_strategy_tags_tx.send(enabled_strategy_tags.clone());
                            let _ = ws_instruments_tx.send(enabled_instruments(
                                &strategy_catalog,
                                &enabled_strategy_tags,
                            ));
                            persist_strategy_session_state(
                                &mut app_state,
                                &strategy_catalog,
                                &current_strategy_profile,
                                &enabled_strategy_tags,
                            );
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            if app_state.grid_tab() != GridTab::Strategies {
                                continue;
                            }
                            if let Some(selected_label) = app_state
                                .strategy_items
                                .get(app_state.selected_grid_strategy_index())
                                .cloned()
                            {
                                if let Some(idx) = strategy_catalog.index_of_label(&selected_label)
                                {
                                    if let Some(profile) = strategy_catalog.get(idx).cloned() {
                                        app_state.set_strategy_editor_open(true);
                                        app_state.strategy_editor_index = idx;
                                        app_state.strategy_editor_field = 0;
                                        app_state.strategy_editor_symbol_index = app_state
                                            .symbol_items
                                            .iter()
                                            .position(|item| item == &profile.symbol)
                                            .unwrap_or(0);
                                        app_state.strategy_editor_fast = profile.fast_period;
                                        app_state.strategy_editor_slow = profile.slow_period;
                                        app_state.strategy_editor_cooldown =
                                            profile.min_ticks_between_signals;
                                    }
                                }
                            }
                        }
                        KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Delete => {
                            if app_state.grid_tab() != GridTab::Strategies {
                                continue;
                            }
                            let selected_idx = app_state
                                .selected_grid_strategy_index()
                                .min(strategy_catalog.len().saturating_sub(1));
                            let selected_profile = strategy_catalog.get(selected_idx).cloned();
                            if let Some(profile) = selected_profile {
                                if !StrategyCatalog::is_custom_source_tag(&profile.source_tag) {
                                    app_state.push_log(format!(
                                        "Strategy delete blocked (builtin): {}",
                                        profile.label
                                    ));
                                } else if strategy_catalog
                                    .remove_custom_profile(selected_idx)
                                    .is_some()
                                {
                                    enabled_strategy_tags.remove(&profile.source_tag);
                                    let mut fallback_idx = selected_idx.saturating_sub(1);
                                    if strategy_catalog.len() > 0 {
                                        fallback_idx = fallback_idx
                                            .min(strategy_catalog.len().saturating_sub(1));
                                    }
                                    if profile.source_tag == current_strategy_profile.source_tag {
                                        set_strategy_enabled(
                                            &mut strategy_catalog,
                                            &mut enabled_strategy_tags,
                                            &current_strategy_profile.source_tag,
                                            false,
                                            app_state.paused,
                                        );
                                        if let Some(next_profile) =
                                            strategy_catalog.get(fallback_idx).cloned()
                                        {
                                            set_strategy_enabled(
                                                &mut strategy_catalog,
                                                &mut enabled_strategy_tags,
                                                &next_profile.source_tag,
                                                true,
                                                app_state.paused,
                                            );
                                            current_strategy_profile = next_profile.clone();
                                            app_state.strategy_label = next_profile.label.clone();
                                            app_state.set_focus_strategy_id(Some(
                                                next_profile.label.clone(),
                                            ));
                                            apply_symbol_selection(
                                                &next_profile.symbol,
                                                &mut current_symbol,
                                                &mut app_state,
                                                &ws_symbol_tx,
                                                &rest_client,
                                                &config,
                                                &app_tx,
                                            );
                                            app_state.fast_sma = None;
                                            app_state.slow_sma = None;
                                            let _ = strategy_profile_tx.send(next_profile);
                                        }
                                    }
                                    let _ = strategy_profiles_tx
                                        .send(strategy_catalog.profiles().to_vec());
                                    let _ = enabled_strategy_tags_tx
                                        .send(enabled_strategy_tags.clone());
                                    let _ = ws_instruments_tx.send(enabled_instruments(
                                        &strategy_catalog,
                                        &enabled_strategy_tags,
                                    ));
                                    refresh_strategy_lists(
                                        &mut app_state,
                                        &strategy_catalog,
                                        &enabled_strategy_tags,
                                    );
                                    app_state.set_selected_grid_strategy_index(
                                        fallback_idx.min(strategy_catalog.len().saturating_sub(1)),
                                    );
                                    app_state.set_on_panel_selected(
                                        app_state
                                        .strategy_item_active
                                        .get(app_state.selected_grid_strategy_index())
                                        .copied()
                                        .unwrap_or(false),
                                    );
                                    app_state
                                        .push_log(format!("Strategy deleted: {}", profile.label));
                                    persist_strategy_session_state(
                                        &mut app_state,
                                        &strategy_catalog,
                                        &current_strategy_profile,
                                        &enabled_strategy_tags,
                                    );
                                }
                            }
                        }
                        KeyCode::Char('o') | KeyCode::Char('O') => {
                            if app_state.grid_tab() != GridTab::Strategies {
                                continue;
                            }
                            if let Some(item) = app_state
                                .strategy_items
                                .get(app_state.selected_grid_strategy_index())
                                .cloned()
                            {
                                if let Some(next_profile) = strategy_catalog
                                    .index_of_label(&item)
                                    .and_then(|idx| strategy_catalog.get(idx).cloned())
                                {
                                    if enabled_strategy_tags.contains(&next_profile.source_tag) {
                                        set_strategy_enabled(
                                            &mut strategy_catalog,
                                            &mut enabled_strategy_tags,
                                            &next_profile.source_tag,
                                            false,
                                            app_state.paused,
                                        );
                                        app_state.push_log(format!(
                                            "Strategy OFF: {}",
                                            next_profile.label
                                        ));
                                    } else {
                                        set_strategy_enabled(
                                            &mut strategy_catalog,
                                            &mut enabled_strategy_tags,
                                            &next_profile.source_tag,
                                            true,
                                            app_state.paused,
                                        );
                                        app_state.paused = false;
                                        let _ = strategy_enabled_tx.send(true);
                                        app_state.push_log(format!(
                                            "Strategy ON from grid: {}",
                                            next_profile.label
                                        ));
                                    }
                                    let _ = strategy_profiles_tx
                                        .send(strategy_catalog.profiles().to_vec());
                                    let _ = enabled_strategy_tags_tx
                                        .send(enabled_strategy_tags.clone());
                                    let _ = ws_instruments_tx.send(enabled_instruments(
                                        &strategy_catalog,
                                        &enabled_strategy_tags,
                                    ));
                                    refresh_strategy_lists(
                                        &mut app_state,
                                        &strategy_catalog,
                                        &enabled_strategy_tags,
                                    );
                                    app_state.set_on_panel_selected(
                                        app_state
                                        .strategy_item_active
                                        .get(app_state.selected_grid_strategy_index())
                                        .copied()
                                        .unwrap_or(false),
                                    );
                                    persist_strategy_session_state(
                                        &mut app_state,
                                        &strategy_catalog,
                                        &current_strategy_profile,
                                        &enabled_strategy_tags,
                                    );
                                }
                            }
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            if app_state.grid_tab() != GridTab::Strategies {
                                continue;
                            }
                            if let Some(item) = app_state
                                .strategy_items
                                .get(app_state.selected_grid_strategy_index())
                                .cloned()
                            {
                                app_state.set_focus_symbol(Some(app_state.symbol.clone()));
                                app_state.set_focus_strategy_id(Some(item.clone()));
                                if let Some(next_profile) = strategy_catalog
                                    .index_of_label(&item)
                                    .and_then(|idx| strategy_catalog.get(idx).cloned())
                                {
                                    set_strategy_enabled(
                                        &mut strategy_catalog,
                                        &mut enabled_strategy_tags,
                                        &next_profile.source_tag,
                                        true,
                                        app_state.paused,
                                    );
                                    apply_symbol_selection(
                                        &next_profile.symbol,
                                        &mut current_symbol,
                                        &mut app_state,
                                        &ws_symbol_tx,
                                        &rest_client,
                                        &config,
                                        &app_tx,
                                    );
                                    current_strategy_profile = next_profile.clone();
                                    app_state.strategy_label = next_profile.label.clone();
                                    app_state.fast_sma = None;
                                    app_state.slow_sma = None;
                                    let _ = strategy_profile_tx.send(next_profile.clone());
                                    let _ = strategy_profiles_tx
                                        .send(strategy_catalog.profiles().to_vec());
                                    let _ = enabled_strategy_tags_tx
                                        .send(enabled_strategy_tags.clone());
                                    let _ = ws_instruments_tx.send(enabled_instruments(
                                        &strategy_catalog,
                                        &enabled_strategy_tags,
                                    ));
                                    app_state.push_log(format!(
                                        "Strategy selected from grid: {} (ON)",
                                        next_profile.label
                                    ));
                                    persist_strategy_session_state(
                                        &mut app_state,
                                        &strategy_catalog,
                                        &current_strategy_profile,
                                        &enabled_strategy_tags,
                                    );
                                }
                                app_state.set_grid_open(false);
                                app_state.set_focus_popup_open(false);
                            }
                        }
                        KeyCode::Esc | KeyCode::Char('g') | KeyCode::Char('G') => {
                            app_state.set_grid_open(false);
                        }
                        _ => {}
                    }
                    continue;
                }
                match key.code {
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        if !app_state.paused {
                            for source_tag in enabled_strategy_tags.clone() {
                                mark_strategy_stopped(&mut strategy_catalog, &source_tag);
                            }
                            app_state.paused = true;
                            let _ = strategy_enabled_tx.send(false);
                            app_state.push_log("Strategy OFF".to_string());
                            let _ = strategy_profiles_tx.send(strategy_catalog.profiles().to_vec());
                        }
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        if app_state.paused {
                            for source_tag in enabled_strategy_tags.clone() {
                                mark_strategy_running(&mut strategy_catalog, &source_tag);
                            }
                            app_state.paused = false;
                            let _ = strategy_enabled_tx.send(true);
                            app_state.push_log("Strategy ON".to_string());
                            let _ = strategy_profiles_tx.send(strategy_catalog.profiles().to_vec());
                        }
                    }
                    KeyCode::Char('b') | KeyCode::Char('B') => {
                        app_state.push_log(format!(
                            "Manual BUY ({:.2} USDT)",
                            config.strategy.order_amount_usdt
                        ));
                        let _ = manual_order_tx.try_send(Signal::Buy);
                    }
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        app_state.push_log("Manual SELL (position)".to_string());
                        let _ = manual_order_tx.try_send(Signal::Sell);
                    }
                    KeyCode::Char('0') => {
                        switch_timeframe(&current_symbol, "1s", &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('1') => {
                        switch_timeframe(&current_symbol, "1m", &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('h') | KeyCode::Char('H') => {
                        switch_timeframe(&current_symbol, "1h", &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        switch_timeframe(&current_symbol, "1d", &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('w') | KeyCode::Char('W') => {
                        switch_timeframe(&current_symbol, "1w", &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        switch_timeframe(&current_symbol, "1M", &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') => {
                        app_state.set_symbol_selector_index(
                            app_state
                                .symbol_items
                                .iter()
                                .position(|s| s == &current_symbol)
                                .unwrap_or(0),
                        );
                        app_state.set_symbol_selector_open(true);
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        app_state.set_strategy_selector_index(
                            strategy_catalog
                                .index_of_label(&current_strategy_profile.label)
                                .unwrap_or(0),
                        );
                        app_state.set_strategy_selector_open(true);
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        app_state.set_account_popup_open(true);
                    }
                    KeyCode::Char('i') | KeyCode::Char('I') => {
                        app_state.refresh_history_rows();
                        app_state.set_history_popup_open(true);
                    }
                    KeyCode::Char('g') | KeyCode::Char('G') => {
                        app_state.set_selected_grid_symbol_index(
                            app_state
                                .symbol_items
                                .iter()
                                .position(|item| item == &current_symbol)
                                .unwrap_or(0),
                        );
                        app_state.set_selected_grid_strategy_index(
                            app_state
                                .strategy_items
                                .iter()
                                .position(|item| item == &app_state.strategy_label)
                                .unwrap_or(0),
                        );
                        app_state.set_on_panel_selected(
                            app_state
                                .strategy_item_active
                                .get(app_state.selected_grid_strategy_index())
                                .copied()
                                .unwrap_or(false),
                        );
                        app_state.set_grid_open(true);
                    }
                    KeyCode::Char('f') | KeyCode::Char('F') => {
                        app_state.set_selected_grid_symbol_index(
                            app_state
                                .symbol_items
                                .iter()
                                .position(|item| item == &current_symbol)
                                .unwrap_or(0),
                        );
                        app_state.set_selected_grid_strategy_index(
                            app_state
                                .strategy_items
                                .iter()
                                .position(|item| item == &app_state.strategy_label)
                                .unwrap_or(0),
                        );
                        app_state.set_on_panel_selected(
                            app_state
                                .strategy_item_active
                                .get(app_state.selected_grid_strategy_index())
                                .copied()
                                .unwrap_or(false),
                        );
                        app_state.set_grid_open(true);
                    }
                    _ => {}
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
