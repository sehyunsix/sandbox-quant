pub mod chart;
pub mod dashboard;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};

use crate::event::{AppEvent, WsConnectionStatus};
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_manager::OrderUpdate;

use chart::PriceChart;
use dashboard::{KeybindBar, LogPanel, OrderLogPanel, PositionPanel, StatusBar};

const MAX_LOG_MESSAGES: usize = 200;

pub struct AppState {
    pub symbol: String,
    pub prices: Vec<f64>,
    pub price_history_len: usize,
    pub position: Position,
    pub last_signal: Option<Signal>,
    pub last_order: Option<OrderUpdate>,
    pub fast_sma: Option<f64>,
    pub slow_sma: Option<f64>,
    pub ws_connected: bool,
    pub paused: bool,
    pub tick_count: u64,
    pub log_messages: Vec<String>,
}

impl AppState {
    pub fn new(symbol: &str, price_history_len: usize) -> Self {
        Self {
            symbol: symbol.to_string(),
            prices: Vec::with_capacity(price_history_len),
            price_history_len,
            position: Position::new(symbol.to_string()),
            last_signal: None,
            last_order: None,
            fast_sma: None,
            slow_sma: None,
            ws_connected: false,
            paused: false,
            tick_count: 0,
            log_messages: Vec::new(),
        }
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
                self.prices.push(tick.price);
                if self.prices.len() > self.price_history_len {
                    self.prices.remove(0);
                }
                self.position.update_unrealized_pnl(tick.price);
            }
            AppEvent::StrategySignal(ref signal) => {
                self.last_signal = Some(signal.clone());
                match signal {
                    Signal::Buy { qty } => {
                        self.push_log(format!("Signal: BUY {:.5}", qty));
                    }
                    Signal::Sell { qty } => {
                        self.push_log(format!("Signal: SELL {:.5}", qty));
                    }
                    Signal::Hold => {}
                }
            }
            AppEvent::StrategyState { fast_sma, slow_sma } => {
                self.fast_sma = fast_sma;
                self.slow_sma = slow_sma;
            }
            AppEvent::OrderUpdate(ref update) => {
                match update {
                    OrderUpdate::Filled {
                        client_order_id,
                        side,
                        fills,
                        avg_price,
                    } => {
                        self.position.apply_fill(*side, fills);
                        self.push_log(format!(
                            "FILLED {} {} @ {:.2}",
                            side, client_order_id, avg_price
                        ));
                    }
                    OrderUpdate::Submitted {
                        client_order_id,
                        server_order_id,
                    } => {
                        self.push_log(format!(
                            "Submitted {} (id: {})",
                            client_order_id, server_order_id
                        ));
                    }
                    OrderUpdate::Rejected {
                        client_order_id,
                        reason,
                    } => {
                        self.push_log(format!(
                            "[ERR] Rejected {}: {}",
                            client_order_id, reason
                        ));
                    }
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
            Constraint::Min(8),    // main area (chart + position)
            Constraint::Length(5), // order log
            Constraint::Length(8), // system log
            Constraint::Length(1), // keybinds
        ])
        .split(frame.area());

    // Status bar
    frame.render_widget(
        StatusBar {
            symbol: &state.symbol,
            ws_connected: state.ws_connected,
            paused: state.paused,
            tick_count: state.tick_count,
        },
        outer[0],
    );

    // Main area: chart + position panel
    let main_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(24)])
        .split(outer[1]);

    // Price chart
    let current_price = state.prices.last().copied();
    frame.render_widget(
        PriceChart::new(&state.prices, &state.symbol)
            .fast_sma(state.fast_sma)
            .slow_sma(state.slow_sma),
        main_area[0],
    );

    // Position panel (now with current price)
    frame.render_widget(
        PositionPanel::new(&state.position, current_price),
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

    // System log panel
    frame.render_widget(LogPanel::new(&state.log_messages), outer[3]);

    // Keybind bar
    frame.render_widget(KeybindBar, outer[4]);
}
