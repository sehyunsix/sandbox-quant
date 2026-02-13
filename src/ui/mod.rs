pub mod chart;
pub mod dashboard;

use std::collections::HashMap;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::Clear;
use ratatui::Frame;

use crate::alpaca::rest::OptionChainSnapshot;
use crate::config::{StrategyPreset, TradingProduct};
use crate::event::{AppEvent, WsConnectionStatus};
use crate::model::candle::{Candle, CandleBuilder};
use crate::model::order::Fill;
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_manager::{HistoricalFill, OrderUpdate};
use crate::strategy_stats::StrategyStats;

use chart::{FillMarker, PriceChart};
use dashboard::{
    AccountHistoryPanel, AccountPanel, KeybindBar, LogPanel, OptionPanel, OrderHistoryPanel,
    OrderLogPanel, PositionPanel, ProductSelectorPanel, StatusBar, StrategySelectorPanel,
};

const MAX_SYSTEM_LOG_MESSAGES: usize = 200;
const MAX_FILL_MARKERS: usize = 200;
const MAX_ACCOUNT_TOTAL_POINTS: usize = 720;

#[derive(Debug, Clone, Copy)]
pub struct AccountTotalPoint {
    pub timestamp_ms: u64,
    pub total: f64,
}

pub struct AppState {
    pub symbol: String,
    pub product_label: String,
    pub strategy_label: String,
    pub candles: Vec<Candle>,
    pub current_candle: Option<CandleBuilder>,
    pub candle_interval_ms: u64,
    pub timeframe: String,
    pub price_history_len: usize,
    pub position: Position,
    pub last_signal: Option<Signal>,
    pub last_order: Option<OrderUpdate>,
    pub order_history: Vec<String>,
    pub order_history_fills: Vec<HistoricalFill>,
    pub order_history_scroll: usize,
    pub fast_sma: Option<f64>,
    pub slow_sma: Option<f64>,
    pub fast_sma_period: usize,
    pub slow_sma_period: usize,
    pub ws_connected: bool,
    pub paused: bool,
    pub tick_count: u64,
    pub last_market_update_ms: Option<u64>,
    pub log_messages: Vec<String>,
    pub balances: HashMap<String, f64>,
    pub strategy_stats: HashMap<String, StrategyStats>,
    pub option_chain: Option<OptionChainSnapshot>,
    pub fill_markers: Vec<FillMarker>,
    pub product_selector_open: bool,
    pub product_selector_index: usize,
    pub product_selector_items: Vec<String>,
    pub strategy_selector_open: bool,
    pub strategy_selector_index: usize,
    pub account_modal_open: bool,
    pub account_history_open: bool,
    pub account_total_history: Vec<AccountTotalPoint>,
}

impl AppState {
    pub fn new(
        symbol: &str,
        product_label: &str,
        strategy_label: &str,
        price_history_len: usize,
        candle_interval_ms: u64,
        timeframe: &str,
        fast_sma_period: usize,
        slow_sma_period: usize,
    ) -> Self {
        Self {
            symbol: symbol.to_string(),
            product_label: product_label.to_string(),
            strategy_label: strategy_label.to_string(),
            candles: Vec::with_capacity(price_history_len),
            current_candle: None,
            candle_interval_ms,
            timeframe: timeframe.to_string(),
            price_history_len,
            position: Position::new(symbol.to_string()),
            last_signal: None,
            last_order: None,
            order_history: Vec::new(),
            order_history_fills: Vec::new(),
            order_history_scroll: 0,
            fast_sma: None,
            slow_sma: None,
            fast_sma_period,
            slow_sma_period,
            ws_connected: false,
            paused: false,
            tick_count: 0,
            last_market_update_ms: None,
            log_messages: Vec::new(),
            balances: HashMap::new(),
            strategy_stats: HashMap::new(),
            option_chain: None,
            fill_markers: Vec::new(),
            product_selector_open: false,
            product_selector_index: 0,
            product_selector_items: vec![
                TradingProduct::BtcSpot.selector_label().to_string(),
                TradingProduct::BtcFuture.selector_label().to_string(),
                TradingProduct::EthSpot.selector_label().to_string(),
                TradingProduct::EthFuture.selector_label().to_string(),
            ],
            strategy_selector_open: false,
            strategy_selector_index: 0,
            account_modal_open: false,
            account_history_open: false,
            account_total_history: Vec::new(),
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
        if self.log_messages.len() > MAX_SYSTEM_LOG_MESSAGES {
            self.log_messages.remove(0);
        }
    }

    fn split_symbol_assets(&self) -> Option<(&str, &str)> {
        for quote in ["USDT", "USDC", "BUSD", "FDUSD", "BTC", "ETH", "BNB"] {
            if let Some(base) = self.symbol.strip_suffix(quote) {
                if !base.is_empty() {
                    return Some((base, quote));
                }
            }
        }
        None
    }

    fn account_total_value(&self) -> Option<f64> {
        if self.balances.is_empty() {
            return None;
        }
        if let Some((base, quote)) = self.split_symbol_assets() {
            let quote_bal = self.balances.get(quote).copied().unwrap_or(0.0);
            let base_bal = self.balances.get(base).copied().unwrap_or(0.0);
            let px = self.last_price().unwrap_or(0.0);
            return Some(quote_bal + base_bal * px);
        }
        Some(self.balances.values().copied().sum::<f64>())
    }

    fn rebuild_account_total_history_from_order_fills(&mut self) {
        if self.order_history_fills.is_empty() {
            self.account_total_history.clear();
            return;
        }

        let mut fills = self.order_history_fills.clone();
        fills.sort_by_key(|f| f.timestamp_ms);

        let mut quote_delta = 0.0_f64;
        let mut base_delta = 0.0_f64;
        let mut rel_points: Vec<AccountTotalPoint> = Vec::with_capacity(fills.len());
        let mut last_fill_price = 0.0_f64;

        for fill in fills {
            match fill.side {
                crate::model::order::OrderSide::Buy => {
                    quote_delta -= fill.avg_price * fill.qty + fill.commission_quote;
                    base_delta += fill.qty;
                }
                crate::model::order::OrderSide::Sell => {
                    quote_delta += fill.avg_price * fill.qty - fill.commission_quote;
                    base_delta -= fill.qty;
                }
            }
            last_fill_price = fill.avg_price;
            rel_points.push(AccountTotalPoint {
                timestamp_ms: fill.timestamp_ms,
                total: quote_delta + base_delta * fill.avg_price,
            });
        }

        let mark_price = self.last_price().unwrap_or(last_fill_price);
        let relative_now = quote_delta + base_delta * mark_price;
        let current_total = self.account_total_value().unwrap_or(relative_now);
        let anchor = current_total - relative_now;

        self.account_total_history = rel_points
            .into_iter()
            .map(|p| AccountTotalPoint {
                timestamp_ms: p.timestamp_ms,
                total: p.total + anchor,
            })
            .collect();

        if self.account_total_history.len() > MAX_ACCOUNT_TOTAL_POINTS {
            let drop_n = self.account_total_history.len() - MAX_ACCOUNT_TOTAL_POINTS;
            self.account_total_history.drain(..drop_n);
        }
    }

    fn candle_index_for_timestamp(&self, timestamp_ms: u64) -> Option<usize> {
        if let Some(index) = self
            .candles
            .iter()
            .position(|c| timestamp_ms >= c.open_time && timestamp_ms < c.close_time)
        {
            return Some(index);
        }
        if self
            .current_candle
            .as_ref()
            .is_some_and(|cb| cb.contains(timestamp_ms))
        {
            return Some(self.candles.len());
        }
        None
    }

    fn rebuild_fill_markers_from_history(&mut self, fills: &[HistoricalFill]) {
        self.fill_markers.clear();
        for fill in fills {
            if let Some(candle_index) = self.candle_index_for_timestamp(fill.timestamp_ms) {
                self.fill_markers.push(FillMarker {
                    candle_index,
                    price: fill.avg_price,
                    side: fill.side,
                });
                if self.fill_markers.len() > MAX_FILL_MARKERS {
                    self.fill_markers.remove(0);
                }
            }
        }
    }

    fn rebuild_position_from_history(&mut self, fills: &[HistoricalFill]) {
        let mut reconstructed = Position::new(self.symbol.clone());
        for fill in fills {
            let synthetic_fill = Fill {
                price: fill.avg_price,
                qty: fill.qty,
                commission: fill.commission_quote,
                commission_asset: "USDT".to_string(),
            };
            let _ = reconstructed.apply_fill(fill.side, &[synthetic_fill]);
        }
        self.position = reconstructed;
        if let Some(price) = self.last_price() {
            self.position.update_unrealized_pnl(price);
        }
    }

    pub fn apply(&mut self, event: AppEvent) {
        match event {
            AppEvent::MarketTick(tick) => {
                self.tick_count += 1;
                self.last_market_update_ms = Some(chrono::Utc::now().timestamp_millis() as u64);

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
                            // Shift marker indices when oldest candle is trimmed.
                            self.fill_markers.retain_mut(|m| {
                                if m.candle_index == 0 {
                                    false
                                } else {
                                    m.candle_index -= 1;
                                    true
                                }
                            });
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
                self.rebuild_account_total_history_from_order_fills();
            }
            AppEvent::DataHeartbeat => {
                self.last_market_update_ms = Some(chrono::Utc::now().timestamp_millis() as u64);
            }
            AppEvent::StrategySignal(ref signal) => {
                self.last_signal = Some(signal.clone());
                match signal {
                    Signal::Buy { .. } => {
                        self.push_log("Signal: BUY".to_string());
                    }
                    Signal::Sell { .. } => {
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
                match update {
                    OrderUpdate::Filled {
                        client_order_id,
                        side,
                        fills,
                        avg_price,
                    } => {
                        let _ = self.position.apply_fill(*side, fills);
                        let candle_index = if self.current_candle.is_some() {
                            self.candles.len()
                        } else {
                            self.candles.len().saturating_sub(1)
                        };
                        self.fill_markers.push(FillMarker {
                            candle_index,
                            price: *avg_price,
                            side: *side,
                        });
                        if self.fill_markers.len() > MAX_FILL_MARKERS {
                            self.fill_markers.remove(0);
                        }
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
                        self.push_log(format!("[ERR] Rejected {}: {}", client_order_id, reason));
                    }
                }
                self.last_order = Some(update.clone());
            }
            AppEvent::WsStatus(ref status) => match status {
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
            },
            AppEvent::HistoricalCandles {
                candles,
                interval_ms,
                interval,
            } => {
                self.last_market_update_ms = Some(chrono::Utc::now().timestamp_millis() as u64);
                self.candles = candles;
                if self.candles.len() > self.price_history_len {
                    let excess = self.candles.len() - self.price_history_len;
                    self.candles.drain(..excess);
                }
                self.candle_interval_ms = interval_ms;
                self.timeframe = interval;
                self.current_candle = None;
                self.fill_markers.clear();
                self.push_log(format!(
                    "Switched to {} ({} candles)",
                    self.timeframe,
                    self.candles.len()
                ));
            }
            AppEvent::OptionChainUpdate(chain) => {
                self.option_chain = chain;
            }
            AppEvent::BalanceUpdate(balances) => {
                self.balances = balances;
                self.rebuild_account_total_history_from_order_fills();
            }
            AppEvent::StrategyStatsUpdate(stats) => {
                self.strategy_stats = stats;
            }
            AppEvent::OrderHistoryUpdate(snapshot) => {
                self.order_history = snapshot.rows;
                self.order_history_fills = snapshot.fills;
                let max_scroll = self.order_history.len().saturating_sub(1);
                self.order_history_scroll = self.order_history_scroll.min(max_scroll);
                let fills = self.order_history_fills.clone();
                self.rebuild_position_from_history(&fills);
                self.rebuild_fill_markers_from_history(&fills);
                self.rebuild_account_total_history_from_order_fills();
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
            Constraint::Length(4),  // order log
            Constraint::Length(10), // order history
            Constraint::Length(5),  // system log
            Constraint::Length(1),  // keybinds
        ])
        .split(frame.area());

    // Status bar
    frame.render_widget(
        StatusBar {
            symbol: &state.symbol,
            product_label: &state.product_label,
            strategy_label: &state.strategy_label,
            ws_connected: state.ws_connected,
            paused: state.paused,
            tick_count: state.tick_count,
            last_market_update_ms: state.last_market_update_ms,
            timeframe: &state.timeframe,
        },
        outer[0],
    );

    // Main area: chart + position panel
    let right_panel_width = if state.product_label == "US OPTION" {
        38
    } else {
        24
    };
    let main_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(right_panel_width)])
        .split(outer[1]);

    // Price chart (candles + in-progress candle)
    let current_price = state.last_price();
    frame.render_widget(
        PriceChart::new(&state.candles, &state.symbol)
            .current_candle(state.current_candle.as_ref())
            .fill_markers(&state.fill_markers)
            .fast_sma(state.fast_sma)
            .slow_sma(state.slow_sma)
            .sma_periods(state.fast_sma_period, state.slow_sma_period),
        main_area[0],
    );

    // Position panel (with current price and balances)
    if state.product_label == "US OPTION" {
        frame.render_widget(
            OptionPanel {
                chain: state.option_chain.as_ref(),
            },
            main_area[1],
        );
    } else {
        frame.render_widget(
            PositionPanel::new(
                &state.position,
                current_price,
                &state.balances,
                &state.strategy_label,
                state.strategy_stats.get(&state.strategy_label).copied(),
            ),
            main_area[1],
        );
    }

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
    frame.render_widget(
        OrderHistoryPanel::new(&state.order_history, state.order_history_scroll),
        outer[3],
    );

    // System log panel
    frame.render_widget(LogPanel::new(&state.log_messages), outer[4]);

    // Keybind bar
    frame.render_widget(KeybindBar, outer[5]);

    if state.product_selector_open {
        let popup = centered_rect(36, 8, frame.area());
        let options: Vec<&str> = state
            .product_selector_items
            .iter()
            .map(String::as_str)
            .collect();
        frame.render_widget(Clear, popup);
        frame.render_widget(
            ProductSelectorPanel {
                items: &options,
                selected: state.product_selector_index,
            },
            popup,
        );
    }

    if state.strategy_selector_open {
        let popup = centered_rect(84, 9, frame.area());
        let items: Vec<String> = StrategyPreset::ALL
            .iter()
            .map(|preset| {
                let label = preset.display_label();
                let stats = state.strategy_stats.get(label).copied().unwrap_or_default();
                format!(
                    "{:<13}  pnl {:+9.2}  trades {:>4}  w {:>3}  l {:>3}  wr {:>5.1}%",
                    preset.selector_label(),
                    stats.realized_pnl,
                    stats.total(),
                    stats.wins,
                    stats.losses,
                    stats.win_rate_percent()
                )
            })
            .collect();
        let options: Vec<&str> = items.iter().map(String::as_str).collect();
        frame.render_widget(Clear, popup);
        frame.render_widget(
            StrategySelectorPanel {
                items: &options,
                selected: state.strategy_selector_index,
            },
            popup,
        );
    }

    if state.account_modal_open {
        let popup = centered_rect(64, 18, frame.area());
        frame.render_widget(Clear, popup);
        frame.render_widget(
            AccountPanel {
                symbol: &state.symbol,
                product_label: &state.product_label,
                position: &state.position,
                balances: &state.balances,
                strategy_stats: &state.strategy_stats,
            },
            popup,
        );
    }

    if state.account_history_open {
        let popup = centered_rect(64, 18, frame.area());
        frame.render_widget(Clear, popup);
        frame.render_widget(
            AccountHistoryPanel {
                points: &state.account_total_history,
            },
            popup,
        );
    }
}

fn centered_rect(width: u16, height: u16, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    ratatui::layout::Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}
