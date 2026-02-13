pub mod chart;
pub mod dashboard;

use std::collections::HashMap;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};

use crate::event::{AppEvent, WsConnectionStatus};
use crate::model::candle::{Candle, CandleBuilder};
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_manager::OrderUpdate;

use chart::PriceChart;
use dashboard::{
    KeybindBar, LogPanel, OrderHistoryPanel, OrderLogPanel, PositionPanel, StatusBar,
};

const MAX_LOG_MESSAGES: usize = 200;

pub struct AppState {
    pub symbol: String,
    pub candles: Vec<Candle>,
    pub current_candle: Option<CandleBuilder>,
    pub candle_interval_ms: u64,
    pub timeframe: String,
    pub price_history_len: usize,
    pub position: Position,
    pub last_signal: Option<Signal>,
    pub last_order: Option<OrderUpdate>,
    pub order_history: Vec<String>,
    pub fast_sma: Option<f64>,
    pub slow_sma: Option<f64>,
    pub ws_connected: bool,
    pub paused: bool,
    pub tick_count: u64,
    pub log_messages: Vec<String>,
    pub balances: HashMap<String, f64>,
}

impl AppState {
    pub fn new(symbol: &str, price_history_len: usize, candle_interval_ms: u64, timeframe: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            candles: Vec::with_capacity(price_history_len),
            current_candle: None,
            candle_interval_ms,
            timeframe: timeframe.to_string(),
            price_history_len,
            position: Position::new(symbol.to_string()),
            last_signal: None,
            last_order: None,
            order_history: Vec::new(),
            fast_sma: None,
            slow_sma: None,
            ws_connected: false,
            paused: false,
            tick_count: 0,
            log_messages: Vec::new(),
            balances: HashMap::new(),
        }
    }

    /// Get the latest price (from current candle or last finalized candle).
    pub fn last_price(&self) -> Option<f64> {
        self.current_candle
            .as_ref()
            .map(|cb| cb.close)
            .or_else(|| self.candles.last().map(|c| c.close))
    }

    pub fn push_log(&mut self, msg: String) {
        self.log_messages.push(msg);
        if self.log_messages.len() > MAX_LOG_MESSAGES {
            self.log_messages.remove(0);
        }
    }

    pub fn apply(&mut self, event: AppEvent) {
        match event {
            AppEvent::MarketTick(tick) => {
                self.tick_count += 1;

                // Aggregate tick into candles
                let should_new = match &self.current_candle {
                    Some(cb) => !cb.contains(tick.timestamp_ms),
                    None => true,
                };
                if should_new {
                    if let Some(cb) = self.current_candle.take() {
                        self.candles.push(cb.finish());
                        if self.candles.len() > self.price_history_len {
                            self.candles.remove(0);
                        }
                    }
                    self.current_candle = Some(CandleBuilder::new(
                        tick.price,
                        tick.timestamp_ms,
                        self.candle_interval_ms,
                    ));
                } else {
                    self.current_candle.as_mut().unwrap().update(tick.price);
                }

                self.position.update_unrealized_pnl(tick.price);
            }
            AppEvent::StrategySignal(ref signal) => {
                self.last_signal = Some(signal.clone());
                match signal {
                    Signal::Buy => {
                        self.push_log("Signal: BUY".to_string());
                    }
                    Signal::Sell => {
                        self.push_log("Signal: SELL".to_string());
                    }
                    Signal::Hold => {}
                }
            }
            AppEvent::StrategyState { fast_sma, slow_sma } => {
                self.fast_sma = fast_sma;
                self.slow_sma = slow_sma;
            }
            AppEvent::OrderUpdate(ref update) => {
                let now = chrono::Local::now().format("%H:%M:%S");
                match update {
                    OrderUpdate::Filled {
                        client_order_id,
                        side,
                        fills,
                        avg_price,
                    } => {
                        let total_qty: f64 = fills.iter().map(|f| f.qty).sum();
                        self.position.apply_fill(*side, fills);
                        self.order_history.push(format!(
                            "{} FILLED {} {:.5} @ {:.2}  {}",
                            now, side, total_qty, avg_price, client_order_id
                        ));
                        self.push_log(format!(
                            "FILLED {} {} @ {:.2}",
                            side, client_order_id, avg_price
                        ));
                    }
                    OrderUpdate::Submitted {
                        client_order_id,
                        server_order_id,
                    } => {
                        self.order_history.push(format!(
                            "{} SUBMITTED  {}  (id: {})",
                            now, client_order_id, server_order_id
                        ));
                        self.push_log(format!(
                            "Submitted {} (id: {})",
                            client_order_id, server_order_id
                        ));
                    }
                    OrderUpdate::Rejected {
                        client_order_id,
                        reason,
                    } => {
                        self.order_history.push(format!(
                            "{} REJECTED  {}  {}",
                            now, client_order_id, reason
                        ));
                        self.push_log(format!(
                            "[ERR] Rejected {}: {}",
                            client_order_id, reason
                        ));
                    }
                }
                if self.order_history.len() > MAX_LOG_MESSAGES {
                    self.order_history.remove(0);
                }
                self.last_order = Some(update.clone());
            }
            AppEvent::WsStatus(ref status) => {
                match status {
                    WsConnectionStatus::Connected => {
                        self.ws_connected = true;
                        self.push_log("WebSocket Connected".to_string());
                    }
                    WsConnectionStatus::Disconnected => {
                        self.ws_connected = false;
                        self.push_log("[WARN] WebSocket Disconnected".to_string());
                    }
                    WsConnectionStatus::Reconnecting { attempt, delay_ms } => {
                        self.ws_connected = false;
                        self.push_log(format!(
                            "[WARN] Reconnecting (attempt {}, wait {}ms)",
                            attempt, delay_ms
                        ));
                    }
                }
            }
            AppEvent::HistoricalCandles {
                candles,
                interval_ms,
                interval,
            } => {
                self.candles = candles;
                if self.candles.len() > self.price_history_len {
                    let excess = self.candles.len() - self.price_history_len;
                    self.candles.drain(..excess);
                }
                self.candle_interval_ms = interval_ms;
                self.timeframe = interval;
                self.current_candle = None;
                self.push_log(format!(
                    "Switched to {} ({} candles)",
                    self.timeframe,
                    self.candles.len()
                ));
            }
            AppEvent::BalanceUpdate(balances) => {
                self.balances = balances;
            }
            AppEvent::LogMessage(msg) => {
                self.push_log(msg);
            }
            AppEvent::Error(msg) => {
                self.push_log(format!("[ERR] {}", msg));
            }
        }
    }
}

pub fn render(frame: &mut Frame, state: &AppState) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // status bar
            Constraint::Min(8),     // main area (chart + position)
            Constraint::Length(5),  // order log
            Constraint::Length(6),  // order history
            Constraint::Length(8),  // system log
            Constraint::Length(1),  // keybinds
        ])
        .split(frame.area());

    // Status bar
    frame.render_widget(
        StatusBar {
            symbol: &state.symbol,
            ws_connected: state.ws_connected,
            paused: state.paused,
            tick_count: state.tick_count,
            timeframe: &state.timeframe,
        },
        outer[0],
    );

    // Main area: chart + position panel
    let main_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(24)])
        .split(outer[1]);

    // Price chart (candles + in-progress candle)
    let current_price = state.last_price();
    frame.render_widget(
        PriceChart::new(&state.candles, &state.symbol)
            .current_candle(state.current_candle.as_ref())
            .fast_sma(state.fast_sma)
            .slow_sma(state.slow_sma),
        main_area[0],
    );

    // Position panel (with current price and balances)
    frame.render_widget(
        PositionPanel::new(&state.position, current_price, &state.balances),
        main_area[1],
    );

    // Order log
    frame.render_widget(
        OrderLogPanel::new(
            &state.last_signal,
            &state.last_order,
            state.fast_sma,
            state.slow_sma,
        ),
        outer[2],
    );

    // Order history panel
    frame.render_widget(OrderHistoryPanel::new(&state.order_history), outer[3]);

    // System log panel
    frame.render_widget(LogPanel::new(&state.log_messages), outer[4]);

    // Keybind bar
    frame.render_widget(KeybindBar, outer[5]);
}
