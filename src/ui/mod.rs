pub mod chart;
pub mod dashboard;
pub mod app_state_v2;

use std::collections::HashMap;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::event::{AppEvent, WsConnectionStatus};
use crate::model::candle::{Candle, CandleBuilder};
use crate::model::order::{Fill, OrderSide};
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_manager::{OrderHistoryFill, OrderHistoryStats, OrderUpdate};
use crate::order_store;
use crate::risk_module::RateBudgetSnapshot;

use app_state_v2::AppStateV2;
use chart::{FillMarker, PriceChart};
use dashboard::{KeybindBar, LogPanel, OrderHistoryPanel, OrderLogPanel, PositionPanel, StatusBar};

const MAX_LOG_MESSAGES: usize = 200;
const MAX_FILL_MARKERS: usize = 200;

pub struct AppState {
    pub symbol: String,
    pub strategy_label: String,
    pub candles: Vec<Candle>,
    pub current_candle: Option<CandleBuilder>,
    pub candle_interval_ms: u64,
    pub timeframe: String,
    pub price_history_len: usize,
    pub position: Position,
    pub last_signal: Option<Signal>,
    pub last_order: Option<OrderUpdate>,
    pub open_order_history: Vec<String>,
    pub filled_order_history: Vec<String>,
    pub fast_sma: Option<f64>,
    pub slow_sma: Option<f64>,
    pub ws_connected: bool,
    pub paused: bool,
    pub tick_count: u64,
    pub log_messages: Vec<String>,
    pub balances: HashMap<String, f64>,
    pub initial_equity_usdt: Option<f64>,
    pub current_equity_usdt: Option<f64>,
    pub history_estimated_total_pnl_usdt: Option<f64>,
    pub fill_markers: Vec<FillMarker>,
    pub history_trade_count: u32,
    pub history_win_count: u32,
    pub history_lose_count: u32,
    pub history_realized_pnl: f64,
    pub strategy_stats: HashMap<String, OrderHistoryStats>,
    pub history_fills: Vec<OrderHistoryFill>,
    pub last_price_update_ms: Option<u64>,
    pub last_price_event_ms: Option<u64>,
    pub last_price_latency_ms: Option<u64>,
    pub last_order_history_update_ms: Option<u64>,
    pub last_order_history_event_ms: Option<u64>,
    pub last_order_history_latency_ms: Option<u64>,
    pub trade_stats_reset_warned: bool,
    pub symbol_selector_open: bool,
    pub symbol_selector_index: usize,
    pub symbol_items: Vec<String>,
    pub strategy_selector_open: bool,
    pub strategy_selector_index: usize,
    pub strategy_items: Vec<String>,
    pub account_popup_open: bool,
    pub history_popup_open: bool,
    pub focus_popup_open: bool,
    pub history_rows: Vec<String>,
    pub history_bucket: order_store::HistoryBucket,
    pub last_applied_fee: String,
    pub v2_grid_open: bool,
    pub v2_state: AppStateV2,
    pub rate_budget_global: RateBudgetSnapshot,
    pub rate_budget_orders: RateBudgetSnapshot,
    pub rate_budget_account: RateBudgetSnapshot,
    pub rate_budget_market_data: RateBudgetSnapshot,
}

impl AppState {
    pub fn new(
        symbol: &str,
        strategy_label: &str,
        price_history_len: usize,
        candle_interval_ms: u64,
        timeframe: &str,
    ) -> Self {
        Self {
            symbol: symbol.to_string(),
            strategy_label: strategy_label.to_string(),
            candles: Vec::with_capacity(price_history_len),
            current_candle: None,
            candle_interval_ms,
            timeframe: timeframe.to_string(),
            price_history_len,
            position: Position::new(symbol.to_string()),
            last_signal: None,
            last_order: None,
            open_order_history: Vec::new(),
            filled_order_history: Vec::new(),
            fast_sma: None,
            slow_sma: None,
            ws_connected: false,
            paused: false,
            tick_count: 0,
            log_messages: Vec::new(),
            balances: HashMap::new(),
            initial_equity_usdt: None,
            current_equity_usdt: None,
            history_estimated_total_pnl_usdt: None,
            fill_markers: Vec::new(),
            history_trade_count: 0,
            history_win_count: 0,
            history_lose_count: 0,
            history_realized_pnl: 0.0,
            strategy_stats: HashMap::new(),
            history_fills: Vec::new(),
            last_price_update_ms: None,
            last_price_event_ms: None,
            last_price_latency_ms: None,
            last_order_history_update_ms: None,
            last_order_history_event_ms: None,
            last_order_history_latency_ms: None,
            trade_stats_reset_warned: false,
            symbol_selector_open: false,
            symbol_selector_index: 0,
            symbol_items: Vec::new(),
            strategy_selector_open: false,
            strategy_selector_index: 0,
            strategy_items: vec![
                "MA(Config)".to_string(),
                "MA(Fast 5/20)".to_string(),
                "MA(Slow 20/60)".to_string(),
            ],
            account_popup_open: false,
            history_popup_open: false,
            focus_popup_open: false,
            history_rows: Vec::new(),
            history_bucket: order_store::HistoryBucket::Day,
            last_applied_fee: "---".to_string(),
            v2_grid_open: false,
            v2_state: AppStateV2::new(),
            rate_budget_global: RateBudgetSnapshot {
                used: 0,
                limit: 0,
                reset_in_ms: 0,
            },
            rate_budget_orders: RateBudgetSnapshot {
                used: 0,
                limit: 0,
                reset_in_ms: 0,
            },
            rate_budget_account: RateBudgetSnapshot {
                used: 0,
                limit: 0,
                reset_in_ms: 0,
            },
            rate_budget_market_data: RateBudgetSnapshot {
                used: 0,
                limit: 0,
                reset_in_ms: 0,
            },
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

    pub fn refresh_history_rows(&mut self) {
        match order_store::load_realized_returns_by_bucket(self.history_bucket, 400) {
            Ok(rows) => {
                use std::collections::{BTreeMap, BTreeSet};

                let mut date_set: BTreeSet<String> = BTreeSet::new();
                let mut ticker_map: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
                for row in rows {
                    date_set.insert(row.date.clone());
                    ticker_map
                        .entry(row.symbol.clone())
                        .or_default()
                        .insert(row.date, row.realized_return_pct);
                }

                // Keep recent dates only to avoid horizontal overflow in terminal.
                let mut dates: Vec<String> = date_set.into_iter().collect();
                dates.sort();
                const MAX_DATE_COLS: usize = 6;
                if dates.len() > MAX_DATE_COLS {
                    dates = dates[dates.len() - MAX_DATE_COLS..].to_vec();
                }

                let mut lines = Vec::new();
                if dates.is_empty() {
                    lines.push("Ticker            (no daily realized roi data)".to_string());
                    self.history_rows = lines;
                    return;
                }

                let mut header = format!("{:<14}", "Ticker");
                for d in &dates {
                    header.push_str(&format!(" {:>10}", d));
                }
                lines.push(header);

                for (ticker, by_date) in ticker_map {
                    let mut line = format!("{:<14}", ticker);
                    for d in &dates {
                        let cell = by_date
                            .get(d)
                            .map(|v| format!("{:.2}%", v))
                            .unwrap_or_else(|| "-".to_string());
                        line.push_str(&format!(" {:>10}", cell));
                    }
                    lines.push(line);
                }
                self.history_rows = lines;
            }
            Err(e) => {
                self.history_rows = vec![
                    "Ticker           Date         RealizedROI   RealizedPnL".to_string(),
                    format!("(failed to load history: {})", e),
                ];
            }
        }
    }

    fn refresh_equity_usdt(&mut self) {
        let usdt = self.balances.get("USDT").copied().unwrap_or(0.0);
        let btc = self.balances.get("BTC").copied().unwrap_or(0.0);
        let mark_price = self
            .last_price()
            .or_else(|| (self.position.entry_price > 0.0).then_some(self.position.entry_price));
        if let Some(price) = mark_price {
            let total = usdt + btc * price;
            self.current_equity_usdt = Some(total);
            self.recompute_initial_equity_from_history();
        }
    }

    fn recompute_initial_equity_from_history(&mut self) {
        if let Some(current) = self.current_equity_usdt {
            if let Some(total_pnl) = self.history_estimated_total_pnl_usdt {
                self.initial_equity_usdt = Some(current - total_pnl);
            } else if self.history_trade_count == 0 && self.initial_equity_usdt.is_none() {
                self.initial_equity_usdt = Some(current);
            }
        }
    }

    fn candle_index_for_timestamp(&self, timestamp_ms: u64) -> Option<usize> {
        if let Some((idx, _)) = self
            .candles
            .iter()
            .enumerate()
            .find(|(_, c)| timestamp_ms >= c.open_time && timestamp_ms < c.close_time)
        {
            return Some(idx);
        }
        if let Some(cb) = &self.current_candle {
            if cb.contains(timestamp_ms) {
                return Some(self.candles.len());
            }
        }
        // Fallback: if timestamp is newer than the latest finalized candle range
        // (e.g. coarse timeframe like 1M and no in-progress bucket), pin to nearest past candle.
        if let Some((idx, _)) = self
            .candles
            .iter()
            .enumerate()
            .rev()
            .find(|(_, c)| c.open_time <= timestamp_ms)
        {
            return Some(idx);
        }
        None
    }

    fn rebuild_fill_markers_from_history(&mut self, fills: &[OrderHistoryFill]) {
        self.fill_markers.clear();
        for fill in fills {
            if let Some(candle_index) = self.candle_index_for_timestamp(fill.timestamp_ms) {
                self.fill_markers.push(FillMarker {
                    candle_index,
                    price: fill.price,
                    side: fill.side,
                });
            }
        }
        if self.fill_markers.len() > MAX_FILL_MARKERS {
            let excess = self.fill_markers.len() - MAX_FILL_MARKERS;
            self.fill_markers.drain(..excess);
        }
    }

    pub fn apply(&mut self, event: AppEvent) {
        let prev_focus = self.v2_state.focus.clone();
        match event {
            AppEvent::MarketTick(tick) => {
                self.tick_count += 1;
                let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                self.last_price_update_ms = Some(now_ms);
                self.last_price_event_ms = Some(tick.timestamp_ms);
                self.last_price_latency_ms = Some(now_ms.saturating_sub(tick.timestamp_ms));

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
                } else if let Some(cb) = self.current_candle.as_mut() {
                    cb.update(tick.price);
                } else {
                    // Defensive fallback: avoid panic if tick ordering/state gets out of sync.
                    self.current_candle = Some(CandleBuilder::new(
                        tick.price,
                        tick.timestamp_ms,
                        self.candle_interval_ms,
                    ));
                    self.push_log("[WARN] Recovered missing current candle state".to_string());
                }

                self.position.update_unrealized_pnl(tick.price);
                self.refresh_equity_usdt();
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
                        intent_id,
                        client_order_id,
                        side,
                        fills,
                        avg_price,
                    } => {
                        if let Some(summary) = format_last_applied_fee(&self.symbol, fills) {
                            self.last_applied_fee = summary;
                        }
                        self.position.apply_fill(*side, fills);
                        self.refresh_equity_usdt();
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
                            "FILLED {} {} ({}) @ {:.2}",
                            side, client_order_id, intent_id, avg_price
                        ));
                    }
                    OrderUpdate::Submitted {
                        intent_id,
                        client_order_id,
                        server_order_id,
                    } => {
                        self.refresh_equity_usdt();
                        self.push_log(format!(
                            "Submitted {} (id: {}, {})",
                            client_order_id, server_order_id, intent_id
                        ));
                    }
                    OrderUpdate::Rejected {
                        intent_id,
                        client_order_id,
                        reason_code,
                        reason,
                    } => {
                        self.push_log(format!(
                            "[ERR] Rejected {} ({}) [{}]: {}",
                            client_order_id, intent_id, reason_code, reason
                        ));
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
                self.candles = candles;
                if self.candles.len() > self.price_history_len {
                    let excess = self.candles.len() - self.price_history_len;
                    self.candles.drain(..excess);
                }
                self.candle_interval_ms = interval_ms;
                self.timeframe = interval;
                self.current_candle = None;
                let fills = self.history_fills.clone();
                self.rebuild_fill_markers_from_history(&fills);
                self.push_log(format!(
                    "Switched to {} ({} candles)",
                    self.timeframe,
                    self.candles.len()
                ));
            }
            AppEvent::BalanceUpdate(balances) => {
                self.balances = balances;
                self.refresh_equity_usdt();
            }
            AppEvent::OrderHistoryUpdate(snapshot) => {
                let mut open = Vec::new();
                let mut filled = Vec::new();

                for row in snapshot.rows {
                    let status = row.split_whitespace().nth(1).unwrap_or_default();
                    if status == "FILLED" {
                        filled.push(row);
                    } else {
                        open.push(row);
                    }
                }

                if open.len() > MAX_LOG_MESSAGES {
                    let excess = open.len() - MAX_LOG_MESSAGES;
                    open.drain(..excess);
                }
                if filled.len() > MAX_LOG_MESSAGES {
                    let excess = filled.len() - MAX_LOG_MESSAGES;
                    filled.drain(..excess);
                }

                self.open_order_history = open;
                self.filled_order_history = filled;
                if snapshot.trade_data_complete {
                    let stats_looks_reset = snapshot.stats.trade_count == 0
                        && (self.history_trade_count > 0 || !self.history_fills.is_empty());
                    if stats_looks_reset {
                        if !self.trade_stats_reset_warned {
                            self.push_log(
                                "[WARN] Ignored transient trade stats reset from order-history sync"
                                    .to_string(),
                            );
                            self.trade_stats_reset_warned = true;
                        }
                    } else {
                        self.trade_stats_reset_warned = false;
                        self.history_trade_count = snapshot.stats.trade_count;
                        self.history_win_count = snapshot.stats.win_count;
                        self.history_lose_count = snapshot.stats.lose_count;
                        self.history_realized_pnl = snapshot.stats.realized_pnl;
                        self.strategy_stats = snapshot.strategy_stats;
                        // Keep position panel aligned with exchange history state
                        // so Qty/Entry/UnrPL reflect actual holdings, not only session fills.
                        if snapshot.open_qty > f64::EPSILON {
                            self.position.side = Some(OrderSide::Buy);
                            self.position.qty = snapshot.open_qty;
                            self.position.entry_price = snapshot.open_entry_price;
                            if let Some(px) = self.last_price() {
                                self.position.unrealized_pnl =
                                    (px - snapshot.open_entry_price) * snapshot.open_qty;
                            }
                        } else {
                            self.position.side = None;
                            self.position.qty = 0.0;
                            self.position.entry_price = 0.0;
                            self.position.unrealized_pnl = 0.0;
                        }
                    }
                    if !snapshot.fills.is_empty() || self.history_fills.is_empty() {
                        self.history_fills = snapshot.fills.clone();
                        self.rebuild_fill_markers_from_history(&snapshot.fills);
                    }
                    self.history_estimated_total_pnl_usdt = snapshot.estimated_total_pnl_usdt;
                    self.recompute_initial_equity_from_history();
                }
                self.last_order_history_update_ms = Some(snapshot.fetched_at_ms);
                self.last_order_history_event_ms = snapshot.latest_event_ms;
                self.last_order_history_latency_ms = Some(snapshot.fetch_latency_ms);
                self.refresh_history_rows();
            }
            AppEvent::RiskRateSnapshot {
                global,
                orders,
                account,
                market_data,
            } => {
                self.rate_budget_global = global;
                self.rate_budget_orders = orders;
                self.rate_budget_account = account;
                self.rate_budget_market_data = market_data;
            }
            AppEvent::LogMessage(msg) => {
                self.push_log(msg);
            }
            AppEvent::Error(msg) => {
                self.push_log(format!("[ERR] {}", msg));
            }
        }
        let mut next = AppStateV2::from_legacy(self);
        if prev_focus.symbol.is_some() {
            next.focus.symbol = prev_focus.symbol;
        }
        if prev_focus.strategy_id.is_some() {
            next.focus.strategy_id = prev_focus.strategy_id;
        }
        self.v2_state = next;
    }
}

pub fn render(frame: &mut Frame, state: &AppState) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status bar
            Constraint::Min(8),    // main area (chart + position)
            Constraint::Length(5), // order log
            Constraint::Length(6), // order history
            Constraint::Length(8), // system log
            Constraint::Length(1), // keybinds
        ])
        .split(frame.area());

    // Status bar
    frame.render_widget(
        StatusBar {
            symbol: &state.symbol,
            strategy_label: &state.strategy_label,
            ws_connected: state.ws_connected,
            paused: state.paused,
            timeframe: &state.timeframe,
            last_price_update_ms: state.last_price_update_ms,
            last_price_latency_ms: state.last_price_latency_ms,
            last_order_history_update_ms: state.last_order_history_update_ms,
            last_order_history_latency_ms: state.last_order_history_latency_ms,
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
            .fill_markers(&state.fill_markers)
            .fast_sma(state.fast_sma)
            .slow_sma(state.slow_sma),
        main_area[0],
    );

    // Position panel (with current price and balances)
    frame.render_widget(
        PositionPanel::new(
            &state.position,
            current_price,
            &state.balances,
            state.initial_equity_usdt,
            state.current_equity_usdt,
            state.history_trade_count,
            state.history_realized_pnl,
            &state.last_applied_fee,
        ),
        main_area[1],
    );

    // Order log
    frame.render_widget(
        OrderLogPanel::new(
            &state.last_signal,
            &state.last_order,
            state.fast_sma,
            state.slow_sma,
            state.history_trade_count,
            state.history_win_count,
            state.history_lose_count,
            state.history_realized_pnl,
        ),
        outer[2],
    );

    // Order history panel
    frame.render_widget(
        OrderHistoryPanel::new(&state.open_order_history, &state.filled_order_history),
        outer[3],
    );

    // System log panel
    frame.render_widget(LogPanel::new(&state.log_messages), outer[4]);

    // Keybind bar
    frame.render_widget(KeybindBar, outer[5]);

    if state.symbol_selector_open {
        render_selector_popup(
            frame,
            " Select Symbol ",
            &state.symbol_items,
            state.symbol_selector_index,
            None,
            None,
        );
    } else if state.strategy_selector_open {
        render_selector_popup(
            frame,
            " Select Strategy ",
            &state.strategy_items,
            state.strategy_selector_index,
            Some(&state.strategy_stats),
            Some(OrderHistoryStats {
                trade_count: state.history_trade_count,
                win_count: state.history_win_count,
                lose_count: state.history_lose_count,
                realized_pnl: state.history_realized_pnl,
            }),
        );
    } else if state.account_popup_open {
        render_account_popup(frame, &state.balances);
    } else if state.history_popup_open {
        render_history_popup(frame, &state.history_rows, state.history_bucket);
    } else if state.focus_popup_open {
        render_focus_popup(frame, state);
    } else if state.v2_grid_open {
        render_v2_grid_popup(frame, state);
    }
}

fn render_focus_popup(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let popup = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2).max(70),
        height: area.height.saturating_sub(2).max(22),
    };
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Focus View (V2 Drill-down) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(7),
        ])
        .split(inner);

    let focus_symbol = state
        .v2_state
        .focus
        .symbol
        .as_deref()
        .unwrap_or(&state.symbol);
    let focus_strategy = state
        .v2_state
        .focus
        .strategy_id
        .as_deref()
        .unwrap_or(&state.strategy_label);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Symbol: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    focus_symbol,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  Strategy: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    focus_strategy,
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(Span::styled(
                "Reuse legacy chart/position/history widgets. Press [F]/[Esc] to close.",
                Style::default().fg(Color::DarkGray),
            )),
        ]),
        rows[0],
    );

    let main_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(48), Constraint::Length(28)])
        .split(rows[1]);

    frame.render_widget(
        PriceChart::new(&state.candles, focus_symbol)
            .current_candle(state.current_candle.as_ref())
            .fill_markers(&state.fill_markers)
            .fast_sma(state.fast_sma)
            .slow_sma(state.slow_sma),
        main_cols[0],
    );
    frame.render_widget(
        PositionPanel::new(
            &state.position,
            state.last_price(),
            &state.balances,
            state.initial_equity_usdt,
            state.current_equity_usdt,
            state.history_trade_count,
            state.history_realized_pnl,
            &state.last_applied_fee,
        ),
        main_cols[1],
    );

    frame.render_widget(
        OrderHistoryPanel::new(&state.open_order_history, &state.filled_order_history),
        rows[2],
    );
}

fn render_v2_grid_popup(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let popup = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2).max(60),
        height: area.height.saturating_sub(2).max(20),
    };
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Portfolio Grid (V2) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Min(4),
        ])
        .split(inner);

    let mut asset_lines = vec![Line::from(Span::styled(
        "Asset Table",
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    ))];
    for a in &state.v2_state.assets {
        asset_lines.push(Line::from(format!(
            "{}  px={} qty={:.5}  rlz={:+.4}  unrlz={:+.4}",
            a.symbol,
            a.last_price
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "---".to_string()),
            a.position_qty,
            a.realized_pnl_usdt,
            a.unrealized_pnl_usdt
        )));
    }
    frame.render_widget(Paragraph::new(asset_lines), chunks[0]);

    let mut strategy_lines = vec![Line::from(Span::styled(
        "Strategy Table",
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    ))];
    for s in &state.v2_state.strategies {
        strategy_lines.push(Line::from(format!(
            "{}  W:{} L:{} T:{}  PnL:{:+.4}",
            s.strategy_id, s.win_count, s.lose_count, s.trade_count, s.realized_pnl_usdt
        )));
    }
    frame.render_widget(Paragraph::new(strategy_lines), chunks[1]);

    let heat = format!(
        "Risk/Rate Heatmap  global {}/{} | orders {}/{} | account {}/{} | mkt {}/{}",
        state.rate_budget_global.used,
        state.rate_budget_global.limit,
        state.rate_budget_orders.used,
        state.rate_budget_orders.limit,
        state.rate_budget_account.used,
        state.rate_budget_account.limit,
        state.rate_budget_market_data.used,
        state.rate_budget_market_data.limit
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Risk/Rate Heatmap",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(heat),
        ]),
        chunks[2],
    );

    let mut rejection_lines = vec![Line::from(Span::styled(
        "Rejection Stream",
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    ))];
    let recent_rejections: Vec<&String> = state
        .log_messages
        .iter()
        .filter(|m| m.contains("[ERR] Rejected"))
        .rev()
        .take(20)
        .collect();
    for msg in recent_rejections.into_iter().rev() {
        rejection_lines.push(Line::from(Span::styled(
            msg.as_str(),
            Style::default().fg(Color::Red),
        )));
    }
    if rejection_lines.len() == 1 {
        rejection_lines.push(Line::from(Span::styled(
            "(no rejections yet)",
            Style::default().fg(Color::DarkGray),
        )));
    }
    frame.render_widget(Paragraph::new(rejection_lines), chunks[3]);
}

fn render_account_popup(frame: &mut Frame, balances: &HashMap<String, f64>) {
    let area = frame.area();
    let popup = Rect {
        x: area.x + 4,
        y: area.y + 2,
        width: area.width.saturating_sub(8).max(30),
        height: area.height.saturating_sub(4).max(10),
    };
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Account Assets ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut assets: Vec<(&String, &f64)> = balances.iter().collect();
    assets.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut lines = Vec::with_capacity(assets.len() + 2);
    lines.push(Line::from(vec![
        Span::styled(
            "Asset",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "      Free",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    for (asset, qty) in assets {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<8}", asset), Style::default().fg(Color::White)),
            Span::styled(format!("{:>14.8}", qty), Style::default().fg(Color::Yellow)),
        ]));
    }
    if lines.len() == 1 {
        lines.push(Line::from(Span::styled(
            "No assets",
            Style::default().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_history_popup(frame: &mut Frame, rows: &[String], bucket: order_store::HistoryBucket) {
    let area = frame.area();
    let popup = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4).max(40),
        height: area.height.saturating_sub(2).max(12),
    };
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(match bucket {
            order_store::HistoryBucket::Day => " History (Day ROI) ",
            order_store::HistoryBucket::Hour => " History (Hour ROI) ",
            order_store::HistoryBucket::Month => " History (Month ROI) ",
        })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let max_rows = inner.height.saturating_sub(1) as usize;
    let mut visible: Vec<Line> = Vec::new();
    for (idx, row) in rows.iter().take(max_rows).enumerate() {
        let color = if idx == 0 {
            Color::Cyan
        } else if row.contains('-') && row.contains('%') {
            Color::White
        } else {
            Color::DarkGray
        };
        visible.push(Line::from(Span::styled(
            row.clone(),
            Style::default().fg(color),
        )));
    }
    if visible.is_empty() {
        visible.push(Line::from(Span::styled(
            "No history rows",
            Style::default().fg(Color::DarkGray),
        )));
    }
    frame.render_widget(Paragraph::new(visible), inner);
}

fn render_selector_popup(
    frame: &mut Frame,
    title: &str,
    items: &[String],
    selected: usize,
    stats: Option<&HashMap<String, OrderHistoryStats>>,
    total_stats: Option<OrderHistoryStats>,
) {
    let area = frame.area();
    let available_width = area.width.saturating_sub(2).max(1);
    let width = if stats.is_some() {
        let min_width = 44;
        let preferred = 84;
        preferred
            .min(available_width)
            .max(min_width.min(available_width))
    } else {
        let min_width = 24;
        let preferred = 48;
        preferred
            .min(available_width)
            .max(min_width.min(available_width))
    };
    let available_height = area.height.saturating_sub(2).max(1);
    let desired_height = if stats.is_some() {
        items.len() as u16 + 7
    } else {
        items.len() as u16 + 4
    };
    let height = desired_height
        .min(available_height)
        .max(6.min(available_height));
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    if stats.is_some() {
        lines.push(Line::from(vec![Span::styled(
            "  Strategy           W    L    T    PnL",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]));
    }

    let mut item_lines: Vec<Line> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let item_text = if let Some(stats_map) = stats {
                if let Some(s) = strategy_stats_for_item(stats_map, item) {
                    format!(
                        "{:<16}  W:{:<3} L:{:<3} T:{:<3} PnL:{:.4}",
                        item, s.win_count, s.lose_count, s.trade_count, s.realized_pnl
                    )
                } else {
                    format!("{:<16}  W:0   L:0   T:0   PnL:0.0000", item)
                }
            } else {
                item.clone()
            };
            if idx == selected {
                Line::from(vec![
                    Span::styled("▶ ", Style::default().fg(Color::Yellow)),
                    Span::styled(
                        item_text,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(item_text, Style::default().fg(Color::DarkGray)),
                ])
            }
        })
        .collect();
    lines.append(&mut item_lines);
    if let (Some(stats_map), Some(t)) = (stats, total_stats.as_ref()) {
        let mut strategy_sum = OrderHistoryStats::default();
        for item in items {
            if let Some(s) = strategy_stats_for_item(stats_map, item) {
                strategy_sum.trade_count += s.trade_count;
                strategy_sum.win_count += s.win_count;
                strategy_sum.lose_count += s.lose_count;
                strategy_sum.realized_pnl += s.realized_pnl;
            }
        }
        let manual = subtract_stats(t, &strategy_sum);
        lines.push(Line::from(vec![Span::styled(
            format!(
                "  MANUAL(rest)       W:{:<3} L:{:<3} T:{:<3} PnL:{:.4}",
                manual.win_count, manual.lose_count, manual.trade_count, manual.realized_pnl
            ),
            Style::default().fg(Color::LightBlue),
        )]));
    }
    if let Some(t) = total_stats {
        lines.push(Line::from(vec![Span::styled(
            format!(
                "  TOTAL              W:{:<3} L:{:<3} T:{:<3} PnL:{:.4}",
                t.win_count, t.lose_count, t.trade_count, t.realized_pnl
            ),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().fg(Color::White)),
        inner,
    );
}

fn strategy_stats_for_item<'a>(
    stats_map: &'a HashMap<String, OrderHistoryStats>,
    item: &str,
) -> Option<&'a OrderHistoryStats> {
    if let Some(s) = stats_map.get(item) {
        return Some(s);
    }
    let source_tag = match item {
        "MA(Config)" => Some("cfg"),
        "MA(Fast 5/20)" => Some("fst"),
        "MA(Slow 20/60)" => Some("slw"),
        _ => None,
    };
    source_tag.and_then(|tag| {
        stats_map
            .get(tag)
            .or_else(|| stats_map.get(&tag.to_ascii_uppercase()))
    })
}

fn subtract_stats(total: &OrderHistoryStats, used: &OrderHistoryStats) -> OrderHistoryStats {
    OrderHistoryStats {
        trade_count: total.trade_count.saturating_sub(used.trade_count),
        win_count: total.win_count.saturating_sub(used.win_count),
        lose_count: total.lose_count.saturating_sub(used.lose_count),
        realized_pnl: total.realized_pnl - used.realized_pnl,
    }
}

fn split_symbol_assets(symbol: &str) -> (String, String) {
    const QUOTE_SUFFIXES: [&str; 10] = [
        "USDT", "USDC", "FDUSD", "BUSD", "TUSD", "TRY", "EUR", "BTC", "ETH", "BNB",
    ];
    for q in QUOTE_SUFFIXES {
        if let Some(base) = symbol.strip_suffix(q) {
            if !base.is_empty() {
                return (base.to_string(), q.to_string());
            }
        }
    }
    (symbol.to_string(), String::new())
}

fn format_last_applied_fee(symbol: &str, fills: &[Fill]) -> Option<String> {
    if fills.is_empty() {
        return None;
    }
    let (base_asset, quote_asset) = split_symbol_assets(symbol);
    let mut fee_by_asset: HashMap<String, f64> = HashMap::new();
    let mut notional_quote = 0.0;
    let mut fee_quote_equiv = 0.0;
    let mut quote_convertible = !quote_asset.is_empty();

    for f in fills {
        if f.qty > 0.0 && f.price > 0.0 {
            notional_quote += f.qty * f.price;
        }
        if f.commission <= 0.0 {
            continue;
        }
        *fee_by_asset.entry(f.commission_asset.clone()).or_insert(0.0) += f.commission;
        if !quote_asset.is_empty() && f.commission_asset.eq_ignore_ascii_case(&quote_asset) {
            fee_quote_equiv += f.commission;
        } else if !base_asset.is_empty() && f.commission_asset.eq_ignore_ascii_case(&base_asset) {
            fee_quote_equiv += f.commission * f.price.max(0.0);
        } else {
            quote_convertible = false;
        }
    }

    if fee_by_asset.is_empty() {
        return Some("0".to_string());
    }

    if quote_convertible && notional_quote > f64::EPSILON {
        let fee_pct = fee_quote_equiv / notional_quote * 100.0;
        return Some(format!(
            "{:.3}% ({:.4} {})",
            fee_pct, fee_quote_equiv, quote_asset
        ));
    }

    let mut items: Vec<(String, f64)> = fee_by_asset.into_iter().collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    if items.len() == 1 {
        let (asset, amount) = &items[0];
        Some(format!("{:.6} {}", amount, asset))
    } else {
        Some(format!("mixed fees ({})", items.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::format_last_applied_fee;
    use crate::model::order::Fill;

    #[test]
    fn fee_summary_from_quote_asset_commission() {
        let fills = vec![Fill {
            price: 2000.0,
            qty: 0.5,
            commission: 1.0,
            commission_asset: "USDT".to_string(),
        }];
        let summary = format_last_applied_fee("ETHUSDT", &fills).unwrap();
        assert_eq!(summary, "0.100% (1.0000 USDT)");
    }

    #[test]
    fn fee_summary_from_base_asset_commission() {
        let fills = vec![Fill {
            price: 2000.0,
            qty: 0.5,
            commission: 0.0005,
            commission_asset: "ETH".to_string(),
        }];
        let summary = format_last_applied_fee("ETHUSDT", &fills).unwrap();
        assert_eq!(summary, "0.100% (1.0000 USDT)");
    }
}
