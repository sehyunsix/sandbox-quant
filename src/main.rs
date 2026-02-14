mod binance;
mod config;
mod error;
mod event;
mod indicator;
mod model;
mod order_manager;
mod strategy;
mod ui;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use tokio::sync::{mpsc, watch};

use crate::binance::rest::BinanceRestClient;
use crate::binance::ws::BinanceWsClient;
use crate::config::{parse_interval_ms, Config};
use crate::event::AppEvent;
use crate::model::signal::Signal;
use crate::order_manager::OrderManager;
use crate::strategy::ma_crossover::MaCrossover;
use crate::ui::AppState;

const ORDER_HISTORY_LIMIT: usize = 100;
const ORDER_HISTORY_SYNC_SECS: u64 = 5;

fn switch_timeframe(
    interval: &str,
    rest_client: &Arc<BinanceRestClient>,
    config: &Config,
    app_tx: &mpsc::Sender<AppEvent>,
) {
    let interval = interval.to_string();
    let rest = rest_client.clone();
    let symbol = config.binance.symbol.clone();
    let limit = config.ui.price_history_len;
    let tx = app_tx.clone();
    let interval_ms = match parse_interval_ms(&interval) {
        Ok(v) => v,
        Err(e) => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let _ = tx
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
        match rest.get_klines(&symbol, &iv, limit).await {
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
    let (tick_tx, mut tick_rx) = mpsc::channel::<model::tick::Tick>(256);
    let (manual_order_tx, mut manual_order_rx) = mpsc::channel::<Signal>(16);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (strategy_enabled_tx, strategy_enabled_rx) = watch::channel(true);

    // REST client
    let rest_client = Arc::new(BinanceRestClient::new(
        &config.binance.rest_base_url,
        &config.binance.api_key,
        &config.binance.api_secret,
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
    let historical_candles = match rest_client
        .get_klines(
            &config.binance.symbol,
            &config.binance.kline_interval,
            config.ui.price_history_len,
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
    let ws_streams = vec![format!("{}@trade", config.binance.symbol.to_lowercase())];
    let ws_client = BinanceWsClient::new(&config.binance.ws_base_url, ws_streams);
    let ws_tick_tx = tick_tx;
    // ^ Move tick_tx into WS task. This way, when WS task drops ws_tick_tx,
    //   the strategy task's tick_rx.recv() returns None → clean shutdown.
    let ws_app_tx = app_tx.clone();
    let ws_shutdown = shutdown_rx.clone();
    tokio::spawn(async move {
        if let Err(e) = ws_client
            .connect_and_run(ws_tick_tx, ws_app_tx.clone(), ws_shutdown)
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
    tokio::spawn(async move {
        let mut strategy = MaCrossover::new(
            strat_config.strategy.fast_period,
            strat_config.strategy.slow_period,
            strat_config.strategy.min_ticks_between_signals,
        );
        let mut order_mgr = OrderManager::new(
            strat_rest,
            &strat_config.binance.symbol,
            strat_config.strategy.order_amount_usdt,
        );
        let mut order_history_sync =
            tokio::time::interval(Duration::from_secs(ORDER_HISTORY_SYNC_SECS));

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

        let _ = strat_app_tx
            .send(AppEvent::LogMessage(format!(
                "Strategy: MA({}/{}) usdt={} cooldown={}",
                strat_config.strategy.fast_period,
                strat_config.strategy.slow_period,
                strat_config.strategy.order_amount_usdt,
                strat_config.strategy.min_ticks_between_signals,
            )))
            .await;

        // Warm up SMA indicators with historical kline close prices (no orders)
        for price in &strat_historical_closes {
            let tick = model::tick::Tick::from_price(*price);
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

                    // Always run strategy to keep SMA state updated, but skip orders if paused
                    let signal = strategy.on_tick(&tick);

                    // Send SMA state to UI on every tick
                    let _ = strat_app_tx
                        .send(AppEvent::StrategyState {
                            fast_sma: strategy.fast_sma_value(),
                            slow_sma: strategy.slow_sma_value(),
                        })
                        .await;

                    // Update unrealized PnL
                    order_mgr.update_unrealized_pnl(tick.price);

                    // Only submit strategy orders when enabled
                    let enabled = *strat_enabled_rx.borrow();
                    if signal != Signal::Hold && enabled {
                        let _ = strat_app_tx
                            .send(AppEvent::StrategySignal(signal.clone()))
                            .await;

                        // Submit order
                        match order_mgr.submit_order(signal).await {
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
                                    }
                                }
                                // Send updated balances to UI after fill
                                if matches!(update, crate::order_manager::OrderUpdate::Filled { .. }) {
                                    let _ = strat_app_tx
                                        .send(AppEvent::BalanceUpdate(order_mgr.balances().clone()))
                                        .await;
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                tracing::error!(error = %e, "Order submission failed");
                                let _ = strat_app_tx
                                    .send(AppEvent::Error(e.to_string()))
                                    .await;
                            }
                        }
                    }
                }
                Some(signal) = manual_order_rx.recv() => {
                    tracing::info!(signal = ?signal, "Manual order received");
                    let _ = strat_app_tx
                        .send(AppEvent::StrategySignal(signal.clone()))
                        .await;

                    match order_mgr.submit_order(signal).await {
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
                                }
                            }
                            if matches!(update, crate::order_manager::OrderUpdate::Filled { .. }) {
                                let _ = strat_app_tx
                                    .send(AppEvent::BalanceUpdate(order_mgr.balances().clone()))
                                    .await;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::error!(error = %e, "Manual order submission failed");
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
                        }
                    }
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
    let candle_interval_ms = config.binance.kline_interval_ms()?;
    let mut app_state = AppState::new(
        &config.binance.symbol,
        config.ui.price_history_len,
        candle_interval_ms,
        &config.binance.kline_interval,
    );

    // Pre-fill chart with historical candles
    if !historical_candles.is_empty() {
        app_state.candles = historical_candles;
        if app_state.candles.len() > app_state.price_history_len {
            let excess = app_state.candles.len() - app_state.price_history_len;
            app_state.candles.drain(..excess);
        }
    }

    app_state.push_log(format!(
        "sandbox-quant started | {} | demo",
        config.binance.symbol
    ));

    loop {
        // Draw
        terminal.draw(|frame| ui::render(frame, &app_state))?;

        // Handle input (non-blocking with timeout)
        if crossterm::event::poll(Duration::from_millis(config.ui.refresh_rate_ms))? {
            if let Event::Key(key) = crossterm::event::read()? {
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
                    KeyCode::Char('1') => {
                        switch_timeframe("1m", &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('h') | KeyCode::Char('H') => {
                        switch_timeframe("1h", &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        switch_timeframe("1d", &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('w') | KeyCode::Char('W') => {
                        switch_timeframe("1w", &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        switch_timeframe("1M", &rest_client, &config, &app_tx);
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

    ratatui::restore();
    tracing::info!("Shutdown complete");
    println!("Goodbye! Check sandbox-quant.log for details.");
    Ok(())
}
