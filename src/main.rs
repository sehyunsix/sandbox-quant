mod alpaca;
mod binance;
mod config;
mod error;
mod event;
mod indicator;
mod model;
mod order_manager;
mod strategy;
mod strategy_stats;
mod ui;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::alpaca::rest::AlpacaRestClient;
use crate::binance::rest::BinanceRestClient;
use crate::binance::ws::BinanceWsClient;
use crate::config::{
    parse_interval_ms, AlpacaAssetClass, Broker, Config, StrategyPreset, TradingProduct,
};
use crate::event::AppEvent;
use crate::model::order::{Fill, OrderSide};
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_manager::OrderManager;
use crate::strategy::ma_crossover::MaCrossover;
use crate::strategy_stats::StrategyStatsStore;
use crate::ui::AppState;

const ORDER_HISTORY_PAGE_SIZE: usize = 1000;
const ORDER_HISTORY_SYNC_SECS: u64 = 5;
const STRATEGY_STATS_DB_PATH: &str = "data/strategy_stats.sqlite";
const PRODUCT_SELECTOR_COUNT: usize = 4;
const ALPACA_PRODUCT_SELECTOR_COUNT: usize = 3;
const STRATEGY_SELECTOR_COUNT: usize = 3;

fn product_from_selector_index(index: usize) -> TradingProduct {
    TradingProduct::ALL[index.min(PRODUCT_SELECTOR_COUNT - 1)]
}

fn selector_index_from_product_label(product_label: &str) -> usize {
    TradingProduct::ALL
        .iter()
        .position(|p| p.product_label() == product_label)
        .unwrap_or(0)
}

fn strategy_from_selector_index(index: usize) -> StrategyPreset {
    StrategyPreset::ALL[index.min(STRATEGY_SELECTOR_COUNT - 1)]
}

fn selector_index_from_strategy_label(strategy_label: &str) -> usize {
    StrategyPreset::ALL
        .iter()
        .position(|s| s.display_label() == strategy_label)
        .unwrap_or(0)
}

fn strategy_label_for_signal(is_manual: bool, active_preset: StrategyPreset) -> String {
    if is_manual {
        "MANUAL".to_string()
    } else {
        active_preset.display_label().to_string()
    }
}

fn alpaca_asset_from_selector_index(index: usize) -> AlpacaAssetClass {
    match index.min(ALPACA_PRODUCT_SELECTOR_COUNT - 1) {
        0 => AlpacaAssetClass::UsEquity,
        1 => AlpacaAssetClass::UsOption,
        _ => AlpacaAssetClass::UsFuture,
    }
}

fn selector_index_from_alpaca_asset_class(asset_class: AlpacaAssetClass) -> usize {
    match asset_class {
        AlpacaAssetClass::UsEquity => 0,
        AlpacaAssetClass::UsOption => 1,
        AlpacaAssetClass::UsFuture => 2,
    }
}

fn select_broker_at_start(default_broker: Broker) -> Result<Broker> {
    let mut terminal = ratatui::init();
    let mut selected = match default_broker {
        Broker::Binance => 0usize,
        Broker::Alpaca => 1usize,
    };
    let options = [Broker::Binance, Broker::Alpaca];

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let popup = Rect {
                x: area.x + area.width.saturating_sub(44) / 2,
                y: area.y + area.height.saturating_sub(10) / 2,
                width: 44.min(area.width),
                height: 10.min(area.height),
            };
            frame.render_widget(Clear, popup);
            let block = Block::default()
                .title(" Select Broker ")
                .borders(Borders::ALL);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);

            for (idx, broker) in options.iter().enumerate() {
                let y = inner.y + 1 + idx as u16;
                let focused = idx == selected;
                let marker = if focused { "▶" } else { " " };
                let label = match broker {
                    Broker::Binance => "Binance",
                    Broker::Alpaca => "Alpaca",
                };
                let style = if focused {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                frame.render_widget(
                    Paragraph::new(format!("{} {}", marker, label)).style(style),
                    Rect {
                        x: inner.x + 1,
                        y,
                        width: inner.width.saturating_sub(2),
                        height: 1,
                    },
                );
            }

            frame.render_widget(
                Paragraph::new("Use Up/Down + Enter  |  Esc = default"),
                Rect {
                    x: inner.x + 1,
                    y: inner.y + inner.height.saturating_sub(2),
                    width: inner.width.saturating_sub(2),
                    height: 1,
                },
            );
        })?;

        if crossterm::event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = crossterm::event::read()? {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                        selected = (selected + 1).min(options.len().saturating_sub(1));
                    }
                    KeyCode::Enter => {
                        ratatui::restore();
                        return Ok(options[selected]);
                    }
                    KeyCode::Esc => {
                        ratatui::restore();
                        return Ok(default_broker);
                    }
                    _ => {}
                }
            }
        }
    }
}

fn switch_timeframe(
    interval: &str,
    symbol: &str,
    rest_client: &Arc<BinanceRestClient>,
    config: &Config,
    app_tx: &mpsc::Sender<AppEvent>,
) {
    let interval = interval.to_string();
    let interval_ms = match parse_interval_ms(&interval) {
        Ok(ms) => ms,
        Err(e) => {
            let tx = app_tx.clone();
            tokio::spawn(async move {
                let _ = tx
                    .send(AppEvent::Error(format!(
                        "Invalid timeframe interval '{}': {}",
                        interval, e
                    )))
                    .await;
            });
            return;
        }
    };
    let rest = rest_client.clone();
    let symbol = symbol.to_string();
    let limit = config.ui.price_history_len;
    let tx = app_tx.clone();
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

fn switch_timeframe_alpaca(
    interval: &str,
    symbol: &str,
    asset_class: AlpacaAssetClass,
    rest_client: &Arc<AlpacaRestClient>,
    config: &Config,
    app_tx: &mpsc::Sender<AppEvent>,
) {
    let interval = interval.to_string();
    let interval_ms = match parse_interval_ms(&interval) {
        Ok(ms) => ms,
        Err(e) => {
            let tx = app_tx.clone();
            tokio::spawn(async move {
                let _ = tx
                    .send(AppEvent::Error(format!(
                        "Invalid timeframe interval '{}': {}",
                        interval, e
                    )))
                    .await;
            });
            return;
        }
    };
    let rest = rest_client.clone();
    let symbol = symbol.to_string();
    let limit = config.ui.price_history_len;
    let tx = app_tx.clone();
    let iv = interval.clone();
    tokio::spawn(async move {
        match rest.get_klines(&symbol, &iv, limit, asset_class).await {
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
                    .send(AppEvent::Error(format!("Alpaca kline fetch failed: {}", e)))
                    .await;
            }
        }
    });
}

fn select_product_in_tui(
    app_state: &mut AppState,
    strategy_enabled_tx: &watch::Sender<bool>,
    ws_symbol_tx: &watch::Sender<String>,
    product: TradingProduct,
) {
    app_state.symbol = product.symbol().to_string();
    app_state.product_label = product.product_label().to_string();
    app_state.candles.clear();
    app_state.current_candle = None;
    app_state.fill_markers.clear();
    if !app_state.paused {
        app_state.paused = true;
        let _ = strategy_enabled_tx.send(false);
    }
    let _ = ws_symbol_tx.send(app_state.symbol.clone());
    app_state.push_log(format!(
        "Product switched to {} | Strategy paused",
        app_state.product_label
    ));
}

fn enqueue_manual_order(
    app_state: &mut AppState,
    manual_order_tx: &mpsc::Sender<Signal>,
    signal: Signal,
    success_msg: &str,
    action: &str,
) {
    match manual_order_tx.try_send(signal) {
        Ok(()) => app_state.push_log(success_msg.to_string()),
        Err(TrySendError::Full(_)) => app_state.push_log(format!(
            "[WARN] Manual {} dropped: order queue is full",
            action
        )),
        Err(TrySendError::Closed(_)) => app_state.push_log(format!(
            "[ERR] Manual {} failed: order queue is closed",
            action
        )),
    }
}

async fn run_alpaca(config: &Config) -> Result<()> {
    let (app_tx, mut app_rx) = mpsc::channel::<AppEvent>(256);
    let (tick_tx, mut tick_rx) = mpsc::channel::<model::tick::Tick>(256);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (strategy_enabled_tx, strategy_enabled_rx) = watch::channel(true);
    let (alpaca_asset_tx, mut alpaca_asset_rx) = watch::channel(config.alpaca.asset_class);
    let alpaca_symbols = config.alpaca.tradable_symbols();
    let mut current_symbol_index = 0usize;
    let initial_symbol = alpaca_symbols
        .get(current_symbol_index)
        .cloned()
        .unwrap_or_else(|| config.alpaca.symbol.to_ascii_uppercase());
    let (alpaca_symbol_tx, mut alpaca_symbol_rx) = watch::channel(initial_symbol.clone());
    let order_notional = config.strategy.order_amount_usdt;

    let alpaca_rest = Arc::new(AlpacaRestClient::new(
        &config.alpaca.trading_base_url,
        &config.alpaca.data_base_url,
        &config.alpaca.api_key,
        &config.alpaca.api_secret,
        &config.alpaca.normalized_option_snapshot_feeds(),
    )?);

    let ping_app_tx = app_tx.clone();
    match alpaca_rest.ping().await {
        Ok(()) => {
            let _ = ping_app_tx
                .send(AppEvent::LogMessage("Alpaca paper ping OK".to_string()))
                .await;
        }
        Err(e) => {
            let _ = ping_app_tx
                .send(AppEvent::LogMessage(format!(
                    "[ERR] Alpaca ping failed: {}",
                    e
                )))
                .await;
        }
    }

    let mut initial_asset_class = config.alpaca.asset_class;
    if matches!(initial_asset_class, AlpacaAssetClass::UsFuture) {
        initial_asset_class = AlpacaAssetClass::UsEquity;
        let _ = app_tx
            .send(AppEvent::LogMessage(
                "[WARN] US FUTURE is not supported yet. Falling back to US EQUITY".to_string(),
            ))
            .await;
    }

    let historical_candles = match alpaca_rest
        .get_klines(
            &initial_symbol,
            &config.alpaca.kline_interval,
            config.ui.price_history_len,
            initial_asset_class,
        )
        .await
    {
        Ok(candles) => candles,
        Err(e) => {
            let _ = app_tx
                .send(AppEvent::LogMessage(format!(
                    "[WARN] Alpaca kline fetch failed: {}",
                    e
                )))
                .await;
            Vec::new()
        }
    };

    let ws_app_tx = app_tx.clone();
    let mut ws_shutdown = shutdown_rx.clone();
    let rest_for_ticks = alpaca_rest.clone();
    tokio::spawn(async move {
        let _ = ws_app_tx
            .send(AppEvent::WsStatus(
                crate::event::WsConnectionStatus::Connected,
            ))
            .await;
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        let mut last_ts = 0_u64;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let current_asset = *alpaca_asset_rx.borrow();
                    let current_symbol = alpaca_symbol_rx.borrow().clone();
                    match rest_for_ticks.get_latest_trade(&current_symbol, current_asset).await {
                        Ok(Some(tick)) => {
                            let _ = ws_app_tx.send(AppEvent::DataHeartbeat).await;
                            if tick.timestamp_ms > last_ts {
                                last_ts = tick.timestamp_ms;
                                let _ = tick_tx.send(tick).await;
                            }
                        }
                        Ok(None) => {
                            let _ = ws_app_tx.send(AppEvent::DataHeartbeat).await;
                        }
                        Err(e) => {
                            let _ = ws_app_tx
                                .send(AppEvent::LogMessage(format!("[WARN] Alpaca latest trade failed: {}", e)))
                                .await;
                        }
                    }

                    if matches!(current_asset, AlpacaAssetClass::UsOption) {
                        match rest_for_ticks.get_option_chain_snapshot(&current_symbol, 40).await {
                            Ok(chain) => {
                                let _ = ws_app_tx
                                    .send(AppEvent::OptionChainUpdate(chain))
                                    .await;
                            }
                            Err(e) => {
                                let _ = ws_app_tx
                                    .send(AppEvent::LogMessage(format!(
                                        "[WARN] Alpaca option snapshot failed: {}",
                                        e
                                    )))
                                    .await;
                            }
                        }
                    } else {
                        let _ = ws_app_tx.send(AppEvent::OptionChainUpdate(None)).await;
                    }
                }
                _ = ws_shutdown.changed() => break,
                _ = alpaca_symbol_rx.changed() => {
                    // Reset de-dup cursor so the first tick from a newly selected symbol is not skipped.
                    last_ts = 0;
                }
                _ = alpaca_asset_rx.changed() => {}
            }
        }
    });

    let strat_app_tx = app_tx.clone();
    let mut strat_shutdown = shutdown_rx.clone();
    let strat_historical_closes: Vec<f64> = historical_candles.iter().map(|c| c.close).collect();
    let strat_cfg = config.strategy.clone();
    tokio::spawn(async move {
        let mut strategy = MaCrossover::new(
            strat_cfg.fast_period,
            strat_cfg.slow_period,
            strat_cfg.min_ticks_between_signals,
        );
        for price in &strat_historical_closes {
            strategy.on_tick(&model::tick::Tick::from_price(*price));
        }
        loop {
            tokio::select! {
                result = tick_rx.recv() => {
                    let tick = match result {
                        Some(t) => t,
                        None => break,
                    };
                    let _ = strat_app_tx.send(AppEvent::MarketTick(tick.clone())).await;
                    let signal = strategy.on_tick(&tick);
                    let _ = strat_app_tx.send(AppEvent::StrategyState {
                        fast_sma: strategy.fast_sma_value(),
                        slow_sma: strategy.slow_sma_value(),
                    }).await;
                    if signal != Signal::Hold && *strategy_enabled_rx.borrow() {
                        let _ = strat_app_tx.send(AppEvent::StrategySignal(signal)).await;
                        let _ = strat_app_tx.send(AppEvent::LogMessage(
                            "[WARN] Alpaca order execution is not implemented yet".to_string()
                        )).await;
                    }
                }
                _ = strat_shutdown.changed() => break,
            }
        }
    });

    let ctrl_c_shutdown = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = ctrl_c_shutdown.send(true);
    });

    let mut terminal = ratatui::init();
    let candle_interval_ms = config.alpaca.kline_interval_ms()?;
    let mut app_state = AppState::new(
        &initial_symbol,
        initial_asset_class.label(),
        StrategyPreset::ConfigMa.display_label(),
        config.ui.price_history_len,
        candle_interval_ms,
        &config.alpaca.kline_interval,
        config.strategy.fast_period,
        config.strategy.slow_period,
    );
    app_state.product_selector_items = vec![
        AlpacaAssetClass::UsEquity.label().to_string(),
        AlpacaAssetClass::UsOption.label().to_string(),
        format!("{} (N/A)", AlpacaAssetClass::UsFuture.label()),
    ];
    app_state.product_selector_index = selector_index_from_alpaca_asset_class(initial_asset_class);
    let mut current_asset_class = initial_asset_class;
    if !historical_candles.is_empty() {
        app_state.candles = historical_candles;
        if app_state.candles.len() > app_state.price_history_len {
            let excess = app_state.candles.len() - app_state.price_history_len;
            app_state.candles.drain(..excess);
        }
    }
    app_state.push_log(format!(
        "sandbox-quant started | {} {} | Alpaca paper",
        initial_symbol,
        config.alpaca.asset_class.label()
    ));

    loop {
        terminal.draw(|frame| ui::render(frame, &app_state))?;

        if crossterm::event::poll(Duration::from_millis(config.ui.refresh_rate_ms))? {
            if let Event::Key(key) = crossterm::event::read()? {
                if app_state.product_selector_open {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('t') | KeyCode::Char('T') => {
                            app_state.product_selector_open = false;
                        }
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                            app_state.product_selector_index =
                                app_state.product_selector_index.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                            app_state.product_selector_index = (app_state.product_selector_index
                                + 1)
                            .min(ALPACA_PRODUCT_SELECTOR_COUNT - 1);
                        }
                        KeyCode::Enter => {
                            let selected_asset =
                                alpaca_asset_from_selector_index(app_state.product_selector_index);
                            if matches!(selected_asset, AlpacaAssetClass::UsFuture) {
                                app_state.push_log(
                                    "[WARN] US FUTURE is not supported yet. Select US EQUITY or US OPTION"
                                        .to_string(),
                                );
                                app_state.product_selector_open = false;
                                continue;
                            }
                            current_asset_class = selected_asset;
                            let _ = alpaca_asset_tx.send(current_asset_class);
                            app_state.product_label = current_asset_class.label().to_string();
                            app_state.candles.clear();
                            app_state.current_candle = None;
                            app_state.option_chain = None;
                            app_state.fill_markers.clear();
                            switch_timeframe_alpaca(
                                &app_state.timeframe,
                                &app_state.symbol,
                                current_asset_class,
                                &alpaca_rest,
                                config,
                                &app_tx,
                            );
                            app_state.push_log(format!(
                                "Product switched to {}",
                                app_state.product_label
                            ));
                            app_state.product_selector_open = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        let _ = shutdown_tx.send(true);
                        break;
                    }
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        if app_state.paused {
                            app_state.paused = false;
                            let _ = strategy_enabled_tx.send(true);
                            app_state.push_log("Strategy ON".to_string());
                        } else {
                            app_state.paused = true;
                            let _ = strategy_enabled_tx.send(false);
                            app_state.push_log("Strategy OFF".to_string());
                        }
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        switch_timeframe_alpaca(
                            &app_state.timeframe,
                            &app_state.symbol,
                            current_asset_class,
                            &alpaca_rest,
                            config,
                            &app_tx,
                        );
                        app_state.push_log("Refreshed chart data".to_string());
                    }
                    KeyCode::Char('1') => {
                        switch_timeframe_alpaca(
                            "1m",
                            &app_state.symbol,
                            current_asset_class,
                            &alpaca_rest,
                            config,
                            &app_tx,
                        );
                    }
                    KeyCode::Char('0') => {
                        switch_timeframe_alpaca(
                            "1s",
                            &app_state.symbol,
                            current_asset_class,
                            &alpaca_rest,
                            config,
                            &app_tx,
                        );
                    }
                    KeyCode::Char('h') | KeyCode::Char('H') => {
                        switch_timeframe_alpaca(
                            "1h",
                            &app_state.symbol,
                            current_asset_class,
                            &alpaca_rest,
                            config,
                            &app_tx,
                        );
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        switch_timeframe_alpaca(
                            "1d",
                            &app_state.symbol,
                            current_asset_class,
                            &alpaca_rest,
                            config,
                            &app_tx,
                        );
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') => {
                        app_state.product_selector_index =
                            selector_index_from_alpaca_asset_class(current_asset_class);
                        app_state.product_selector_open = true;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') => {
                        if alpaca_symbols.len() <= 1 {
                            app_state.push_log(
                                "[WARN] No additional Alpaca symbols configured".to_string(),
                            );
                            continue;
                        }
                        current_symbol_index = (current_symbol_index + 1) % alpaca_symbols.len();
                        let next_symbol = alpaca_symbols[current_symbol_index].clone();
                        app_state.symbol = next_symbol.clone();
                        app_state.position = Position::new(next_symbol.clone());
                        app_state.candles.clear();
                        app_state.current_candle = None;
                        app_state.option_chain = None;
                        app_state.fill_markers.clear();
                        let _ = alpaca_symbol_tx.send(next_symbol.clone());
                        switch_timeframe_alpaca(
                            &app_state.timeframe,
                            &next_symbol,
                            current_asset_class,
                            &alpaca_rest,
                            config,
                            &app_tx,
                        );
                        app_state.push_log(format!("Alpaca symbol switched to {}", next_symbol));
                    }
                    KeyCode::Char('v') | KeyCode::Char('V') => {
                        if alpaca_symbols.len() <= 1 {
                            app_state.push_log(
                                "[WARN] No additional Alpaca symbols configured".to_string(),
                            );
                            continue;
                        }
                        current_symbol_index = (current_symbol_index + alpaca_symbols.len() - 1)
                            % alpaca_symbols.len();
                        let prev_symbol = alpaca_symbols[current_symbol_index].clone();
                        app_state.symbol = prev_symbol.clone();
                        app_state.position = Position::new(prev_symbol.clone());
                        app_state.candles.clear();
                        app_state.current_candle = None;
                        app_state.option_chain = None;
                        app_state.fill_markers.clear();
                        let _ = alpaca_symbol_tx.send(prev_symbol.clone());
                        switch_timeframe_alpaca(
                            &app_state.timeframe,
                            &prev_symbol,
                            current_asset_class,
                            &alpaca_rest,
                            config,
                            &app_tx,
                        );
                        app_state.push_log(format!("Alpaca symbol switched to {}", prev_symbol));
                    }
                    KeyCode::Char('b') | KeyCode::Char('B') => {
                        let rest = alpaca_rest.clone();
                        let tx = app_tx.clone();
                        let symbol = app_state.symbol.clone();
                        let asset_class = current_asset_class;
                        match current_asset_class {
                            AlpacaAssetClass::UsFuture => {
                                app_state.push_log(
                                    "[WARN] US FUTURE manual order is not supported yet"
                                        .to_string(),
                                );
                            }
                            AlpacaAssetClass::UsOption => {
                                tokio::spawn(async move {
                                    let resolved = match rest
                                        .resolve_symbol_for_asset(&symbol, asset_class)
                                        .await
                                    {
                                        Ok(s) => s,
                                        Err(e) => {
                                            let _ = tx
                                                .send(AppEvent::Error(format!(
                                                    "Alpaca option symbol resolve failed: {}",
                                                    e
                                                )))
                                                .await;
                                            return;
                                        }
                                    };
                                    match rest.place_market_order_qty(&resolved, "buy", 1.0).await {
                                        Ok(ack) => {
                                            let avg = ack.filled_avg_price.unwrap_or(0.0);
                                            let _ = tx
                                                .send(AppEvent::LogMessage(format!(
                                                    "Alpaca BUY {} id={} qty={} avg={}",
                                                    ack.status,
                                                    ack.id,
                                                    ack.qty
                                                        .map(|q| format!("{:.4}", q))
                                                        .unwrap_or_else(|| "-".to_string()),
                                                    ack.filled_avg_price
                                                        .map(|p| format!("{:.4}", p))
                                                        .unwrap_or_else(|| "-".to_string())
                                                )))
                                                .await;
                                            if avg > 0.0 {
                                                let _ = tx
                                                    .send(AppEvent::OrderUpdate(
                                                        crate::order_manager::OrderUpdate::Filled {
                                                            client_order_id: format!("alpaca-{}", ack.id),
                                                            side: OrderSide::Buy,
                                                            fills: Vec::new(),
                                                            avg_price: avg,
                                                        },
                                                    ))
                                                    .await;
                                            }
                                        }
                                        Err(e) => {
                                            let _ = tx
                                                .send(AppEvent::Error(format!(
                                                    "Alpaca BUY failed: {}",
                                                    e
                                                )))
                                                .await;
                                        }
                                    }
                                });
                            }
                            AlpacaAssetClass::UsEquity => {
                                tokio::spawn(async move {
                                    match rest
                                        .place_market_order_notional(&symbol, "buy", order_notional)
                                        .await
                                    {
                                        Ok(ack) => {
                                            let avg = ack.filled_avg_price.unwrap_or(0.0);
                                            let _ = tx
                                                .send(AppEvent::LogMessage(format!(
                                                    "Alpaca BUY {} id={} qty={} avg={}",
                                                    ack.status,
                                                    ack.id,
                                                    ack.qty
                                                        .map(|q| format!("{:.4}", q))
                                                        .unwrap_or_else(|| "-".to_string()),
                                                    ack.filled_avg_price
                                                        .map(|p| format!("{:.4}", p))
                                                        .unwrap_or_else(|| "-".to_string())
                                                )))
                                                .await;
                                            if avg > 0.0 {
                                                let _ = tx
                                                    .send(AppEvent::OrderUpdate(
                                                        crate::order_manager::OrderUpdate::Filled {
                                                            client_order_id: format!("alpaca-{}", ack.id),
                                                            side: OrderSide::Buy,
                                                            fills: Vec::new(),
                                                            avg_price: avg,
                                                        },
                                                    ))
                                                    .await;
                                            }
                                        }
                                        Err(e) => {
                                            let _ = tx
                                                .send(AppEvent::Error(format!(
                                                    "Alpaca BUY failed: {}",
                                                    e
                                                )))
                                                .await;
                                        }
                                    }
                                });
                            }
                        }
                    }
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        let rest = alpaca_rest.clone();
                        let tx = app_tx.clone();
                        let symbol = app_state.symbol.clone();
                        let asset_class = current_asset_class;
                        match current_asset_class {
                            AlpacaAssetClass::UsFuture => {
                                app_state.push_log(
                                    "[WARN] US FUTURE manual order is not supported yet"
                                        .to_string(),
                                );
                            }
                            _ => {
                                tokio::spawn(async move {
                                    let resolved = match rest
                                        .resolve_symbol_for_asset(&symbol, asset_class)
                                        .await
                                    {
                                        Ok(s) => s,
                                        Err(e) => {
                                            let _ = tx
                                                .send(AppEvent::Error(format!(
                                                    "Alpaca symbol resolve failed: {}",
                                                    e
                                                )))
                                                .await;
                                            return;
                                        }
                                    };
                                    match rest.get_position_qty(&resolved).await {
                                        Ok(qty) if qty > 0.0 => {
                                            match rest
                                                .place_market_order_qty(&resolved, "sell", qty)
                                                .await
                                            {
                                                Ok(ack) => {
                                                    let avg = ack.filled_avg_price.unwrap_or(0.0);
                                                    let _ = tx
                                                        .send(AppEvent::LogMessage(format!(
                                                            "Alpaca SELL {} id={} qty={} avg={}",
                                                            ack.status,
                                                            ack.id,
                                                            ack.qty
                                                                .map(|q| format!("{:.4}", q))
                                                                .unwrap_or_else(|| "-".to_string()),
                                                            ack.filled_avg_price
                                                                .map(|p| format!("{:.4}", p))
                                                                .unwrap_or_else(|| "-".to_string())
                                                        )))
                                                        .await;
                                                    if avg > 0.0 {
                                                        let _ = tx
                                                            .send(AppEvent::OrderUpdate(
                                                                crate::order_manager::OrderUpdate::Filled {
                                                                    client_order_id: format!("alpaca-{}", ack.id),
                                                                    side: OrderSide::Sell,
                                                                    fills: Vec::new(),
                                                                    avg_price: avg,
                                                                },
                                                            ))
                                                            .await;
                                                    }
                                                }
                                                Err(e) => {
                                                    let _ = tx
                                                        .send(AppEvent::Error(format!(
                                                            "Alpaca SELL failed: {}",
                                                            e
                                                        )))
                                                        .await;
                                                }
                                            }
                                        }
                                        Ok(_) => {
                                            let _ = tx
                                                .send(AppEvent::LogMessage(
                                                    "[WARN] Alpaca SELL skipped: no open position"
                                                        .to_string(),
                                                ))
                                                .await;
                                        }
                                        Err(e) => {
                                            let _ = tx
                                                .send(AppEvent::Error(format!(
                                                    "Alpaca position fetch failed: {}",
                                                    e
                                                )))
                                                .await;
                                        }
                                    }
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        while let Ok(evt) = app_rx.try_recv() {
            app_state.apply(evt);
        }
        if *shutdown_rx.borrow() {
            break;
        }
    }

    ratatui::restore();
    tracing::info!("Shutdown complete");
    Ok(())
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
            std::process::exit(1);
        }
    };
    let selected_broker = match select_broker_at_start(config.broker) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read broker selection: {:#}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = config.validate_for_broker(selected_broker) {
        eprintln!("Broker credential validation failed: {:#}", e);
        std::process::exit(1);
    }

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

    match selected_broker {
        Broker::Binance => {
            tracing::info!(
                symbol = %config.binance.selected_symbol(),
                market = %config.binance.market_label(),
                product = %config.binance.product_label(),
                rest_url = %config.binance.rest_base_url,
                ws_url = %config.binance.ws_base_url,
                "Starting sandbox-quant"
            );
        }
        Broker::Alpaca => {
            tracing::info!(
                symbol = %config.alpaca.symbol,
                asset_class = %config.alpaca.asset_class.label(),
                trading_url = %config.alpaca.trading_base_url,
                data_url = %config.alpaca.data_base_url,
                "Starting sandbox-quant (Alpaca)"
            );
            if matches!(config.alpaca.asset_class, AlpacaAssetClass::UsFuture) {
                tracing::warn!("Alpaca futures are not supported by this app");
            }
            return run_alpaca(&config).await;
        }
    }

    // Channels
    let (app_tx, mut app_rx) = mpsc::channel::<AppEvent>(256);
    let (tick_tx, mut tick_rx) = mpsc::channel::<model::tick::Tick>(256);
    let (manual_order_tx, mut manual_order_rx) = mpsc::channel::<Signal>(16);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (strategy_enabled_tx, strategy_enabled_rx) = watch::channel(true);
    let (strategy_preset_tx, strategy_preset_rx) = watch::channel(StrategyPreset::ConfigMa);
    let (ws_symbol_tx, ws_symbol_rx) = watch::channel(config.binance.selected_symbol().to_string());

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
            config.binance.selected_symbol(),
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
    let ws_client = BinanceWsClient::new(&config.binance.ws_base_url);
    let ws_tick_tx = tick_tx;
    // ^ Move tick_tx into WS task. This way, when WS task drops ws_tick_tx,
    //   the strategy task's tick_rx.recv() returns None → clean shutdown.
    let ws_app_tx = app_tx.clone();
    let ws_shutdown = shutdown_rx.clone();
    tokio::spawn(async move {
        if let Err(e) = ws_client
            .connect_and_run(
                ws_tick_tx,
                ws_app_tx.clone(),
                ws_symbol_rx.clone(),
                ws_shutdown,
            )
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
    let mut strat_preset_rx = strategy_preset_rx;
    tokio::spawn(async move {
        let mut active_preset = *strat_preset_rx.borrow();
        let (mut fast_period, mut slow_period, mut min_ticks_between_signals) =
            active_preset.periods(&strat_config.strategy);
        let mut strategy = MaCrossover::new(fast_period, slow_period, min_ticks_between_signals);
        let mut order_mgr = OrderManager::new(
            strat_rest,
            strat_config.binance.selected_symbol(),
            strat_config.strategy.order_amount_usdt,
        );
        let mut stats_position = Position::new(strat_config.binance.selected_symbol().to_string());
        let stats_store = StrategyStatsStore::open(std::path::Path::new(STRATEGY_STATS_DB_PATH));
        match &stats_store {
            Ok(store) => match store.snapshot() {
                Ok(snapshot) => {
                    let _ = strat_app_tx
                        .send(AppEvent::StrategyStatsUpdate(snapshot))
                        .await;
                }
                Err(e) => {
                    let _ = strat_app_tx
                        .send(AppEvent::LogMessage(format!(
                            "[WARN] Failed to load strategy stats: {}",
                            e
                        )))
                        .await;
                }
            },
            Err(e) => {
                let _ = strat_app_tx
                    .send(AppEvent::LogMessage(format!(
                        "[WARN] Strategy stats DB unavailable: {}",
                        e
                    )))
                    .await;
            }
        }
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
        match order_mgr
            .refresh_order_history(ORDER_HISTORY_PAGE_SIZE)
            .await
        {
            Ok(history) => {
                for fill in &history.fills {
                    let synthetic_fill = Fill {
                        price: fill.avg_price,
                        qty: fill.qty,
                        commission: fill.commission_quote,
                        commission_asset: "USDT".to_string(),
                    };
                    let _ = stats_position.apply_fill(fill.side, &[synthetic_fill]);
                }
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
                fast_period,
                slow_period,
                strat_config.strategy.order_amount_usdt,
                min_ticks_between_signals,
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
                        tracing::info!(signal = ?signal, "Strategy signal received");
                        let _ = strat_app_tx
                            .send(AppEvent::StrategySignal(signal.clone()))
                            .await;

                        // Submit order
                        match order_mgr.submit_order(signal).await {
                            Ok(Some(ref update)) => {
                                if let crate::order_manager::OrderUpdate::Filled { side, fills, .. } = update {
                                    let summary = stats_position.apply_fill(*side, fills);
                                    if summary.winning_closes > 0
                                        || summary.losing_closes > 0
                                        || summary.realized_pnl_delta != 0.0
                                    {
                                        let strategy_label =
                                            strategy_label_for_signal(false, active_preset);
                                        if let Ok(store) = &stats_store {
                                            if let Err(e) = store.increment(
                                                &strategy_label,
                                                summary.winning_closes,
                                                summary.losing_closes,
                                                summary.realized_pnl_delta,
                                            ) {
                                                let _ = strat_app_tx
                                                    .send(AppEvent::LogMessage(format!(
                                                        "[WARN] Failed to persist strategy stats: {}",
                                                        e
                                                    )))
                                                    .await;
                                            } else if let Ok(snapshot) = store.snapshot() {
                                                let _ = strat_app_tx
                                                    .send(AppEvent::StrategyStatsUpdate(snapshot))
                                                    .await;
                                            }
                                        }
                                    }
                                }
                                let _ = strat_app_tx
                                    .send(AppEvent::OrderUpdate(update.clone()))
                                    .await;
                                match order_mgr.refresh_order_history(ORDER_HISTORY_PAGE_SIZE).await {
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
                            if let crate::order_manager::OrderUpdate::Filled { side, fills, .. } = update {
                                let summary = stats_position.apply_fill(*side, fills);
                                if summary.winning_closes > 0
                                    || summary.losing_closes > 0
                                    || summary.realized_pnl_delta != 0.0
                                {
                                    let strategy_label = strategy_label_for_signal(true, active_preset);
                                    if let Ok(store) = &stats_store {
                                        if let Err(e) = store.increment(
                                            &strategy_label,
                                            summary.winning_closes,
                                            summary.losing_closes,
                                            summary.realized_pnl_delta,
                                        ) {
                                            let _ = strat_app_tx
                                                .send(AppEvent::LogMessage(format!(
                                                    "[WARN] Failed to persist strategy stats: {}",
                                                    e
                                                )))
                                                .await;
                                        } else if let Ok(snapshot) = store.snapshot() {
                                            let _ = strat_app_tx
                                                .send(AppEvent::StrategyStatsUpdate(snapshot))
                                                .await;
                                        }
                                    }
                                }
                            }
                            let _ = strat_app_tx
                                .send(AppEvent::OrderUpdate(update.clone()))
                                .await;
                            match order_mgr.refresh_order_history(ORDER_HISTORY_PAGE_SIZE).await {
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
                    match order_mgr.refresh_order_history(ORDER_HISTORY_PAGE_SIZE).await {
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
                _ = strat_preset_rx.changed() => {
                    active_preset = *strat_preset_rx.borrow();
                    (fast_period, slow_period, min_ticks_between_signals) =
                        active_preset.periods(&strat_config.strategy);
                    strategy = MaCrossover::new(fast_period, slow_period, min_ticks_between_signals);
                    for price in &strat_historical_closes {
                        let tick = model::tick::Tick::from_price(*price);
                        strategy.on_tick(&tick);
                    }
                    let _ = strat_app_tx
                        .send(AppEvent::LogMessage(format!(
                            "Strategy switched: {} | MA({}/{}) cooldown={}",
                            active_preset.display_label(),
                            fast_period,
                            slow_period,
                            min_ticks_between_signals,
                        )))
                        .await;
                    let _ = strat_app_tx
                        .send(AppEvent::StrategyState {
                            fast_sma: strategy.fast_sma_value(),
                            slow_sma: strategy.slow_sma_value(),
                        })
                        .await;
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
        config.binance.selected_symbol(),
        config.binance.product_label(),
        StrategyPreset::ConfigMa.display_label(),
        config.ui.price_history_len,
        candle_interval_ms,
        &config.binance.kline_interval,
        StrategyPreset::ConfigMa.periods(&config.strategy).0,
        StrategyPreset::ConfigMa.periods(&config.strategy).1,
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
        config.binance.product_label()
    ));

    loop {
        // Draw
        terminal.draw(|frame| ui::render(frame, &app_state))?;

        // Handle input (non-blocking with timeout)
        if crossterm::event::poll(Duration::from_millis(config.ui.refresh_rate_ms))? {
            if let Event::Key(key) = crossterm::event::read()? {
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
                            .min(STRATEGY_SELECTOR_COUNT - 1);
                        }
                        KeyCode::Enter => {
                            let selected =
                                strategy_from_selector_index(app_state.strategy_selector_index);
                            let _ = strategy_preset_tx.send(selected);
                            let (fast, slow, _) = selected.periods(&config.strategy);
                            app_state.fast_sma = None;
                            app_state.slow_sma = None;
                            app_state.fast_sma_period = fast;
                            app_state.slow_sma_period = slow;
                            app_state.strategy_label = selected.display_label().to_string();
                            app_state.push_log(format!(
                                "Strategy selector: {}",
                                app_state.strategy_label
                            ));
                            app_state.strategy_selector_open = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                if app_state.product_selector_open {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('t') | KeyCode::Char('T') => {
                            app_state.product_selector_open = false;
                        }
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                            app_state.product_selector_index =
                                app_state.product_selector_index.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                            app_state.product_selector_index = (app_state.product_selector_index
                                + 1)
                            .min(PRODUCT_SELECTOR_COUNT - 1);
                        }
                        KeyCode::Enter => {
                            let selected =
                                product_from_selector_index(app_state.product_selector_index);
                            select_product_in_tui(
                                &mut app_state,
                                &strategy_enabled_tx,
                                &ws_symbol_tx,
                                selected,
                            );
                            switch_timeframe(
                                &app_state.timeframe,
                                &app_state.symbol,
                                &rest_client,
                                &config,
                                &app_tx,
                            );
                            app_state.product_selector_open = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                if app_state.account_modal_open {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('a') | KeyCode::Char('A') => {
                            app_state.account_modal_open = false;
                            app_state.account_history_open = false;
                        }
                        KeyCode::Char('h') | KeyCode::Char('H') => {
                            app_state.account_history_open = !app_state.account_history_open;
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
                        if app_state.paused {
                            app_state.paused = false;
                            let _ = strategy_enabled_tx.send(true);
                            app_state.push_log("Strategy ON".to_string());
                        } else {
                            app_state.paused = true;
                            let _ = strategy_enabled_tx.send(false);
                            app_state.push_log("Strategy OFF".to_string());
                        }
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        switch_timeframe(
                            &app_state.timeframe,
                            &app_state.symbol,
                            &rest_client,
                            &config,
                            &app_tx,
                        );
                        app_state.push_log("Refreshed chart data".to_string());
                    }
                    KeyCode::Char('b') | KeyCode::Char('B') => {
                        enqueue_manual_order(
                            &mut app_state,
                            &manual_order_tx,
                            Signal::Buy {
                                trace_id: Uuid::new_v4(),
                            },
                            &format!("Manual BUY ({:.2} USDT)", config.strategy.order_amount_usdt),
                            "BUY",
                        );
                    }
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        enqueue_manual_order(
                            &mut app_state,
                            &manual_order_tx,
                            Signal::Sell {
                                trace_id: Uuid::new_v4(),
                            },
                            "Manual SELL (position)",
                            "SELL",
                        );
                    }
                    KeyCode::Char('1') => {
                        switch_timeframe("1m", &app_state.symbol, &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('0') => {
                        switch_timeframe("1s", &app_state.symbol, &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('h') | KeyCode::Char('H') => {
                        switch_timeframe("1h", &app_state.symbol, &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        switch_timeframe("1d", &app_state.symbol, &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('w') | KeyCode::Char('W') => {
                        switch_timeframe("1w", &app_state.symbol, &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        switch_timeframe("1M", &app_state.symbol, &rest_client, &config, &app_tx);
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') => {
                        app_state.product_selector_index =
                            selector_index_from_product_label(&app_state.product_label);
                        app_state.product_selector_open = true;
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        app_state.strategy_selector_index =
                            selector_index_from_strategy_label(&app_state.strategy_label);
                        app_state.strategy_selector_open = true;
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        app_state.account_modal_open = true;
                        app_state.account_history_open = false;
                    }
                    KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Down => {
                        let max_scroll = app_state.order_history.len().saturating_sub(1);
                        app_state.order_history_scroll =
                            (app_state.order_history_scroll + 1).min(max_scroll);
                    }
                    KeyCode::Char('k') | KeyCode::Char('K') | KeyCode::Up => {
                        app_state.order_history_scroll =
                            app_state.order_history_scroll.saturating_sub(1);
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

#[cfg(test)]
mod tests {
    use super::{
        product_from_selector_index, select_product_in_tui, selector_index_from_product_label,
        selector_index_from_strategy_label, strategy_from_selector_index,
        strategy_label_for_signal,
    };
    use crate::config::{StrategyPreset, TradingProduct};
    use crate::model::candle::CandleBuilder;
    use crate::model::order::OrderSide;
    use crate::ui::chart::FillMarker;
    use crate::ui::AppState;
    use tokio::sync::watch;

    #[test]
    fn select_product_updates_symbol_and_stream_channel() {
        let (strategy_enabled_tx, strategy_enabled_rx) = watch::channel(true);
        let (ws_symbol_tx, ws_symbol_rx) = watch::channel("BTCUSDT".to_string());
        let mut app_state = AppState::new(
            "BTCUSDT",
            "BTC/USDT SPOT",
            "MA(Config)",
            200,
            60_000,
            "1m",
            10,
            30,
        );
        app_state.paused = false;
        app_state.current_candle = Some(CandleBuilder::new(100.0, 60_000, 60_000));
        app_state.fill_markers.push(FillMarker {
            candle_index: 0,
            price: 100.0,
            side: OrderSide::Buy,
        });

        select_product_in_tui(
            &mut app_state,
            &strategy_enabled_tx,
            &ws_symbol_tx,
            TradingProduct::EthFuture,
        );

        assert_eq!(app_state.symbol, "ETHUSDT");
        assert_eq!(app_state.product_label, "ETH/USDT FUTURE");
        assert!(app_state.paused);
        assert!(app_state.current_candle.is_none());
        assert!(app_state.fill_markers.is_empty());
        assert!(!*strategy_enabled_rx.borrow());
        assert_eq!(&*ws_symbol_rx.borrow(), "ETHUSDT");
    }

    #[test]
    fn selector_mapping_roundtrip() {
        for (idx, product) in TradingProduct::ALL.iter().enumerate() {
            assert_eq!(product_from_selector_index(idx), *product);
            assert_eq!(
                selector_index_from_product_label(product.product_label()),
                idx
            );
        }
    }

    #[test]
    fn strategy_selector_mapping_roundtrip() {
        for (idx, preset) in StrategyPreset::ALL.iter().enumerate() {
            assert_eq!(strategy_from_selector_index(idx), *preset);
            assert_eq!(
                selector_index_from_strategy_label(preset.display_label()),
                idx
            );
        }
    }

    #[test]
    fn strategy_label_for_signal_uses_manual_bucket() {
        assert_eq!(
            strategy_label_for_signal(true, StrategyPreset::FastMa),
            "MANUAL"
        );
        assert_eq!(
            strategy_label_for_signal(false, StrategyPreset::SlowMa),
            StrategyPreset::SlowMa.display_label()
        );
    }
}
