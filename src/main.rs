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
use crate::config::Config;
use crate::event::AppEvent;
use crate::model::signal::Signal;
use crate::order_manager::OrderManager;
use crate::strategy::ma_crossover::MaCrossover;
use crate::ui::AppState;

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
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

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
            tracing::info!("Binance testnet ping OK");
            let _ = ping_app_tx
                .send(AppEvent::LogMessage(
                    "Binance testnet ping OK".to_string(),
                ))
                .await;
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to ping Binance testnet");
            let _ = ping_app_tx
                .send(AppEvent::LogMessage(format!(
                    "[ERR] Ping failed: {}",
                    e
                )))
                .await;
        }
    }

    // Fetch historical klines to pre-fill chart
    let historical_prices = match rest_client
        .get_klines(
            &config.binance.symbol,
            &config.binance.kline_interval,
            config.ui.price_history_len,
        )
        .await
    {
        Ok(prices) => {
            tracing::info!(count = prices.len(), "Fetched historical klines");
            let _ = app_tx
                .send(AppEvent::LogMessage(format!(
                    "Loaded {} historical klines",
                    prices.len()
                )))
                .await;
            prices
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
                .send(AppEvent::LogMessage(format!(
                    "[ERR] WS task failed: {}",
                    e
                )))
                .await;
        }
    });

    // Spawn strategy + order manager task
    let strat_app_tx = app_tx.clone();
    let strat_rest = rest_client.clone();
    let strat_config = config.clone();
    let mut strat_shutdown = shutdown_rx.clone();
    let strat_historical = historical_prices.clone();
    tokio::spawn(async move {
        let mut strategy = MaCrossover::new(
            strat_config.strategy.fast_period,
            strat_config.strategy.slow_period,
            strat_config.strategy.order_qty,
            strat_config.strategy.min_ticks_between_signals,
        );
        let mut order_mgr = OrderManager::new(strat_rest, &strat_config.binance.symbol);

        let _ = strat_app_tx
            .send(AppEvent::LogMessage(format!(
                "Strategy: MA({}/{}) qty={} cooldown={}",
                strat_config.strategy.fast_period,
                strat_config.strategy.slow_period,
                strat_config.strategy.order_qty,
                strat_config.strategy.min_ticks_between_signals,
            )))
            .await;

        // Warm up SMA indicators with historical kline prices (no orders)
        for price in &strat_historical {
            let tick = model::tick::Tick::from_price(*price);
            strategy.on_tick(&tick);
        }
        if !strat_historical.is_empty() {
            tracing::info!(
                count = strat_historical.len(),
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

                    if signal != Signal::Hold {
                        let _ = strat_app_tx
                            .send(AppEvent::StrategySignal(signal.clone()))
                            .await;

                        // Submit order
                        match order_mgr.submit_order(signal).await {
                            Ok(Some(update)) => {
                                let _ = strat_app_tx
                                    .send(AppEvent::OrderUpdate(update))
                                    .await;
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
    let mut app_state = AppState::new(&config.binance.symbol, config.ui.price_history_len);

    // Pre-fill chart with historical prices
    if !historical_prices.is_empty() {
        app_state.prices = historical_prices;
        if app_state.prices.len() > app_state.price_history_len {
            let excess = app_state.prices.len() - app_state.price_history_len;
            app_state.prices.drain(..excess);
        }
    }

    app_state.push_log(format!(
        "sandbox-quant started | {} | testnet",
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
                        app_state.paused = true;
                        app_state.push_log("Strategy PAUSED".to_string());
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        app_state.paused = false;
                        app_state.push_log("Strategy RESUMED".to_string());
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
