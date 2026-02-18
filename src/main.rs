use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode};
use tokio::sync::{mpsc, watch};

use sandbox_quant::binance::rest::BinanceRestClient;
use sandbox_quant::binance::ws::BinanceWsClient;
use sandbox_quant::config::{parse_interval_ms, Config};
use sandbox_quant::event::AppEvent;
use sandbox_quant::model::position::Position;
use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::order_manager::{MarketKind, OrderManager};
use sandbox_quant::order_store;
use sandbox_quant::runtime::strategy_registry::StrategyWorkerRegistry;
use sandbox_quant::strategy::ma_crossover::MaCrossover;
use sandbox_quant::strategy_catalog::{StrategyCatalog, StrategyProfile};
use sandbox_quant::strategy_session;
use sandbox_quant::ui;
use sandbox_quant::ui::AppState;

const ORDER_HISTORY_LIMIT: usize = 20000;
const ORDER_HISTORY_SYNC_SECS: u64 = 5;

fn strategy_worker_id(profile: &StrategyProfile, api_symbol: &str, market: MarketKind) -> String {
    let market_tag = match market {
        MarketKind::Spot => "spot",
        MarketKind::Futures => "futures",
    };
    format!("{}:{}:{}", profile.source_tag, api_symbol, market_tag)
}

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

fn persist_strategy_session_state(
    app_state: &mut AppState,
    strategy_catalog: &StrategyCatalog,
    current_strategy_profile: &StrategyProfile,
) {
    if let Err(e) = strategy_session::persist_strategy_session(
        strategy_catalog,
        &current_strategy_profile.source_tag,
    ) {
        app_state.push_log(format!("[WARN] Failed to save strategy session: {}", e));
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
    app_state.history_trade_count = 0;
    app_state.history_win_count = 0;
    app_state.history_lose_count = 0;
    app_state.history_realized_pnl = 0.0;
    app_state.history_estimated_total_pnl_usdt = Some(0.0);
    app_state.strategy_stats.clear();
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
    let initial_symbol = tradable_symbols
        .first()
        .cloned()
        .unwrap_or_else(|| config.binance.symbol.clone());
    let (ws_symbol_tx, ws_symbol_rx) = watch::channel(initial_symbol.clone());
    let mut strategy_catalog = StrategyCatalog::new(
        config.strategy.fast_period,
        config.strategy.slow_period,
        config.strategy.min_ticks_between_signals,
    );
    let mut restored_selected_source_tag: Option<String> = None;
    match strategy_session::load_strategy_session(
        config.strategy.fast_period,
        config.strategy.slow_period,
        config.strategy.min_ticks_between_signals,
    ) {
        Ok(Some(restored)) => {
            strategy_catalog = restored.catalog;
            restored_selected_source_tag = restored.selected_source_tag;
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
    let (strategy_profile_tx, mut strategy_profile_rx) =
        watch::channel(initial_strategy_profile.clone());

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
                .send(AppEvent::LogMessage("Binance demo ping OK".to_string()))
                .await;
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to ping Binance demo");
            let _ = ping_app_tx
                .send(AppEvent::LogMessage(format!("[ERR] Ping failed: {}", e)))
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
                .send(AppEvent::LogMessage(format!(
                    "Loaded {} historical klines",
                    candles.len()
                )))
                .await;
            candles
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to fetch klines, starting with empty chart");
            let _ = app_tx
                .send(AppEvent::LogMessage(format!(
                    "[WARN] Kline fetch failed: {}",
                    e
                )))
                .await;
            Vec::new()
        }
    };

    // Spawn WebSocket task
    let ws_client = BinanceWsClient::new(
        &config.binance.ws_base_url,
        &config.binance.futures_ws_base_url,
    );
    let ws_tick_tx = tick_tx;
    // ^ Move tick_tx into WS task. This way, when WS task drops ws_tick_tx,
    //   the strategy task's tick_rx.recv() returns None → clean shutdown.
    let ws_app_tx = app_tx.clone();
    let ws_shutdown = shutdown_rx.clone();
    tokio::spawn(async move {
        if let Err(e) = ws_client
            .connect_and_run(ws_tick_tx, ws_app_tx.clone(), ws_symbol_rx, ws_shutdown)
            .await
        {
            tracing::error!(error = %e, "WebSocket task failed");
            let _ = ws_app_tx
                .send(AppEvent::LogMessage(format!("[ERR] WS task failed: {}", e)))
                .await;
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
        let mut active_profile = strategy_profile_rx.borrow().clone();
        let (mut fast_period, mut slow_period, mut min_ticks) = active_profile.periods_tuple();
        let mut strategy = MaCrossover::new(fast_period, slow_period, min_ticks);
        let mut current_symbol = strat_symbol_rx.borrow().clone();
        let (mut current_api_symbol, mut current_market) = parse_instrument_label(&current_symbol);
        let mut order_mgr = OrderManager::new(
            strat_rest.clone(),
            &current_api_symbol,
            current_market,
            strat_config.strategy.order_amount_usdt,
            &strat_config.risk,
        );
        let mut worker_registry = StrategyWorkerRegistry::default();
        let mut worker_id = strategy_worker_id(&active_profile, &current_api_symbol, current_market);
        let (worker_tick_tx, mut worker_tick_rx) = mpsc::channel::<Tick>(256);
        worker_registry.register(worker_id.clone(), current_api_symbol.clone(), worker_tick_tx);
        let (risk_eval_tx, mut risk_eval_rx) = mpsc::channel::<(Signal, String)>(64);
        let mut order_history_sync =
            tokio::time::interval(Duration::from_secs(ORDER_HISTORY_SYNC_SECS));

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

        // Fetch initial balances
        match order_mgr.refresh_balances().await {
            Ok(balances) => {
                let usdt = balances.get("USDT").copied().unwrap_or(0.0);
                let btc = balances.get("BTC").copied().unwrap_or(0.0);
                let _ = strat_app_tx
                    .send(AppEvent::LogMessage(format!(
                        "Balances: {:.2} USDT, {:.5} BTC",
                        usdt, btc
                    )))
                    .await;
                let _ = strat_app_tx.send(AppEvent::BalanceUpdate(balances)).await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch initial balances");
                let _ = strat_app_tx
                    .send(AppEvent::LogMessage(format!(
                        "[WARN] Balance fetch failed: {}",
                        e
                    )))
                    .await;
            }
        }
        match order_mgr.refresh_order_history(ORDER_HISTORY_LIMIT).await {
            Ok(history) => {
                let _ = strat_app_tx
                    .send(AppEvent::OrderHistoryUpdate(history))
                    .await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch initial order history");
                let _ = strat_app_tx
                    .send(AppEvent::LogMessage(format!(
                        "[WARN] Order history fetch failed: {}",
                        e
                    )))
                    .await;
            }
        }
        emit_rate_snapshot(&strat_app_tx, &order_mgr);

        let _ = strat_app_tx
            .send(AppEvent::LogMessage(format!(
                "Strategy: MA({}/{}) usdt={} cooldown={}",
                fast_period, slow_period, strat_config.strategy.order_amount_usdt, min_ticks,
            )))
            .await;

        // Warm up SMA indicators with historical kline close prices (no orders)
        for price in &strat_historical_closes {
            let tick = Tick::from_price(*price);
            strategy.on_tick(&tick);
        }
        if !strat_historical_closes.is_empty() {
            tracing::info!(
                count = strat_historical_closes.len(),
                fast_sma = ?strategy.fast_sma_value(),
                slow_sma = ?strategy.slow_sma_value(),
                "SMA indicators warmed up from historical klines"
            );
            let _ = strat_app_tx
                .send(AppEvent::StrategyState {
                    fast_sma: strategy.fast_sma_value(),
                    slow_sma: strategy.slow_sma_value(),
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

                    // Forward tick to UI
                    let _ = strat_app_tx
                        .send(AppEvent::MarketTick(tick.clone()))
                        .await;
                    worker_registry.dispatch_tick(tick);
                }
                result = worker_tick_rx.recv() => {
                    let tick = match result {
                        Some(t) => t,
                        None => {
                            tracing::warn!("Worker tick channel closed; skipping tick");
                            continue;
                        }
                    };

                    // Always run strategy to keep SMA state updated, but skip orders if paused.
                    let signal = strategy.on_tick(&tick);

                    let _ = strat_app_tx
                        .send(AppEvent::StrategyState {
                            fast_sma: strategy.fast_sma_value(),
                            slow_sma: strategy.slow_sma_value(),
                        })
                        .await;

                    order_mgr.update_unrealized_pnl(tick.price);
                    emit_rate_snapshot(&strat_app_tx, &order_mgr);

                    let enabled = *strat_enabled_rx.borrow();
                    if signal != Signal::Hold && enabled {
                        let _ = strat_app_tx
                            .send(AppEvent::StrategySignal(signal.clone()))
                            .await;
                        if let Err(e) = risk_eval_tx
                            .send((signal, active_profile.source_tag.clone()))
                            .await
                        {
                            tracing::error!(error = %e, "Failed to enqueue strategy signal");
                        }
                    }
                }
                Some(signal) = manual_order_rx.recv() => {
                    tracing::info!(signal = ?signal, "Manual order received");
                    let _ = strat_app_tx
                        .send(AppEvent::StrategySignal(signal.clone()))
                        .await;
                    if let Err(e) = risk_eval_tx.send((signal, "mnl".to_string())).await {
                        tracing::error!(error = %e, "Failed to enqueue manual signal");
                    }
                }
                Some((signal, source_tag)) = risk_eval_rx.recv() => {
                    match order_mgr.submit_order(signal, &source_tag).await {
                        Ok(Some(ref update)) => {
                            let _ = strat_app_tx
                                .send(AppEvent::OrderUpdate(update.clone()))
                                .await;
                            match order_mgr.refresh_order_history(ORDER_HISTORY_LIMIT).await {
                                Ok(history) => {
                                    let _ = strat_app_tx
                                        .send(AppEvent::OrderHistoryUpdate(history))
                                        .await;
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "Failed to refresh order history");
                                    let _ = strat_app_tx
                                        .send(AppEvent::LogMessage(format!(
                                            "[WARN] Order history refresh failed: {}",
                                            e
                                        )))
                                        .await;
                                }
                            }
                            if matches!(
                                update,
                                sandbox_quant::order_manager::OrderUpdate::Filled { .. }
                            ) {
                                let _ = strat_app_tx
                                    .send(AppEvent::BalanceUpdate(order_mgr.balances().clone()))
                                    .await;
                            }
                            emit_rate_snapshot(&strat_app_tx, &order_mgr);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::error!(error = %e, "Queued order submission failed");
                            let _ = strat_app_tx
                                .send(AppEvent::Error(e.to_string()))
                                .await;
                        }
                    }
                }
                _ = order_history_sync.tick() => {
                    match order_mgr.refresh_order_history(ORDER_HISTORY_LIMIT).await {
                        Ok(history) => {
                            let _ = strat_app_tx
                                .send(AppEvent::OrderHistoryUpdate(history))
                                .await;
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Periodic order history sync failed");
                            let _ = strat_app_tx
                                .send(AppEvent::LogMessage(format!(
                                    "[WARN] Periodic order history sync failed: {}",
                                    e
                                )))
                                .await;
                        }
                    }
                    emit_rate_snapshot(&strat_app_tx, &order_mgr);
                }
                _ = strat_symbol_rx.changed() => {
                    current_symbol = strat_symbol_rx.borrow().clone();
                    let (api_symbol, market) = parse_instrument_label(&current_symbol);
                    current_api_symbol = api_symbol.clone();
                    current_market = market;
                    order_mgr = OrderManager::new(
                        strat_rest.clone(),
                        &api_symbol,
                        current_market,
                        strat_config.strategy.order_amount_usdt,
                        &strat_config.risk,
                    );
                    worker_registry.unregister(&worker_id);
                    let (next_worker_tick_tx, next_worker_tick_rx) = mpsc::channel::<Tick>(256);
                    worker_id = strategy_worker_id(&active_profile, &current_api_symbol, current_market);
                    worker_registry.register(
                        worker_id.clone(),
                        current_api_symbol.clone(),
                        next_worker_tick_tx,
                    );
                    worker_tick_rx = next_worker_tick_rx;
                    strategy = MaCrossover::new(fast_period, slow_period, min_ticks);
                    let _ = strat_app_tx
                        .send(AppEvent::LogMessage(format!("Switched symbol to {}", current_symbol)))
                        .await;
                    match order_mgr.refresh_order_history(ORDER_HISTORY_LIMIT).await {
                        Ok(history) => {
                            let _ = strat_app_tx.send(AppEvent::OrderHistoryUpdate(history)).await;
                        }
                        Err(e) => {
                            let _ = strat_app_tx
                                .send(AppEvent::LogMessage(format!(
                                    "[WARN] Symbol switch history refresh failed: {}",
                                    e
                                )))
                                .await;
                        }
                    }
                    if current_market == MarketKind::Spot {
                        match order_mgr.refresh_balances().await {
                            Ok(balances) => {
                                let _ = strat_app_tx.send(AppEvent::BalanceUpdate(balances)).await;
                            }
                            Err(e) => {
                                let _ = strat_app_tx
                                    .send(AppEvent::LogMessage(format!(
                                        "[WARN] Balance refresh failed after symbol switch: {}",
                                        e
                                    )))
                                    .await;
                            }
                        }
                    }
                    emit_rate_snapshot(&strat_app_tx, &order_mgr);
                }
                _ = strategy_profile_rx.changed() => {
                    active_profile = strategy_profile_rx.borrow().clone();
                    (fast_period, slow_period, min_ticks) = active_profile.periods_tuple();
                    strategy = MaCrossover::new(fast_period, slow_period, min_ticks);
                    worker_registry.unregister(&worker_id);
                    let (next_worker_tick_tx, next_worker_tick_rx) = mpsc::channel::<Tick>(256);
                    worker_id = strategy_worker_id(&active_profile, &current_api_symbol, current_market);
                    worker_registry.register(
                        worker_id.clone(),
                        current_api_symbol.clone(),
                        next_worker_tick_tx,
                    );
                    worker_tick_rx = next_worker_tick_rx;
                    let _ = strat_app_tx
                        .send(AppEvent::LogMessage(format!(
                            "Strategy switched: {} ({}/{})",
                            active_profile.label,
                            fast_period,
                            slow_period
                        )))
                        .await;
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
    app_state.strategy_items = strategy_catalog.labels();

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
        // Draw
        terminal.draw(|frame| ui::render(frame, &app_state))?;

        // Handle input (non-blocking with timeout)
        if crossterm::event::poll(Duration::from_millis(config.ui.refresh_rate_ms))? {
            if let Event::Key(key) = crossterm::event::read()? {
                if app_state.symbol_selector_open {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('t') | KeyCode::Char('T') => {
                            app_state.symbol_selector_open = false;
                        }
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                            app_state.symbol_selector_index =
                                app_state.symbol_selector_index.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                            app_state.symbol_selector_index = (app_state.symbol_selector_index + 1)
                                .min(app_state.symbol_items.len().saturating_sub(1));
                        }
                        KeyCode::Enter => {
                            if let Some(next_symbol) =
                                app_state.symbol_items.get(app_state.symbol_selector_index)
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
                            app_state.symbol_selector_open = false;
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.strategy_selector_open {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('y') | KeyCode::Char('Y') => {
                            app_state.strategy_selector_open = false;
                        }
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                            app_state.strategy_selector_index =
                                app_state.strategy_selector_index.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                            app_state.strategy_selector_index = (app_state.strategy_selector_index
                                + 1)
                            .min(app_state.strategy_items.len().saturating_sub(1));
                        }
                        KeyCode::Enter => {
                            if let Some(next_profile) = strategy_catalog
                                .get(app_state.strategy_selector_index)
                                .cloned()
                            {
                                if next_profile != current_strategy_profile {
                                    current_strategy_profile = next_profile.clone();
                                    app_state.strategy_label = next_profile.label.clone();
                                    app_state.v2_state.focus.strategy_id =
                                        Some(next_profile.label.clone());
                                    app_state.strategy_items = strategy_catalog.labels();
                                    app_state.v2_grid_strategy_index = strategy_catalog
                                        .index_of_label(&next_profile.label)
                                        .unwrap_or(0);
                                    app_state.fast_sma = None;
                                    app_state.slow_sma = None;
                                    let _ = strategy_profile_tx.send(next_profile.clone());
                                    app_state.push_log(format!(
                                        "Strategy selected: {}",
                                        next_profile.label
                                    ));
                                    persist_strategy_session_state(
                                        &mut app_state,
                                        &strategy_catalog,
                                        &current_strategy_profile,
                                    );
                                }
                            }
                            app_state.strategy_selector_open = false;
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.account_popup_open {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Enter => {
                            app_state.account_popup_open = false;
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.history_popup_open {
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
                            app_state.history_popup_open = false;
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.focus_popup_open {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::Enter => {
                            app_state.focus_popup_open = false;
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.strategy_editor_open {
                    match key.code {
                        KeyCode::Esc => {
                            app_state.strategy_editor_open = false;
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
                                    app_state.strategy_editor_symbol_index = app_state
                                        .strategy_editor_symbol_index
                                        .saturating_sub(1)
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
                            if let Some(selected_symbol) = app_state
                                .symbol_items
                                .get(app_state.strategy_editor_symbol_index)
                                .cloned()
                            {
                                apply_symbol_selection(
                                    &selected_symbol,
                                    &mut current_symbol,
                                    &mut app_state,
                                    &ws_symbol_tx,
                                    &rest_client,
                                    &config,
                                    &app_tx,
                                );
                            }
                            let maybe_updated = strategy_catalog.fork_profile(
                                app_state.strategy_editor_index,
                                app_state.strategy_editor_fast,
                                app_state.strategy_editor_slow,
                                app_state.strategy_editor_cooldown,
                            );
                            if let Some(updated) = maybe_updated {
                                app_state.strategy_items = strategy_catalog.labels();
                                app_state.v2_grid_strategy_index = strategy_catalog
                                    .index_of_label(&updated.label)
                                    .unwrap_or(0);
                                if edited_profile
                                    .as_ref()
                                    .map(|p| p.source_tag.as_str())
                                    == Some(current_strategy_profile.source_tag.as_str())
                                {
                                    current_strategy_profile = updated.clone();
                                    app_state.strategy_label = updated.label.clone();
                                    app_state.v2_state.focus.strategy_id =
                                        Some(updated.label.clone());
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
                                    app_state.push_log(format!(
                                        "Strategy forked: {}",
                                        updated.label
                                    ));
                                }
                                persist_strategy_session_state(
                                    &mut app_state,
                                    &strategy_catalog,
                                    &current_strategy_profile,
                                );
                            }
                            app_state.strategy_editor_open = false;
                        }
                        _ => {}
                    }
                    continue;
                }
                if app_state.v2_grid_open {
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                            app_state.v2_grid_strategy_index =
                                app_state.v2_grid_strategy_index.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                            app_state.v2_grid_strategy_index = (app_state.v2_grid_strategy_index
                                + 1)
                            .min(app_state.strategy_items.len().saturating_sub(1));
                        }
                        KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                            app_state.v2_grid_symbol_index =
                                app_state.v2_grid_symbol_index.saturating_sub(1);
                        }
                        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                            app_state.v2_grid_symbol_index = (app_state.v2_grid_symbol_index + 1)
                                .min(app_state.symbol_items.len().saturating_sub(1));
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            let base_index = app_state
                                .v2_grid_strategy_index
                                .min(strategy_catalog.len().saturating_sub(1));
                            let created = strategy_catalog.add_custom_from_index(base_index);
                            app_state.strategy_items = strategy_catalog.labels();
                            app_state.v2_grid_strategy_index = strategy_catalog
                                .index_of_label(&created.label)
                                .unwrap_or(0);
                            app_state.strategy_editor_open = true;
                            app_state.strategy_editor_index = app_state.v2_grid_strategy_index;
                            app_state.strategy_editor_field = 0;
                            app_state.strategy_editor_symbol_index = app_state.v2_grid_symbol_index;
                            app_state.strategy_editor_fast = created.fast_period;
                            app_state.strategy_editor_slow = created.slow_period;
                            app_state.strategy_editor_cooldown = created.min_ticks_between_signals;
                            app_state.push_log(format!(
                                "Grid strategy registered: {}",
                                created.label
                            ));
                            persist_strategy_session_state(
                                &mut app_state,
                                &strategy_catalog,
                                &current_strategy_profile,
                            );
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            if let Some(selected_label) = app_state
                                .strategy_items
                                .get(app_state.v2_grid_strategy_index)
                                .cloned()
                            {
                                if let Some(idx) = strategy_catalog.index_of_label(&selected_label) {
                                    if let Some(profile) = strategy_catalog.get(idx).cloned() {
                                        app_state.strategy_editor_open = true;
                                        app_state.strategy_editor_index = idx;
                                        app_state.strategy_editor_field = 0;
                                        app_state.strategy_editor_symbol_index =
                                            app_state.v2_grid_symbol_index;
                                        app_state.strategy_editor_fast = profile.fast_period;
                                        app_state.strategy_editor_slow = profile.slow_period;
                                        app_state.strategy_editor_cooldown =
                                            profile.min_ticks_between_signals;
                                    }
                                }
                            }
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            if let Some(item) = app_state
                                .strategy_items
                                .get(app_state.v2_grid_strategy_index)
                                .cloned()
                            {
                                if let Some(selected_symbol) = app_state
                                    .symbol_items
                                    .get(app_state.v2_grid_symbol_index)
                                    .cloned()
                                {
                                    apply_symbol_selection(
                                        &selected_symbol,
                                        &mut current_symbol,
                                        &mut app_state,
                                        &ws_symbol_tx,
                                        &rest_client,
                                        &config,
                                        &app_tx,
                                    );
                                }
                                app_state.v2_state.focus.symbol = Some(app_state.symbol.clone());
                                app_state.v2_state.focus.strategy_id = Some(item.clone());
                                if let Some(next_profile) = strategy_catalog
                                    .index_of_label(&item)
                                    .and_then(|idx| strategy_catalog.get(idx).cloned())
                                {
                                    if next_profile != current_strategy_profile {
                                        current_strategy_profile = next_profile.clone();
                                        app_state.strategy_label = next_profile.label.clone();
                                        app_state.fast_sma = None;
                                        app_state.slow_sma = None;
                                        let _ = strategy_profile_tx.send(next_profile.clone());
                                        app_state.push_log(format!(
                                            "Strategy selected from grid: {}",
                                            next_profile.label
                                        ));
                                        persist_strategy_session_state(
                                            &mut app_state,
                                            &strategy_catalog,
                                            &current_strategy_profile,
                                        );
                                    }
                                }
                                app_state.v2_grid_open = false;
                                app_state.focus_popup_open = true;
                            }
                        }
                        KeyCode::Esc | KeyCode::Char('g') | KeyCode::Char('G') => {
                            app_state.v2_grid_open = false;
                        }
                        _ => {}
                    }
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        tracing::info!("User quit");
                        let _ = shutdown_tx.send(true);
                        break;
                    }
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        if !app_state.paused {
                            app_state.paused = true;
                            let _ = strategy_enabled_tx.send(false);
                            app_state.push_log("Strategy OFF".to_string());
                        }
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        if app_state.paused {
                            app_state.paused = false;
                            let _ = strategy_enabled_tx.send(true);
                            app_state.push_log("Strategy ON".to_string());
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
                        app_state.symbol_selector_index = app_state
                            .symbol_items
                            .iter()
                            .position(|s| s == &current_symbol)
                            .unwrap_or(0);
                        app_state.symbol_selector_open = true;
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        app_state.strategy_selector_index =
                            strategy_catalog
                                .index_of_label(&current_strategy_profile.label)
                                .unwrap_or(0);
                        app_state.strategy_selector_open = true;
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        app_state.account_popup_open = true;
                    }
                    KeyCode::Char('i') | KeyCode::Char('I') => {
                        app_state.refresh_history_rows();
                        app_state.history_popup_open = true;
                    }
                    KeyCode::Char('g') | KeyCode::Char('G') => {
                        app_state.v2_grid_symbol_index = app_state
                            .symbol_items
                            .iter()
                            .position(|item| item == &current_symbol)
                            .unwrap_or(0);
                        app_state.v2_grid_strategy_index = app_state
                            .strategy_items
                            .iter()
                            .position(|item| item == &app_state.strategy_label)
                            .unwrap_or(0);
                        app_state.v2_grid_open = true;
                    }
                    KeyCode::Char('f') | KeyCode::Char('F') => {
                        app_state.v2_state.focus.symbol = Some(app_state.symbol.clone());
                        app_state.v2_state.focus.strategy_id = Some(app_state.strategy_label.clone());
                        app_state.focus_popup_open = true;
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

    if let Err(e) = strategy_session::persist_strategy_session(
        &strategy_catalog,
        &current_strategy_profile.source_tag,
    ) {
        tracing::warn!(error = %e, "Failed to persist strategy session during shutdown");
    }

    ratatui::restore();
    tracing::info!("Shutdown complete");
    println!("Goodbye! Check sandbox-quant.log for details.");
    Ok(())
}
