pub mod chart;
pub mod dashboard;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};

use crate::event::{AppEvent, WsConnectionStatus};
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_manager::OrderUpdate;

use chart::PriceChart;
use dashboard::{KeybindBar, OrderLogPanel, PositionPanel, StatusBar};

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
            AppEvent::StrategySignal(signal) => {
                self.last_signal = Some(signal);
            }
            AppEvent::OrderUpdate(update) => {
                if let OrderUpdate::Filled { side, ref fills, .. } = update {
                    self.position.apply_fill(side, fills);
                }
                self.last_order = Some(update);
            }
            AppEvent::WsStatus(status) => {
                self.ws_connected = matches!(status, WsConnectionStatus::Connected);
            }
            AppEvent::Error(_) => {}
        }
    }
}

pub fn render(frame: &mut Frame, state: &AppState) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // status bar
            Constraint::Min(8),    // main area
            Constraint::Length(5), // order log
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
        .constraints([Constraint::Min(40), Constraint::Length(22)])
        .split(outer[1]);

    // Price chart
    frame.render_widget(
        PriceChart::new(&state.prices)
            .fast_sma(state.fast_sma)
            .slow_sma(state.slow_sma),
        main_area[0],
    );

    // Position panel
    frame.render_widget(PositionPanel::new(&state.position), main_area[1]);

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

    // Keybind bar
    frame.render_widget(KeybindBar, outer[3]);
}
