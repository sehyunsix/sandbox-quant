pub mod ui_projection;
pub mod chart;
pub mod dashboard;
pub mod network_metrics;

use std::collections::HashMap;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table};
use ratatui::Frame;

use crate::event::{
    AppEvent, AssetPnlEntry, EvSnapshotEntry, ExitPolicyEntry, LogDomain, LogLevel, LogRecord,
    WsConnectionStatus,
};
use crate::model::candle::{Candle, CandleBuilder};
use crate::model::order::{Fill, OrderSide};
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_manager::{OrderHistoryFill, OrderHistoryStats, OrderUpdate};
use crate::order_store;
use crate::risk_module::RateBudgetSnapshot;
use crate::strategy_catalog::{strategy_kind_categories, strategy_kind_labels};
use crate::ui::network_metrics::{classify_health, count_since, percentile, rate_per_sec, ratio_pct, NetworkHealth};

use ui_projection::UiProjection;
use ui_projection::AssetEntry;
use chart::{FillMarker, PriceChart};
use dashboard::{KeybindBar, LogPanel, OrderHistoryPanel, OrderLogPanel, PositionPanel, StatusBar, StrategyMetricsPanel};

const MAX_LOG_MESSAGES: usize = 200;
const MAX_FILL_MARKERS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridTab {
    Assets,
    Strategies,
    Risk,
    Network,
    History,
    SystemLog,
}

#[derive(Debug, Clone)]
pub struct StrategyLastEvent {
    pub side: OrderSide,
    pub price: Option<f64>,
    pub timestamp_ms: u64,
    pub is_filled: bool,
}

#[derive(Debug, Clone)]
pub struct ViewState {
    pub is_grid_open: bool,
    pub selected_grid_tab: GridTab,
    pub selected_symbol_index: usize,
    pub selected_strategy_index: usize,
    pub is_on_panel_selected: bool,
    pub is_symbol_selector_open: bool,
    pub selected_symbol_selector_index: usize,
    pub is_strategy_selector_open: bool,
    pub selected_strategy_selector_index: usize,
    pub is_account_popup_open: bool,
    pub is_history_popup_open: bool,
    pub is_focus_popup_open: bool,
    pub is_strategy_editor_open: bool,
}

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
    pub log_records: Vec<LogRecord>,
    pub balances: HashMap<String, f64>,
    pub initial_equity_usdt: Option<f64>,
    pub current_equity_usdt: Option<f64>,
    pub history_estimated_total_pnl_usdt: Option<f64>,
    pub fill_markers: Vec<FillMarker>,
    pub history_trade_count: u32,
    pub history_win_count: u32,
    pub history_lose_count: u32,
    pub history_realized_pnl: f64,
    pub asset_pnl_by_symbol: HashMap<String, AssetPnlEntry>,
    pub strategy_stats: HashMap<String, OrderHistoryStats>,
    pub ev_snapshot_by_scope: HashMap<String, EvSnapshotEntry>,
    pub exit_policy_by_scope: HashMap<String, ExitPolicyEntry>,
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
    pub strategy_item_symbols: Vec<String>,
    pub strategy_item_active: Vec<bool>,
    pub strategy_item_created_at_ms: Vec<i64>,
    pub strategy_item_total_running_ms: Vec<u64>,
    pub account_popup_open: bool,
    pub history_popup_open: bool,
    pub focus_popup_open: bool,
    pub strategy_editor_open: bool,
    pub strategy_editor_kind_category_selector_open: bool,
    pub strategy_editor_kind_selector_open: bool,
    pub strategy_editor_index: usize,
    pub strategy_editor_field: usize,
    pub strategy_editor_kind_category_items: Vec<String>,
    pub strategy_editor_kind_category_index: usize,
    pub strategy_editor_kind_popup_items: Vec<String>,
    pub strategy_editor_kind_popup_labels: Vec<Option<String>>,
    pub strategy_editor_kind_items: Vec<String>,
    pub strategy_editor_kind_selector_index: usize,
    pub strategy_editor_kind_index: usize,
    pub strategy_editor_symbol_index: usize,
    pub strategy_editor_fast: usize,
    pub strategy_editor_slow: usize,
    pub strategy_editor_cooldown: u64,
    pub grid_symbol_index: usize,
    pub grid_strategy_index: usize,
    pub grid_select_on_panel: bool,
    pub grid_tab: GridTab,
    pub strategy_last_event_by_tag: HashMap<String, StrategyLastEvent>,
    pub network_tick_drop_count: u64,
    pub network_reconnect_count: u64,
    pub network_tick_latencies_ms: Vec<u64>,
    pub network_fill_latencies_ms: Vec<u64>,
    pub network_order_sync_latencies_ms: Vec<u64>,
    pub network_tick_in_timestamps_ms: Vec<u64>,
    pub network_tick_drop_timestamps_ms: Vec<u64>,
    pub network_reconnect_timestamps_ms: Vec<u64>,
    pub network_disconnect_timestamps_ms: Vec<u64>,
    pub network_last_fill_ms: Option<u64>,
    pub network_pending_submit_ms_by_intent: HashMap<String, u64>,
    pub history_rows: Vec<String>,
    pub history_bucket: order_store::HistoryBucket,
    pub last_applied_fee: String,
    pub grid_open: bool,
    pub ui_projection: UiProjection,
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
            log_records: Vec::new(),
            balances: HashMap::new(),
            initial_equity_usdt: None,
            current_equity_usdt: None,
            history_estimated_total_pnl_usdt: None,
            fill_markers: Vec::new(),
            history_trade_count: 0,
            history_win_count: 0,
            history_lose_count: 0,
            history_realized_pnl: 0.0,
            asset_pnl_by_symbol: HashMap::new(),
            strategy_stats: HashMap::new(),
            ev_snapshot_by_scope: HashMap::new(),
            exit_policy_by_scope: HashMap::new(),
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
                "RSA(RSI 14 30/70)".to_string(),
            ],
            strategy_item_symbols: vec![
                symbol.to_ascii_uppercase(),
                symbol.to_ascii_uppercase(),
                symbol.to_ascii_uppercase(),
                symbol.to_ascii_uppercase(),
            ],
            strategy_item_active: vec![false, false, false, false],
            strategy_item_created_at_ms: vec![0, 0, 0, 0],
            strategy_item_total_running_ms: vec![0, 0, 0, 0],
            account_popup_open: false,
            history_popup_open: false,
            focus_popup_open: false,
            strategy_editor_open: false,
            strategy_editor_kind_category_selector_open: false,
            strategy_editor_kind_selector_open: false,
            strategy_editor_index: 0,
            strategy_editor_field: 0,
            strategy_editor_kind_category_items: strategy_kind_categories(),
            strategy_editor_kind_category_index: 0,
            strategy_editor_kind_popup_items: Vec::new(),
            strategy_editor_kind_popup_labels: Vec::new(),
            strategy_editor_kind_items: strategy_kind_labels(),
            strategy_editor_kind_selector_index: 0,
            strategy_editor_kind_index: 0,
            strategy_editor_symbol_index: 0,
            strategy_editor_fast: 5,
            strategy_editor_slow: 20,
            strategy_editor_cooldown: 1,
            grid_symbol_index: 0,
            grid_strategy_index: 0,
            grid_select_on_panel: true,
            grid_tab: GridTab::Strategies,
            strategy_last_event_by_tag: HashMap::new(),
            network_tick_drop_count: 0,
            network_reconnect_count: 0,
            network_tick_latencies_ms: Vec::new(),
            network_fill_latencies_ms: Vec::new(),
            network_order_sync_latencies_ms: Vec::new(),
            network_tick_in_timestamps_ms: Vec::new(),
            network_tick_drop_timestamps_ms: Vec::new(),
            network_reconnect_timestamps_ms: Vec::new(),
            network_disconnect_timestamps_ms: Vec::new(),
            network_last_fill_ms: None,
            network_pending_submit_ms_by_intent: HashMap::new(),
            history_rows: Vec::new(),
            history_bucket: order_store::HistoryBucket::Day,
            last_applied_fee: "---".to_string(),
            grid_open: false,
            ui_projection: UiProjection::new(),
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

    pub fn push_log_record(&mut self, record: LogRecord) {
        self.log_records.push(record.clone());
        if self.log_records.len() > MAX_LOG_MESSAGES {
            self.log_records.remove(0);
        }
        self.push_log(format_log_record_compact(&record));
    }

    fn push_latency_sample(samples: &mut Vec<u64>, value: u64) {
        const MAX_SAMPLES: usize = 200;
        samples.push(value);
        if samples.len() > MAX_SAMPLES {
            let drop_n = samples.len() - MAX_SAMPLES;
            samples.drain(..drop_n);
        }
    }

    fn push_network_event_sample(samples: &mut Vec<u64>, ts_ms: u64) {
        samples.push(ts_ms);
        let lower = ts_ms.saturating_sub(60_000);
        samples.retain(|&v| v >= lower);
    }

    fn prune_network_event_windows(&mut self, now_ms: u64) {
        let lower = now_ms.saturating_sub(60_000);
        self.network_tick_in_timestamps_ms.retain(|&v| v >= lower);
        self.network_tick_drop_timestamps_ms.retain(|&v| v >= lower);
        self.network_reconnect_timestamps_ms.retain(|&v| v >= lower);
        self.network_disconnect_timestamps_ms.retain(|&v| v >= lower);
    }

    /// Transitional projection for RFC-0016 Phase 2.
    /// Keeps runtime behavior unchanged while exposing normalized naming.
    pub fn view_state(&self) -> ViewState {
        ViewState {
            is_grid_open: self.grid_open,
            selected_grid_tab: self.grid_tab,
            selected_symbol_index: self.grid_symbol_index,
            selected_strategy_index: self.grid_strategy_index,
            is_on_panel_selected: self.grid_select_on_panel,
            is_symbol_selector_open: self.symbol_selector_open,
            selected_symbol_selector_index: self.symbol_selector_index,
            is_strategy_selector_open: self.strategy_selector_open,
            selected_strategy_selector_index: self.strategy_selector_index,
            is_account_popup_open: self.account_popup_open,
            is_history_popup_open: self.history_popup_open,
            is_focus_popup_open: self.focus_popup_open,
            is_strategy_editor_open: self.strategy_editor_open,
        }
    }

    pub fn is_grid_open(&self) -> bool {
        self.grid_open
    }
    pub fn set_grid_open(&mut self, open: bool) {
        self.grid_open = open;
    }
    pub fn grid_tab(&self) -> GridTab {
        self.grid_tab
    }
    pub fn set_grid_tab(&mut self, tab: GridTab) {
        self.grid_tab = tab;
    }
    pub fn selected_grid_symbol_index(&self) -> usize {
        self.grid_symbol_index
    }
    pub fn set_selected_grid_symbol_index(&mut self, idx: usize) {
        self.grid_symbol_index = idx;
    }
    pub fn selected_grid_strategy_index(&self) -> usize {
        self.grid_strategy_index
    }
    pub fn set_selected_grid_strategy_index(&mut self, idx: usize) {
        self.grid_strategy_index = idx;
    }
    pub fn is_on_panel_selected(&self) -> bool {
        self.grid_select_on_panel
    }
    pub fn set_on_panel_selected(&mut self, selected: bool) {
        self.grid_select_on_panel = selected;
    }
    pub fn is_symbol_selector_open(&self) -> bool {
        self.symbol_selector_open
    }
    pub fn set_symbol_selector_open(&mut self, open: bool) {
        self.symbol_selector_open = open;
    }
    pub fn symbol_selector_index(&self) -> usize {
        self.symbol_selector_index
    }
    pub fn set_symbol_selector_index(&mut self, idx: usize) {
        self.symbol_selector_index = idx;
    }
    pub fn is_strategy_selector_open(&self) -> bool {
        self.strategy_selector_open
    }
    pub fn set_strategy_selector_open(&mut self, open: bool) {
        self.strategy_selector_open = open;
    }
    pub fn strategy_selector_index(&self) -> usize {
        self.strategy_selector_index
    }
    pub fn set_strategy_selector_index(&mut self, idx: usize) {
        self.strategy_selector_index = idx;
    }
    pub fn is_account_popup_open(&self) -> bool {
        self.account_popup_open
    }
    pub fn set_account_popup_open(&mut self, open: bool) {
        self.account_popup_open = open;
    }
    pub fn is_history_popup_open(&self) -> bool {
        self.history_popup_open
    }
    pub fn set_history_popup_open(&mut self, open: bool) {
        self.history_popup_open = open;
    }
    pub fn is_focus_popup_open(&self) -> bool {
        self.focus_popup_open
    }
    pub fn set_focus_popup_open(&mut self, open: bool) {
        self.focus_popup_open = open;
    }
    pub fn is_strategy_editor_open(&self) -> bool {
        self.strategy_editor_open
    }
    pub fn set_strategy_editor_open(&mut self, open: bool) {
        self.strategy_editor_open = open;
    }
    pub fn focus_symbol(&self) -> Option<&str> {
        self.ui_projection.focus.symbol.as_deref()
    }
    pub fn focus_strategy_id(&self) -> Option<&str> {
        self.ui_projection.focus.strategy_id.as_deref()
    }
    pub fn set_focus_symbol(&mut self, symbol: Option<String>) {
        self.ui_projection.focus.symbol = symbol;
    }
    pub fn set_focus_strategy_id(&mut self, strategy_id: Option<String>) {
        self.ui_projection.focus.strategy_id = strategy_id;
    }
    pub fn focus_pair(&self) -> (Option<String>, Option<String>) {
        (
            self.ui_projection.focus.symbol.clone(),
            self.ui_projection.focus.strategy_id.clone(),
        )
    }
    pub fn assets_view(&self) -> &[AssetEntry] {
        &self.ui_projection.assets
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

    fn sync_projection_portfolio_summary(&mut self) {
        self.ui_projection.portfolio.total_equity_usdt = self.current_equity_usdt;
        self.ui_projection.portfolio.total_realized_pnl_usdt = self.history_realized_pnl;
        self.ui_projection.portfolio.total_unrealized_pnl_usdt = self.position.unrealized_pnl;
        self.ui_projection.portfolio.ws_connected = self.ws_connected;
    }

    fn ensure_projection_focus_defaults(&mut self) {
        if self.ui_projection.focus.symbol.is_none() {
            self.ui_projection.focus.symbol = Some(self.symbol.clone());
        }
        if self.ui_projection.focus.strategy_id.is_none() {
            self.ui_projection.focus.strategy_id = Some(self.strategy_label.clone());
        }
    }

    fn rebuild_projection_preserve_focus(&mut self, prev_focus: (Option<String>, Option<String>)) {
        let mut next = UiProjection::from_legacy(self);
        if prev_focus.0.is_some() {
            next.focus.symbol = prev_focus.0;
        }
        if prev_focus.1.is_some() {
            next.focus.strategy_id = prev_focus.1;
        }
        self.ui_projection = next;
        self.ensure_projection_focus_defaults();
    }

    pub fn apply(&mut self, event: AppEvent) {
        let prev_focus = self.focus_pair();
        let mut rebuild_projection = false;
        match event {
            AppEvent::MarketTick(tick) => {
                rebuild_projection = true;
                self.tick_count += 1;
                let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                self.last_price_update_ms = Some(now_ms);
                self.last_price_event_ms = Some(tick.timestamp_ms);
                self.last_price_latency_ms = Some(now_ms.saturating_sub(tick.timestamp_ms));
                Self::push_network_event_sample(&mut self.network_tick_in_timestamps_ms, now_ms);
                if let Some(lat) = self.last_price_latency_ms {
                    Self::push_latency_sample(&mut self.network_tick_latencies_ms, lat);
                }

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
            AppEvent::StrategySignal {
                ref signal,
                symbol,
                source_tag,
                price,
                timestamp_ms,
            } => {
                self.last_signal = Some(signal.clone());
                let source_tag = source_tag.to_ascii_lowercase();
                match signal {
                    Signal::Buy { .. } => {
                        let should_emit = self
                            .strategy_last_event_by_tag
                            .get(&source_tag)
                            .map(|e| e.side != OrderSide::Buy || timestamp_ms.saturating_sub(e.timestamp_ms) >= 1000)
                            .unwrap_or(true);
                        if should_emit {
                            let mut record = LogRecord::new(
                                LogLevel::Info,
                                LogDomain::Strategy,
                                "signal.emit",
                                format!(
                                    "side=BUY price={}",
                                    price
                                        .map(|v| format!("{:.4}", v))
                                        .unwrap_or_else(|| "-".to_string())
                                ),
                            );
                            record.symbol = Some(symbol.clone());
                            record.strategy_tag = Some(source_tag.clone());
                            self.push_log_record(record);
                        }
                        self.strategy_last_event_by_tag.insert(
                            source_tag.clone(),
                            StrategyLastEvent {
                                side: OrderSide::Buy,
                                price,
                                timestamp_ms,
                                is_filled: false,
                            },
                        );
                    }
                    Signal::Sell { .. } => {
                        let should_emit = self
                            .strategy_last_event_by_tag
                            .get(&source_tag)
                            .map(|e| e.side != OrderSide::Sell || timestamp_ms.saturating_sub(e.timestamp_ms) >= 1000)
                            .unwrap_or(true);
                        if should_emit {
                            let mut record = LogRecord::new(
                                LogLevel::Info,
                                LogDomain::Strategy,
                                "signal.emit",
                                format!(
                                    "side=SELL price={}",
                                    price
                                        .map(|v| format!("{:.4}", v))
                                        .unwrap_or_else(|| "-".to_string())
                                ),
                            );
                            record.symbol = Some(symbol.clone());
                            record.strategy_tag = Some(source_tag.clone());
                            self.push_log_record(record);
                        }
                        self.strategy_last_event_by_tag.insert(
                            source_tag.clone(),
                            StrategyLastEvent {
                                side: OrderSide::Sell,
                                price,
                                timestamp_ms,
                                is_filled: false,
                            },
                        );
                    }
                    Signal::Hold => {}
                }
            }
            AppEvent::StrategyState { fast_sma, slow_sma } => {
                self.fast_sma = fast_sma;
                self.slow_sma = slow_sma;
            }
            AppEvent::OrderUpdate(ref update) => {
                rebuild_projection = true;
                match update {
                    OrderUpdate::Filled {
                        intent_id,
                        client_order_id,
                        side,
                        fills,
                        avg_price,
                    } => {
                        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                        let source_tag = parse_source_tag_from_client_order_id(client_order_id)
                            .map(|s| s.to_ascii_lowercase());
                        if let Some(submit_ms) = self.network_pending_submit_ms_by_intent.remove(intent_id)
                        {
                            Self::push_latency_sample(
                                &mut self.network_fill_latencies_ms,
                                now_ms.saturating_sub(submit_ms),
                            );
                        } else if let Some(signal_ms) = source_tag
                            .as_deref()
                            .and_then(|tag| self.strategy_last_event_by_tag.get(tag))
                            .map(|e| e.timestamp_ms)
                        {
                            // Fallback for immediate-fill paths where Submitted is not emitted.
                            Self::push_latency_sample(
                                &mut self.network_fill_latencies_ms,
                                now_ms.saturating_sub(signal_ms),
                            );
                        }
                        self.network_last_fill_ms = Some(now_ms);
                        if let Some(source_tag) = source_tag {
                            self.strategy_last_event_by_tag.insert(
                                source_tag,
                                StrategyLastEvent {
                                    side: *side,
                                    price: Some(*avg_price),
                                    timestamp_ms: now_ms,
                                    is_filled: true,
                                },
                            );
                        }
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
                        let mut record = LogRecord::new(
                            LogLevel::Info,
                            LogDomain::Order,
                            "fill.received",
                            format!(
                                "side={} client_order_id={} intent_id={} avg_price={:.2}",
                                side, client_order_id, intent_id, avg_price
                            ),
                        );
                        record.symbol = Some(self.symbol.clone());
                        record.strategy_tag =
                            parse_source_tag_from_client_order_id(client_order_id).map(|s| s.to_ascii_lowercase());
                        self.push_log_record(record);
                    }
                    OrderUpdate::Submitted {
                        intent_id,
                        client_order_id,
                        server_order_id,
                    } => {
                        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                        self.network_pending_submit_ms_by_intent
                            .insert(intent_id.clone(), now_ms);
                        self.refresh_equity_usdt();
                        let mut record = LogRecord::new(
                            LogLevel::Info,
                            LogDomain::Order,
                            "submit.accepted",
                            format!(
                                "client_order_id={} server_order_id={} intent_id={}",
                                client_order_id, server_order_id, intent_id
                            ),
                        );
                        record.symbol = Some(self.symbol.clone());
                        record.strategy_tag =
                            parse_source_tag_from_client_order_id(client_order_id).map(|s| s.to_ascii_lowercase());
                        self.push_log_record(record);
                    }
                    OrderUpdate::Rejected {
                        intent_id,
                        client_order_id,
                        reason_code,
                        reason,
                    } => {
                        let level = if reason_code == "risk.qty_too_small" {
                            LogLevel::Warn
                        } else {
                            LogLevel::Error
                        };
                        let mut record = LogRecord::new(
                            level,
                            LogDomain::Order,
                            "reject.received",
                            format!(
                                "client_order_id={} intent_id={} reason_code={} reason={}",
                                client_order_id, intent_id, reason_code, reason
                            ),
                        );
                        record.symbol = Some(self.symbol.clone());
                        record.strategy_tag =
                            parse_source_tag_from_client_order_id(client_order_id).map(|s| s.to_ascii_lowercase());
                        self.push_log_record(record);
                    }
                }
                self.last_order = Some(update.clone());
            }
            AppEvent::WsStatus(ref status) => match status {
                WsConnectionStatus::Connected => {
                    self.ws_connected = true;
                }
                WsConnectionStatus::Disconnected => {
                    self.ws_connected = false;
                    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                    Self::push_network_event_sample(&mut self.network_disconnect_timestamps_ms, now_ms);
                    self.push_log("[WARN] WebSocket Disconnected".to_string());
                }
                WsConnectionStatus::Reconnecting { attempt, delay_ms } => {
                    self.ws_connected = false;
                    self.network_reconnect_count += 1;
                    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                    Self::push_network_event_sample(&mut self.network_reconnect_timestamps_ms, now_ms);
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
                rebuild_projection = true;
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
                rebuild_projection = true;
                self.balances = balances;
                self.refresh_equity_usdt();
            }
            AppEvent::OrderHistoryUpdate(snapshot) => {
                rebuild_projection = true;
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
                Self::push_latency_sample(
                    &mut self.network_order_sync_latencies_ms,
                    snapshot.fetch_latency_ms,
                );
                self.refresh_history_rows();
            }
            AppEvent::StrategyStatsUpdate { strategy_stats } => {
                rebuild_projection = true;
                self.strategy_stats = strategy_stats;
            }
            AppEvent::EvSnapshotUpdate {
                symbol,
                source_tag,
                ev,
                p_win,
                gate_mode,
                gate_blocked,
            } => {
                let key = strategy_stats_scope_key(&symbol, &source_tag);
                self.ev_snapshot_by_scope.insert(
                    key,
                    EvSnapshotEntry {
                        ev,
                        p_win,
                        gate_mode,
                        gate_blocked,
                        updated_at_ms: chrono::Utc::now().timestamp_millis() as u64,
                    },
                );
            }
            AppEvent::ExitPolicyUpdate {
                symbol,
                source_tag,
                stop_price,
                expected_holding_ms,
                protective_stop_ok,
            } => {
                let key = strategy_stats_scope_key(&symbol, &source_tag);
                self.exit_policy_by_scope.insert(
                    key,
                    ExitPolicyEntry {
                        stop_price,
                        expected_holding_ms,
                        protective_stop_ok,
                        updated_at_ms: chrono::Utc::now().timestamp_millis() as u64,
                    },
                );
            }
            AppEvent::AssetPnlUpdate { by_symbol } => {
                rebuild_projection = true;
                self.asset_pnl_by_symbol = by_symbol;
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
            AppEvent::TickDropped => {
                self.network_tick_drop_count = self.network_tick_drop_count.saturating_add(1);
                let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                Self::push_network_event_sample(&mut self.network_tick_drop_timestamps_ms, now_ms);
            }
            AppEvent::LogRecord(record) => {
                self.push_log_record(record);
            }
            AppEvent::LogMessage(msg) => {
                self.push_log(msg);
            }
            AppEvent::Error(msg) => {
                self.push_log(format!("[ERR] {}", msg));
            }
        }
        self.prune_network_event_windows(chrono::Utc::now().timestamp_millis() as u64);
        self.sync_projection_portfolio_summary();
        if rebuild_projection {
            self.rebuild_projection_preserve_focus(prev_focus);
        } else {
            self.ensure_projection_focus_defaults();
        }
    }
}

pub fn render(frame: &mut Frame, state: &AppState) {
    let view = state.view_state();
    if view.is_grid_open {
        render_grid_popup(frame, state);
        if view.is_strategy_editor_open {
            render_strategy_editor_popup(frame, state);
        }
        return;
    }

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
    let selected_strategy_stats =
        strategy_stats_for_item(&state.strategy_stats, &state.strategy_label, &state.symbol)
        .cloned()
        .unwrap_or_default();

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

    // Right panels: Position (symbol scope) + Strategy metrics (strategy scope).
    let right_panels = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(9), Constraint::Length(8)])
        .split(main_area[1]);
    frame.render_widget(
        PositionPanel::new(
            &state.position,
            current_price,
            &state.last_applied_fee,
            ev_snapshot_for_item(&state.ev_snapshot_by_scope, &state.strategy_label, &state.symbol),
            exit_policy_for_item(&state.exit_policy_by_scope, &state.strategy_label, &state.symbol),
        ),
        right_panels[0],
    );
    frame.render_widget(
        StrategyMetricsPanel::new(
            &state.strategy_label,
            selected_strategy_stats.trade_count,
            selected_strategy_stats.win_count,
            selected_strategy_stats.lose_count,
            selected_strategy_stats.realized_pnl,
        ),
        right_panels[1],
    );

    // Order log
    frame.render_widget(
        OrderLogPanel::new(
            &state.last_signal,
            &state.last_order,
            state.fast_sma,
            state.slow_sma,
            selected_strategy_stats.trade_count,
            selected_strategy_stats.win_count,
            selected_strategy_stats.lose_count,
            selected_strategy_stats.realized_pnl,
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

    if view.is_symbol_selector_open {
        render_selector_popup(
            frame,
            " Select Symbol ",
            &state.symbol_items,
            view.selected_symbol_selector_index,
            None,
            None,
            None,
        );
    } else if view.is_strategy_selector_open {
        let selected_strategy_symbol = state
            .strategy_item_symbols
            .get(view.selected_strategy_selector_index)
            .map(String::as_str)
            .unwrap_or(state.symbol.as_str());
        render_selector_popup(
            frame,
            " Select Strategy ",
            &state.strategy_items,
            view.selected_strategy_selector_index,
            Some(&state.strategy_stats),
            Some(OrderHistoryStats {
                trade_count: state.history_trade_count,
                win_count: state.history_win_count,
                lose_count: state.history_lose_count,
                realized_pnl: state.history_realized_pnl,
            }),
            Some(selected_strategy_symbol),
        );
    } else if view.is_account_popup_open {
        render_account_popup(frame, &state.balances);
    } else if view.is_history_popup_open {
        render_history_popup(frame, &state.history_rows, state.history_bucket);
    } else if view.is_focus_popup_open {
        render_focus_popup(frame, state);
    } else if view.is_strategy_editor_open {
        render_strategy_editor_popup(frame, state);
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
        .title(" Focus View (Drill-down) ")
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

    let focus_symbol = state.focus_symbol().unwrap_or(&state.symbol);
    let focus_strategy = state.focus_strategy_id().unwrap_or(&state.strategy_label);
    let focus_strategy_stats = strategy_stats_for_item(
        &state.strategy_stats,
        focus_strategy,
        focus_symbol,
    )
        .cloned()
        .unwrap_or_default();
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
    let focus_right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(8)])
        .split(main_cols[1]);
    frame.render_widget(
        PositionPanel::new(
            &state.position,
            state.last_price(),
            &state.last_applied_fee,
            ev_snapshot_for_item(&state.ev_snapshot_by_scope, focus_strategy, focus_symbol),
            exit_policy_for_item(&state.exit_policy_by_scope, focus_strategy, focus_symbol),
        ),
        focus_right[0],
    );
    frame.render_widget(
        StrategyMetricsPanel::new(
            focus_strategy,
            focus_strategy_stats.trade_count,
            focus_strategy_stats.win_count,
            focus_strategy_stats.lose_count,
            focus_strategy_stats.realized_pnl,
        ),
        focus_right[1],
    );

    frame.render_widget(
        OrderHistoryPanel::new(&state.open_order_history, &state.filled_order_history),
        rows[2],
    );
}

fn render_grid_popup(frame: &mut Frame, state: &AppState) {
    let view = state.view_state();
    let area = frame.area();
    let popup = area;
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Portfolio Grid ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);
    let tab_area = root[0];
    let body_area = root[1];

    let tab_span = |tab: GridTab, key: &str, label: &str| -> Span<'_> {
        let selected = view.selected_grid_tab == tab;
        Span::styled(
            format!("[{} {}]", key, label),
            if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        )
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            tab_span(GridTab::Assets, "1", "Assets"),
            Span::raw(" "),
            tab_span(GridTab::Strategies, "2", "Strategies"),
            Span::raw(" "),
            tab_span(GridTab::Risk, "3", "Risk"),
            Span::raw(" "),
            tab_span(GridTab::Network, "4", "Network"),
            Span::raw(" "),
            tab_span(GridTab::History, "5", "History"),
            Span::raw(" "),
            tab_span(GridTab::SystemLog, "6", "SystemLog"),
        ])),
        tab_area,
    );

    let global_pressure =
        state.rate_budget_global.used as f64 / (state.rate_budget_global.limit.max(1) as f64);
    let orders_pressure =
        state.rate_budget_orders.used as f64 / (state.rate_budget_orders.limit.max(1) as f64);
    let account_pressure =
        state.rate_budget_account.used as f64 / (state.rate_budget_account.limit.max(1) as f64);
    let market_pressure = state.rate_budget_market_data.used as f64
        / (state.rate_budget_market_data.limit.max(1) as f64);
    let max_pressure = global_pressure
        .max(orders_pressure)
        .max(account_pressure)
        .max(market_pressure);
    let (risk_label, risk_color) = if max_pressure >= 0.90 {
        ("CRIT", Color::Red)
    } else if max_pressure >= 0.70 {
        ("WARN", Color::Yellow)
    } else {
        ("OK", Color::Green)
    };

    if view.selected_grid_tab == GridTab::Assets {
        let spot_assets: Vec<&AssetEntry> = state
            .assets_view()
            .iter()
            .filter(|a| !a.is_futures)
            .collect();
        let fut_assets: Vec<&AssetEntry> = state
            .assets_view()
            .iter()
            .filter(|a| a.is_futures)
            .collect();
        let spot_total_rlz: f64 = spot_assets.iter().map(|a| a.realized_pnl_usdt).sum();
        let spot_total_unrlz: f64 = spot_assets.iter().map(|a| a.unrealized_pnl_usdt).sum();
        let fut_total_rlz: f64 = fut_assets.iter().map(|a| a.realized_pnl_usdt).sum();
        let fut_total_unrlz: f64 = fut_assets.iter().map(|a| a.unrealized_pnl_usdt).sum();
        let total_rlz = spot_total_rlz + fut_total_rlz;
        let total_unrlz = spot_total_unrlz + fut_total_unrlz;
        let total_pnl = total_rlz + total_unrlz;
        let panel_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(46),
                Constraint::Percentage(46),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(body_area);

        let spot_header = Row::new(vec![
            Cell::from("Asset"),
            Cell::from("Qty"),
            Cell::from("Price"),
            Cell::from("RlzPnL"),
            Cell::from("UnrPnL"),
        ])
        .style(Style::default().fg(Color::DarkGray));
        let mut spot_rows: Vec<Row> = spot_assets
            .iter()
            .map(|a| {
                Row::new(vec![
                    Cell::from(a.symbol.clone()),
                    Cell::from(format!("{:.5}", a.position_qty)),
                    Cell::from(
                        a.last_price
                            .map(|v| format!("{:.2}", v))
                            .unwrap_or_else(|| "---".to_string()),
                    ),
                    Cell::from(format!("{:+.4}", a.realized_pnl_usdt)),
                    Cell::from(format!("{:+.4}", a.unrealized_pnl_usdt)),
                ])
            })
            .collect();
        if spot_rows.is_empty() {
            spot_rows.push(
                Row::new(vec![
                    Cell::from("(no spot assets)"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                ])
                .style(Style::default().fg(Color::DarkGray)),
            );
        }
        frame.render_widget(
            Table::new(
                spot_rows,
                [
                    Constraint::Length(16),
                    Constraint::Length(12),
                    Constraint::Length(10),
                    Constraint::Length(10),
                    Constraint::Length(10),
                ],
            )
            .header(spot_header)
            .column_spacing(1)
            .block(
                Block::default()
                    .title(format!(
                        " Spot Assets | Total {} | PnL {:+.4} (R {:+.4} / U {:+.4}) ",
                        spot_assets.len(),
                        spot_total_rlz + spot_total_unrlz,
                        spot_total_rlz,
                        spot_total_unrlz
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            panel_chunks[0],
        );

        let fut_header = Row::new(vec![
            Cell::from("Symbol"),
            Cell::from("Side"),
            Cell::from("PosQty"),
            Cell::from("Entry"),
            Cell::from("RlzPnL"),
            Cell::from("UnrPnL"),
        ])
        .style(Style::default().fg(Color::DarkGray));
        let mut fut_rows: Vec<Row> = fut_assets
            .iter()
            .map(|a| {
                Row::new(vec![
                    Cell::from(a.symbol.clone()),
                    Cell::from(a.side.clone().unwrap_or_else(|| "-".to_string())),
                    Cell::from(format!("{:.5}", a.position_qty)),
                    Cell::from(
                        a.entry_price
                            .map(|v| format!("{:.2}", v))
                            .unwrap_or_else(|| "---".to_string()),
                    ),
                    Cell::from(format!("{:+.4}", a.realized_pnl_usdt)),
                    Cell::from(format!("{:+.4}", a.unrealized_pnl_usdt)),
                ])
            })
            .collect();
        if fut_rows.is_empty() {
            fut_rows.push(
                Row::new(vec![
                    Cell::from("(no futures positions)"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                ])
                .style(Style::default().fg(Color::DarkGray)),
            );
        }
        frame.render_widget(
            Table::new(
                fut_rows,
                [
                    Constraint::Length(18),
                    Constraint::Length(8),
                    Constraint::Length(10),
                    Constraint::Length(10),
                    Constraint::Length(10),
                    Constraint::Length(10),
                ],
            )
            .header(fut_header)
            .column_spacing(1)
            .block(
                Block::default()
                    .title(format!(
                        " Futures Positions | Total {} | PnL {:+.4} (R {:+.4} / U {:+.4}) ",
                        fut_assets.len(),
                        fut_total_rlz + fut_total_unrlz,
                        fut_total_rlz,
                        fut_total_unrlz
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            panel_chunks[1],
        );
        let total_color = if total_pnl > 0.0 {
            Color::Green
        } else if total_pnl < 0.0 {
            Color::Red
        } else {
            Color::DarkGray
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Total PnL: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:+.4}", total_pnl),
                    Style::default().fg(total_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("   Realized: {:+.4}   Unrealized: {:+.4}", total_rlz, total_unrlz),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            panel_chunks[2],
        );
        frame.render_widget(Paragraph::new("[1/2/3/4/5/6] tab  [G/Esc] close"), panel_chunks[3]);
        return;
    }

    if view.selected_grid_tab == GridTab::Risk {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(4),
                Constraint::Min(3),
                Constraint::Length(1),
            ])
            .split(body_area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Risk: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    risk_label,
                    Style::default().fg(risk_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  (70%=WARN, 90%=CRIT)",
                    Style::default().fg(Color::DarkGray),
                ),
            ])),
            chunks[0],
        );
        let risk_rows = vec![
            Row::new(vec![
                Cell::from("GLOBAL"),
                Cell::from(format!(
                    "{}/{}",
                    state.rate_budget_global.used, state.rate_budget_global.limit
                )),
                Cell::from(format!("{}ms", state.rate_budget_global.reset_in_ms)),
            ]),
            Row::new(vec![
                Cell::from("ORDERS"),
                Cell::from(format!(
                    "{}/{}",
                    state.rate_budget_orders.used, state.rate_budget_orders.limit
                )),
                Cell::from(format!("{}ms", state.rate_budget_orders.reset_in_ms)),
            ]),
            Row::new(vec![
                Cell::from("ACCOUNT"),
                Cell::from(format!(
                    "{}/{}",
                    state.rate_budget_account.used, state.rate_budget_account.limit
                )),
                Cell::from(format!("{}ms", state.rate_budget_account.reset_in_ms)),
            ]),
            Row::new(vec![
                Cell::from("MARKET"),
                Cell::from(format!(
                    "{}/{}",
                    state.rate_budget_market_data.used, state.rate_budget_market_data.limit
                )),
                Cell::from(format!("{}ms", state.rate_budget_market_data.reset_in_ms)),
            ]),
        ];
        frame.render_widget(
            Table::new(
                risk_rows,
                [
                    Constraint::Length(10),
                    Constraint::Length(16),
                    Constraint::Length(12),
                ],
            )
            .header(Row::new(vec![
                Cell::from("Group"),
                Cell::from("Used/Limit"),
                Cell::from("Reset In"),
            ]))
            .column_spacing(1)
            .block(
                Block::default()
                    .title(" Risk Budgets ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            chunks[1],
        );
        let recent_rejections: Vec<&String> = state
            .log_messages
            .iter()
            .filter(|m| m.contains("order.reject.received"))
            .rev()
            .take(20)
            .collect();
        let mut lines = vec![Line::from(Span::styled(
            "Recent Rejections",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))];
        for msg in recent_rejections.into_iter().rev() {
            lines.push(Line::from(Span::styled(
                msg.as_str(),
                Style::default().fg(Color::Red),
            )));
        }
        if lines.len() == 1 {
            lines.push(Line::from(Span::styled(
                "(no rejections yet)",
                Style::default().fg(Color::DarkGray),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            chunks[2],
        );
        frame.render_widget(Paragraph::new("[1/2/3/4/5/6] tab  [G/Esc] close"), chunks[3]);
        return;
    }

    if view.selected_grid_tab == GridTab::Network {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let tick_in_1s = count_since(&state.network_tick_in_timestamps_ms, now_ms, 1_000);
        let tick_in_10s = count_since(&state.network_tick_in_timestamps_ms, now_ms, 10_000);
        let tick_in_60s = count_since(&state.network_tick_in_timestamps_ms, now_ms, 60_000);
        let tick_drop_1s = count_since(&state.network_tick_drop_timestamps_ms, now_ms, 1_000);
        let tick_drop_10s = count_since(&state.network_tick_drop_timestamps_ms, now_ms, 10_000);
        let tick_drop_60s = count_since(&state.network_tick_drop_timestamps_ms, now_ms, 60_000);
        let reconnect_60s = count_since(&state.network_reconnect_timestamps_ms, now_ms, 60_000);
        let disconnect_60s = count_since(&state.network_disconnect_timestamps_ms, now_ms, 60_000);

        let tick_in_rate_1s = rate_per_sec(tick_in_1s, 1.0);
        let tick_drop_rate_1s = rate_per_sec(tick_drop_1s, 1.0);
        let tick_drop_rate_10s = rate_per_sec(tick_drop_10s, 10.0);
        let tick_drop_rate_60s = rate_per_sec(tick_drop_60s, 60.0);
        let tick_drop_ratio_10s = ratio_pct(tick_drop_10s, tick_in_10s.saturating_add(tick_drop_10s));
        let tick_drop_ratio_60s = ratio_pct(tick_drop_60s, tick_in_60s.saturating_add(tick_drop_60s));
        let reconnect_rate_60s = reconnect_60s as f64;
        let disconnect_rate_60s = disconnect_60s as f64;
        let heartbeat_gap_ms = state.last_price_update_ms.map(|ts| now_ms.saturating_sub(ts));
        let tick_p95_ms = percentile(&state.network_tick_latencies_ms, 95);
        let health = classify_health(
            state.ws_connected,
            tick_drop_ratio_10s,
            reconnect_rate_60s,
            tick_p95_ms,
            heartbeat_gap_ms,
        );
        let (health_label, health_color) = match health {
            NetworkHealth::Ok => ("OK", Color::Green),
            NetworkHealth::Warn => ("WARN", Color::Yellow),
            NetworkHealth::Crit => ("CRIT", Color::Red),
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(6),
                Constraint::Length(6),
                Constraint::Length(1),
            ])
            .split(body_area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Health: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    health_label,
                    Style::default()
                        .fg(health_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  WS: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    if state.ws_connected {
                        "CONNECTED"
                    } else {
                        "DISCONNECTED"
                    },
                    Style::default().fg(if state.ws_connected {
                        Color::Green
                    } else {
                        Color::Red
                    }),
                ),
                Span::styled(
                    format!(
                        "  in1s={:.1}/s  drop10s={:.2}/s  ratio10s={:.2}%  reconn60s={:.0}/min",
                        tick_in_rate_1s, tick_drop_rate_10s, tick_drop_ratio_10s, reconnect_rate_60s
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ])),
            chunks[0],
        );

        let tick_stats = latency_stats(&state.network_tick_latencies_ms);
        let fill_stats = latency_stats(&state.network_fill_latencies_ms);
        let sync_stats = latency_stats(&state.network_order_sync_latencies_ms);
        let last_fill_age = state
            .network_last_fill_ms
            .map(|ts| format_age_ms(now_ms.saturating_sub(ts)))
            .unwrap_or_else(|| "-".to_string());
        let rows = vec![
            Row::new(vec![
                Cell::from("Tick Latency"),
                Cell::from(tick_stats.0),
                Cell::from(tick_stats.1),
                Cell::from(tick_stats.2),
                Cell::from(
                    state
                        .last_price_latency_ms
                        .map(|v| format!("{}ms", v))
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ]),
            Row::new(vec![
                Cell::from("Fill Latency"),
                Cell::from(fill_stats.0),
                Cell::from(fill_stats.1),
                Cell::from(fill_stats.2),
                Cell::from(last_fill_age),
            ]),
            Row::new(vec![
                Cell::from("Order Sync"),
                Cell::from(sync_stats.0),
                Cell::from(sync_stats.1),
                Cell::from(sync_stats.2),
                Cell::from(
                    state
                        .last_order_history_latency_ms
                        .map(|v| format!("{}ms", v))
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ]),
        ];
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(14),
                    Constraint::Length(12),
                    Constraint::Length(12),
                    Constraint::Length(12),
                    Constraint::Length(14),
                ],
            )
            .header(Row::new(vec![
                Cell::from("Metric"),
                Cell::from("p50"),
                Cell::from("p95"),
                Cell::from("p99"),
                Cell::from("last/age"),
            ]))
            .column_spacing(1)
            .block(
                Block::default()
                    .title(" Network Metrics ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            chunks[1],
        );

        let summary_rows = vec![
            Row::new(vec![
                Cell::from("tick_drop_rate_1s"),
                Cell::from(format!("{:.2}/s", tick_drop_rate_1s)),
                Cell::from("tick_drop_rate_60s"),
                Cell::from(format!("{:.2}/s", tick_drop_rate_60s)),
            ]),
            Row::new(vec![
                Cell::from("drop_ratio_60s"),
                Cell::from(format!("{:.2}%", tick_drop_ratio_60s)),
                Cell::from("disconnect_rate_60s"),
                Cell::from(format!("{:.0}/min", disconnect_rate_60s)),
            ]),
            Row::new(vec![
                Cell::from("last_tick_age"),
                Cell::from(
                    heartbeat_gap_ms
                        .map(format_age_ms)
                        .unwrap_or_else(|| "-".to_string()),
                ),
                Cell::from("last_order_update_age"),
                Cell::from(
                    state
                        .last_order_history_update_ms
                        .map(|ts| format_age_ms(now_ms.saturating_sub(ts)))
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ]),
            Row::new(vec![
                Cell::from("tick_drop_total"),
                Cell::from(state.network_tick_drop_count.to_string()),
                Cell::from("reconnect_total"),
                Cell::from(state.network_reconnect_count.to_string()),
            ]),
        ];
        frame.render_widget(
            Table::new(
                summary_rows,
                [
                    Constraint::Length(20),
                    Constraint::Length(18),
                    Constraint::Length(20),
                    Constraint::Length(18),
                ],
            )
            .column_spacing(1)
            .block(
                Block::default()
                    .title(" Network Summary ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            chunks[2],
        );
        frame.render_widget(Paragraph::new("[1/2/3/4/5/6] tab  [G/Esc] close"), chunks[3]);
        return;
    }

    if view.selected_grid_tab == GridTab::History {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(6), Constraint::Length(1)])
            .split(body_area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Bucket: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    match state.history_bucket {
                        order_store::HistoryBucket::Day => "Day",
                        order_store::HistoryBucket::Hour => "Hour",
                        order_store::HistoryBucket::Month => "Month",
                    },
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  (popup hotkeys: D/H/M)",
                    Style::default().fg(Color::DarkGray),
                ),
            ])),
            chunks[0],
        );

        let visible = build_history_lines(&state.history_rows, chunks[1].height.saturating_sub(2) as usize);
        frame.render_widget(
            Paragraph::new(visible).block(
                Block::default()
                    .title(" History ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            chunks[1],
        );
        frame.render_widget(Paragraph::new("[1/2/3/4/5/6] tab  [G/Esc] close"), chunks[2]);
        return;
    }

    if view.selected_grid_tab == GridTab::SystemLog {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(1)])
            .split(body_area);
        let max_rows = chunks[0].height.saturating_sub(2) as usize;
        let mut log_rows: Vec<Row> = state
            .log_messages
            .iter()
            .rev()
            .take(max_rows.max(1))
            .rev()
            .map(|line| Row::new(vec![Cell::from(line.clone())]))
            .collect();
        if log_rows.is_empty() {
            log_rows.push(
                Row::new(vec![Cell::from("(no system logs yet)")])
                    .style(Style::default().fg(Color::DarkGray)),
            );
        }
        frame.render_widget(
            Table::new(log_rows, [Constraint::Min(1)])
                .header(Row::new(vec![Cell::from("Message")]).style(Style::default().fg(Color::DarkGray)))
                .column_spacing(1)
                .block(
                    Block::default()
                        .title(" System Log ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray)),
                ),
            chunks[0],
        );
        frame.render_widget(Paragraph::new("[1/2/3/4/5/6] tab  [G/Esc] close"), chunks[1]);
        return;
    }

    let selected_symbol = state
        .symbol_items
        .get(view.selected_symbol_index)
        .map(String::as_str)
        .unwrap_or(state.symbol.as_str());
    let strategy_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(1),
        ])
        .split(body_area);

    let mut on_indices: Vec<usize> = Vec::new();
    let mut off_indices: Vec<usize> = Vec::new();
    for idx in 0..state.strategy_items.len() {
        if state
            .strategy_item_active
            .get(idx)
            .copied()
            .unwrap_or(false)
        {
            on_indices.push(idx);
        } else {
            off_indices.push(idx);
        }
    }
    let on_weight = on_indices.len().max(1) as u32;
    let off_weight = off_indices.len().max(1) as u32;

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Risk: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                risk_label,
                Style::default().fg(risk_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  GLOBAL ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{}/{}",
                    state.rate_budget_global.used, state.rate_budget_global.limit
                ),
                Style::default().fg(if global_pressure >= 0.9 {
                    Color::Red
                } else if global_pressure >= 0.7 {
                    Color::Yellow
                } else {
                    Color::Cyan
                }),
            ),
            Span::styled("  ORD ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{}/{}",
                    state.rate_budget_orders.used, state.rate_budget_orders.limit
                ),
                Style::default().fg(if orders_pressure >= 0.9 {
                    Color::Red
                } else if orders_pressure >= 0.7 {
                    Color::Yellow
                } else {
                    Color::Cyan
                }),
            ),
            Span::styled("  ACC ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{}/{}",
                    state.rate_budget_account.used, state.rate_budget_account.limit
                ),
                Style::default().fg(if account_pressure >= 0.9 {
                    Color::Red
                } else if account_pressure >= 0.7 {
                    Color::Yellow
                } else {
                    Color::Cyan
                }),
            ),
            Span::styled("  MKT ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{}/{}",
                    state.rate_budget_market_data.used, state.rate_budget_market_data.limit
                ),
                Style::default().fg(if market_pressure >= 0.9 {
                    Color::Red
                } else if market_pressure >= 0.7 {
                    Color::Yellow
                } else {
                    Color::Cyan
                }),
            ),
        ])),
        strategy_chunks[0],
    );

    let strategy_area = strategy_chunks[2];
    let min_panel_height: u16 = 6;
    let total_height = strategy_area.height;
    let (on_height, off_height) = if total_height >= min_panel_height.saturating_mul(2) {
        let total_weight = on_weight + off_weight;
        let mut on_h =
            ((total_height as u32 * on_weight) / total_weight).max(min_panel_height as u32) as u16;
        let max_on_h = total_height.saturating_sub(min_panel_height);
        if on_h > max_on_h {
            on_h = max_on_h;
        }
        let off_h = total_height.saturating_sub(on_h);
        (on_h, off_h)
    } else {
        let on_h = (total_height / 2).max(1);
        let off_h = total_height.saturating_sub(on_h).max(1);
        (on_h, off_h)
    };
    let on_area = Rect {
        x: strategy_area.x,
        y: strategy_area.y,
        width: strategy_area.width,
        height: on_height,
    };
    let off_area = Rect {
        x: strategy_area.x,
        y: strategy_area.y.saturating_add(on_height),
        width: strategy_area.width,
        height: off_height,
    };

    let pnl_sum_for_indices = |indices: &[usize], state: &AppState| -> f64 {
        indices
            .iter()
            .map(|idx| {
                let item = state
                    .strategy_items
                    .get(*idx)
                    .map(String::as_str)
                    .unwrap_or("-");
                let row_symbol = state
                    .strategy_item_symbols
                    .get(*idx)
                    .map(String::as_str)
                    .unwrap_or(state.symbol.as_str());
                strategy_stats_for_item(&state.strategy_stats, item, row_symbol)
                    .map(|s| s.realized_pnl)
                    .unwrap_or(0.0)
            })
            .sum()
    };
    let on_pnl_sum = pnl_sum_for_indices(&on_indices, state);
    let off_pnl_sum = pnl_sum_for_indices(&off_indices, state);
    let total_pnl_sum = on_pnl_sum + off_pnl_sum;

    let total_row = Row::new(vec![
        Cell::from("ON Total"),
        Cell::from(on_indices.len().to_string()),
        Cell::from(format!("{:+.4}", on_pnl_sum)),
        Cell::from("OFF Total"),
        Cell::from(off_indices.len().to_string()),
        Cell::from(format!("{:+.4}", off_pnl_sum)),
        Cell::from("All Total"),
        Cell::from(format!("{:+.4}", total_pnl_sum)),
    ]);
    let total_table = Table::new(
        vec![total_row],
        [
            Constraint::Length(10),
            Constraint::Length(5),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(5),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(12),
        ],
    )
    .column_spacing(1)
    .block(
        Block::default()
            .title(" Total ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(total_table, strategy_chunks[1]);

    let render_strategy_window = |frame: &mut Frame,
                                  area: Rect,
                                  title: &str,
                                  indices: &[usize],
                                  state: &AppState,
                                  pnl_sum: f64,
                                  selected_panel: bool| {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let inner_height = area.height.saturating_sub(2);
        let row_capacity = inner_height.saturating_sub(1) as usize;
        let selected_pos = indices
            .iter()
            .position(|idx| *idx == view.selected_strategy_index);
        let window_start = if row_capacity == 0 {
            0
        } else if let Some(pos) = selected_pos {
            pos.saturating_sub(row_capacity.saturating_sub(1))
        } else {
            0
        };
        let window_end = if row_capacity == 0 {
            0
        } else {
            (window_start + row_capacity).min(indices.len())
        };
        let visible_indices = if indices.is_empty() || row_capacity == 0 {
            &indices[0..0]
        } else {
            &indices[window_start..window_end]
        };
        let header = Row::new(vec![
            Cell::from(" "),
            Cell::from("Symbol"),
            Cell::from("Strategy"),
            Cell::from("Run"),
            Cell::from("Last"),
            Cell::from("Px"),
            Cell::from("Age"),
            Cell::from("W"),
            Cell::from("L"),
            Cell::from("T"),
            Cell::from("PnL"),
            Cell::from("EV"),
            Cell::from("Gate"),
            Cell::from("Stop"),
        ])
        .style(Style::default().fg(Color::DarkGray));
        let mut rows: Vec<Row> = visible_indices
            .iter()
            .map(|idx| {
                let row_symbol = state
                    .strategy_item_symbols
                    .get(*idx)
                    .map(String::as_str)
                    .unwrap_or("-");
                let item = state
                    .strategy_items
                    .get(*idx)
                    .cloned()
                    .unwrap_or_else(|| "-".to_string());
                let running = state
                    .strategy_item_total_running_ms
                    .get(*idx)
                    .copied()
                    .map(format_running_time)
                    .unwrap_or_else(|| "-".to_string());
                let stats = strategy_stats_for_item(&state.strategy_stats, &item, row_symbol);
                let ev_snapshot = ev_snapshot_for_item(&state.ev_snapshot_by_scope, &item, row_symbol);
                let exit_policy = exit_policy_for_item(&state.exit_policy_by_scope, &item, row_symbol);
                let source_tag = source_tag_for_strategy_item(&item);
                let last_evt = source_tag
                    .as_ref()
                    .and_then(|tag| state.strategy_last_event_by_tag.get(tag));
                let (last_label, last_px, last_age, last_style) = if let Some(evt) = last_evt {
                    let age = now_ms.saturating_sub(evt.timestamp_ms);
                    let age_txt = if age < 1_000 {
                        format!("{}ms", age)
                    } else if age < 60_000 {
                        format!("{}s", age / 1_000)
                    } else {
                        format!("{}m", age / 60_000)
                    };
                    let side_txt = match evt.side {
                        OrderSide::Buy => "BUY",
                        OrderSide::Sell => "SELL",
                    };
                    let px_txt = evt
                        .price
                        .map(|v| format!("{:.2}", v))
                        .unwrap_or_else(|| "-".to_string());
                    let style = match evt.side {
                        OrderSide::Buy => Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                        OrderSide::Sell => {
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                        }
                    };
                    (side_txt.to_string(), px_txt, age_txt, style)
                } else {
                    (
                        "-".to_string(),
                        "-".to_string(),
                        "-".to_string(),
                        Style::default().fg(Color::DarkGray),
                    )
                };
                let (w, l, t, pnl) = if let Some(s) = stats {
                    (
                        s.win_count.to_string(),
                        s.lose_count.to_string(),
                        s.trade_count.to_string(),
                        format!("{:+.4}", s.realized_pnl),
                    )
                } else {
                    (
                        "0".to_string(),
                        "0".to_string(),
                        "0".to_string(),
                        "+0.0000".to_string(),
                    )
                };
                let ev_txt = ev_snapshot
                    .map(|v| format!("{:+.3}", v.ev))
                    .unwrap_or_else(|| "-".to_string());
                let gate_txt = ev_snapshot
                    .map(|v| {
                        if v.gate_blocked {
                            "BLOCK".to_string()
                        } else {
                            v.gate_mode.to_ascii_uppercase()
                        }
                    })
                    .unwrap_or_else(|| "-".to_string());
                let stop_txt = exit_policy
                    .and_then(|p| p.stop_price)
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string());
                let marker = if *idx == view.selected_strategy_index {
                    "▶"
                } else {
                    " "
                };
                let mut row = Row::new(vec![
                    Cell::from(marker),
                    Cell::from(row_symbol.to_string()),
                    Cell::from(item),
                    Cell::from(running),
                    Cell::from(last_label).style(last_style),
                    Cell::from(last_px),
                    Cell::from(last_age),
                    Cell::from(w),
                    Cell::from(l),
                    Cell::from(t),
                    Cell::from(pnl),
                    Cell::from(ev_txt),
                    Cell::from(gate_txt),
                    Cell::from(stop_txt),
                ]);
                if *idx == view.selected_strategy_index {
                    row = row.style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    );
                }
                row
            })
            .collect();

        if rows.is_empty() {
            rows.push(
                Row::new(vec![
                    Cell::from(" "),
                    Cell::from("-"),
                    Cell::from("(empty)"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                    Cell::from("-"),
                ])
                .style(Style::default().fg(Color::DarkGray)),
            );
        }

        let table = Table::new(
            rows,
            [
                Constraint::Length(2),
                Constraint::Length(12),
                Constraint::Min(14),
                Constraint::Length(9),
                Constraint::Length(5),
                Constraint::Length(9),
                Constraint::Length(6),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(4),
                Constraint::Length(11),
                Constraint::Length(8),
                Constraint::Length(7),
                Constraint::Length(10),
            ],
        )
        .header(header)
        .column_spacing(1)
        .block(
            Block::default()
                .title(format!(
                    "{} | Total {:+.4} | {}/{}",
                    title,
                    pnl_sum,
                    visible_indices.len(),
                    indices.len()
                ))
                .borders(Borders::ALL)
                .border_style(if selected_panel {
                    Style::default().fg(Color::Yellow)
                } else if risk_label == "CRIT" {
                    Style::default().fg(Color::Red)
                } else if risk_label == "WARN" {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                }),
        );
        frame.render_widget(table, area);
    };

    render_strategy_window(
        frame,
        on_area,
        " ON Strategies ",
        &on_indices,
        state,
        on_pnl_sum,
        view.is_on_panel_selected,
    );
    render_strategy_window(
        frame,
        off_area,
        " OFF Strategies ",
        &off_indices,
        state,
        off_pnl_sum,
        !view.is_on_panel_selected,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Symbol: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                selected_symbol,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  [1/2/3/4]tab [Tab]panel [N]new [C]cfg [O]on/off [X]del [J/K]strategy [H/L]symbol [Enter/F]run [G/Esc]close",
                Style::default().fg(Color::DarkGray),
            ),
        ])),
        strategy_chunks[3],
    );
}

fn format_running_time(total_running_ms: u64) -> String {
    let total_sec = total_running_ms / 1000;
    let days = total_sec / 86_400;
    let hours = (total_sec % 86_400) / 3_600;
    let minutes = (total_sec % 3_600) / 60;
    if days > 0 {
        format!("{}d {:02}h", days, hours)
    } else {
        format!("{:02}h {:02}m", hours, minutes)
    }
}

fn format_age_ms(age_ms: u64) -> String {
    if age_ms < 1_000 {
        format!("{}ms", age_ms)
    } else if age_ms < 60_000 {
        format!("{}s", age_ms / 1_000)
    } else {
        format!("{}m", age_ms / 60_000)
    }
}

fn latency_stats(samples: &[u64]) -> (String, String, String) {
    let p50 = percentile(samples, 50);
    let p95 = percentile(samples, 95);
    let p99 = percentile(samples, 99);
    (
        p50.map(|v| format!("{}ms", v)).unwrap_or_else(|| "-".to_string()),
        p95.map(|v| format!("{}ms", v)).unwrap_or_else(|| "-".to_string()),
        p99.map(|v| format!("{}ms", v)).unwrap_or_else(|| "-".to_string()),
    )
}

fn render_strategy_editor_popup(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let popup = Rect {
        x: area.x + 8,
        y: area.y + 4,
        width: area.width.saturating_sub(16).max(50),
        height: area.height.saturating_sub(8).max(12),
    };
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Strategy Config ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let selected_name = state
        .strategy_items
        .get(state.strategy_editor_index)
        .map(String::as_str)
        .unwrap_or("Unknown");
    let strategy_kind = state
        .strategy_editor_kind_items
        .get(state.strategy_editor_kind_index)
        .map(String::as_str)
        .unwrap_or("MA");
    let is_rsa = strategy_kind.eq_ignore_ascii_case("RSA");
    let is_atr = strategy_kind.eq_ignore_ascii_case("ATR");
    let is_chb = strategy_kind.eq_ignore_ascii_case("CHB");
    let period_1_label = if is_rsa {
        "RSI Period"
    } else if is_atr {
        "ATR Period"
    } else if is_chb {
        "Entry Window"
    } else {
        "Fast Period"
    };
    let period_2_label = if is_rsa {
        "Upper RSI"
    } else if is_atr {
        "Threshold x100"
    } else if is_chb {
        "Exit Window"
    } else {
        "Slow Period"
    };
    let rows = [
        ("Strategy", strategy_kind.to_string()),
        (
            "Symbol",
            state
                .symbol_items
                .get(state.strategy_editor_symbol_index)
                .cloned()
                .unwrap_or_else(|| state.symbol.clone()),
        ),
        (period_1_label, state.strategy_editor_fast.to_string()),
        (period_2_label, state.strategy_editor_slow.to_string()),
        ("Cooldown Tick", state.strategy_editor_cooldown.to_string()),
    ];
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Target: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                selected_name,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            "Use [J/K] field, [H/L] value, [Enter] save, [Esc] cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    if is_rsa {
        let lower = 100usize.saturating_sub(state.strategy_editor_slow.clamp(51, 95));
        lines.push(Line::from(Span::styled(
            format!("RSA lower threshold auto-derived: {}", lower),
            Style::default().fg(Color::DarkGray),
        )));
    } else if is_atr {
        let threshold_x100 = state.strategy_editor_slow.clamp(110, 500);
        lines.push(Line::from(Span::styled(
            format!("ATR expansion threshold: {:.2}x", threshold_x100 as f64 / 100.0),
            Style::default().fg(Color::DarkGray),
        )));
    } else if is_chb {
        lines.push(Line::from(Span::styled(
            "CHB breakout: buy on entry high break, sell on exit low break",
            Style::default().fg(Color::DarkGray),
        )));
    }
    for (idx, (name, value)) in rows.iter().enumerate() {
        let marker = if idx == state.strategy_editor_field {
            "▶ "
        } else {
            "  "
        };
        let style = if idx == state.strategy_editor_field {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(Color::Yellow)),
            Span::styled(format!("{:<14}", name), style),
            Span::styled(value, style),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), inner);
    if state.strategy_editor_kind_category_selector_open {
        render_selector_popup(
            frame,
            " Select Strategy Category ",
            &state.strategy_editor_kind_category_items,
            state
                .strategy_editor_kind_category_index
                .min(state.strategy_editor_kind_category_items.len().saturating_sub(1)),
            None,
            None,
            None,
        );
    } else if state.strategy_editor_kind_selector_open {
        render_selector_popup(
            frame,
            " Select Strategy Type ",
            &state.strategy_editor_kind_popup_items,
            state
                .strategy_editor_kind_selector_index
                .min(state.strategy_editor_kind_popup_items.len().saturating_sub(1)),
            None,
            None,
            None,
        );
    }
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
    let visible = build_history_lines(rows, max_rows);
    frame.render_widget(Paragraph::new(visible), inner);
}

fn build_history_lines(rows: &[String], max_rows: usize) -> Vec<Line<'_>> {
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
            row.as_str(),
            Style::default().fg(color),
        )));
    }
    if visible.is_empty() {
        visible.push(Line::from(Span::styled(
            "No history rows",
            Style::default().fg(Color::DarkGray),
        )));
    }
    visible
}

fn render_selector_popup(
    frame: &mut Frame,
    title: &str,
    items: &[String],
    selected: usize,
    stats: Option<&HashMap<String, OrderHistoryStats>>,
    total_stats: Option<OrderHistoryStats>,
    selected_symbol: Option<&str>,
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
        if let Some(symbol) = selected_symbol {
            lines.push(Line::from(vec![
                Span::styled("  Symbol: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    symbol,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
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
                let symbol = selected_symbol.unwrap_or("-");
                if let Some(s) = strategy_stats_for_item(stats_map, item, symbol) {
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
            let symbol = selected_symbol.unwrap_or("-");
            if let Some(s) = strategy_stats_for_item(stats_map, item, symbol) {
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
    symbol: &str,
) -> Option<&'a OrderHistoryStats> {
    if let Some(source_tag) = source_tag_for_strategy_item(item) {
        let scoped = strategy_stats_scope_key(symbol, &source_tag);
        if let Some(s) = stats_map.get(&scoped) {
            return Some(s);
        }
    }
    if let Some(s) = stats_map.get(item) {
        return Some(s);
    }
    let source_tag = source_tag_for_strategy_item(item);
    source_tag.and_then(|tag| {
        stats_map
            .get(&tag)
            .or_else(|| stats_map.get(&tag.to_ascii_uppercase()))
    })
}

fn ev_snapshot_for_item<'a>(
    ev_map: &'a HashMap<String, EvSnapshotEntry>,
    item: &str,
    symbol: &str,
) -> Option<&'a EvSnapshotEntry> {
    if let Some(source_tag) = source_tag_for_strategy_item(item) {
        if let Some(found) = ev_map.get(&strategy_stats_scope_key(symbol, &source_tag)) {
            return Some(found);
        }
    }
    latest_ev_snapshot_for_symbol(ev_map, symbol)
}

fn exit_policy_for_item<'a>(
    policy_map: &'a HashMap<String, ExitPolicyEntry>,
    item: &str,
    symbol: &str,
) -> Option<&'a ExitPolicyEntry> {
    if let Some(source_tag) = source_tag_for_strategy_item(item) {
        if let Some(found) = policy_map.get(&strategy_stats_scope_key(symbol, &source_tag)) {
            return Some(found);
        }
    }
    latest_exit_policy_for_symbol(policy_map, symbol)
}

fn latest_ev_snapshot_for_symbol<'a>(
    ev_map: &'a HashMap<String, EvSnapshotEntry>,
    symbol: &str,
) -> Option<&'a EvSnapshotEntry> {
    let prefix = format!("{}::", symbol.trim().to_ascii_uppercase());
    ev_map
        .iter()
        .filter(|(k, _)| k.starts_with(&prefix))
        .max_by_key(|(_, v)| v.updated_at_ms)
        .map(|(_, v)| v)
}

fn latest_exit_policy_for_symbol<'a>(
    policy_map: &'a HashMap<String, ExitPolicyEntry>,
    symbol: &str,
) -> Option<&'a ExitPolicyEntry> {
    let prefix = format!("{}::", symbol.trim().to_ascii_uppercase());
    policy_map
        .iter()
        .filter(|(k, _)| k.starts_with(&prefix))
        .max_by_key(|(_, v)| v.updated_at_ms)
        .map(|(_, v)| v)
}

fn strategy_stats_scope_key(symbol: &str, source_tag: &str) -> String {
    format!(
        "{}::{}",
        symbol.trim().to_ascii_uppercase(),
        source_tag.trim().to_ascii_lowercase()
    )
}

fn source_tag_for_strategy_item(item: &str) -> Option<String> {
    match item {
        "MA(Config)" => return Some("cfg".to_string()),
        "MA(Fast 5/20)" => return Some("fst".to_string()),
        "MA(Slow 20/60)" => return Some("slw".to_string()),
        "RSA(RSI 14 30/70)" => return Some("rsa".to_string()),
        "DCT(Donchian 20/10)" => return Some("dct".to_string()),
        "MRV(SMA 20 -2.00%)" => return Some("mrv".to_string()),
        "BBR(BB 20 2.00x)" => return Some("bbr".to_string()),
        "STO(Stoch 14 20/80)" => return Some("sto".to_string()),
        "VLC(Compression 20 1.20%)" => return Some("vlc".to_string()),
        "ORB(Opening 12/8)" => return Some("orb".to_string()),
        "REG(Regime 10/30)" => return Some("reg".to_string()),
        "ENS(Vote 10/30)" => return Some("ens".to_string()),
        "MAC(MACD 12/26)" => return Some("mac".to_string()),
        "ROC(ROC 10 0.20%)" => return Some("roc".to_string()),
        "ARN(Aroon 14 70)" => return Some("arn".to_string()),
        _ => {}
    }
    if let Some((_, tail)) = item.rsplit_once('[') {
        if let Some(tag) = tail.strip_suffix(']') {
            let tag = tag.trim();
            if !tag.is_empty() {
                return Some(tag.to_ascii_lowercase());
            }
        }
    }
    None
}

fn parse_source_tag_from_client_order_id(client_order_id: &str) -> Option<&str> {
    let body = client_order_id.strip_prefix("sq-")?;
    let (source_tag, _) = body.split_once('-')?;
    if source_tag.is_empty() {
        None
    } else {
        Some(source_tag)
    }
}

fn format_log_record_compact(record: &LogRecord) -> String {
    let level = match record.level {
        LogLevel::Debug => "DEBUG",
        LogLevel::Info => "INFO",
        LogLevel::Warn => "WARN",
        LogLevel::Error => "ERR",
    };
    let domain = match record.domain {
        LogDomain::Ws => "ws",
        LogDomain::Strategy => "strategy",
        LogDomain::Risk => "risk",
        LogDomain::Order => "order",
        LogDomain::Portfolio => "portfolio",
        LogDomain::Ui => "ui",
        LogDomain::System => "system",
    };
    let symbol = record.symbol.as_deref().unwrap_or("-");
    let strategy = record.strategy_tag.as_deref().unwrap_or("-");
    format!(
        "[{}] {}.{} {} {} {}",
        level, domain, record.event, symbol, strategy, record.msg
    )
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
        *fee_by_asset
            .entry(f.commission_asset.clone())
            .or_insert(0.0) += f.commission;
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
