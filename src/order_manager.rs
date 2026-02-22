use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use chrono::TimeZone;

use crate::binance::rest::BinanceRestClient;
use crate::binance::types::{BinanceMyTrade, BinanceOrderResponse};
use crate::config::RiskConfig;
use crate::model::order::{Fill, Order, OrderSide, OrderStatus, OrderType};
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_store;
use crate::risk_module::{
    ApiEndpointGroup, EndpointRateLimits, OrderIntent, RateBudgetSnapshot, RejectionReasonCode,
    RiskModule,
};

pub use crate::risk_module::MarketKind;

#[derive(Debug, Clone)]
pub enum OrderUpdate {
    Submitted {
        intent_id: String,
        client_order_id: String,
        server_order_id: u64,
    },
    Filled {
        intent_id: String,
        client_order_id: String,
        side: OrderSide,
        fills: Vec<Fill>,
        avg_price: f64,
    },
    Rejected {
        intent_id: String,
        client_order_id: String,
        reason_code: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct OrderHistoryStats {
    pub trade_count: u32,
    pub win_count: u32,
    pub lose_count: u32,
    pub realized_pnl: f64,
}

#[derive(Debug, Clone, Default)]
pub struct OrderHistorySnapshot {
    pub rows: Vec<String>,
    pub stats: OrderHistoryStats,
    pub strategy_stats: HashMap<String, OrderHistoryStats>,
    pub fills: Vec<OrderHistoryFill>,
    pub open_qty: f64,
    pub open_entry_price: f64,
    pub estimated_total_pnl_usdt: Option<f64>,
    pub trade_data_complete: bool,
    pub fetched_at_ms: u64,
    pub fetch_latency_ms: u64,
    pub latest_event_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct OrderHistoryFill {
    pub timestamp_ms: u64,
    pub side: OrderSide,
    pub price: f64,
}

pub struct OrderManager {
    rest_client: Arc<BinanceRestClient>,
    active_orders: HashMap<String, Order>,
    position: Position,
    symbol: String,
    market: MarketKind,
    order_amount_usdt: f64,
    balances: HashMap<String, f64>,
    last_price: f64,
    risk_module: RiskModule,
    default_strategy_cooldown_ms: u64,
    default_strategy_max_active_orders: u32,
    strategy_limits_by_tag: HashMap<String, StrategyExecutionLimit>,
    last_strategy_submit_ms: HashMap<String, u64>,
    default_symbol_max_exposure_usdt: f64,
    symbol_exposure_limit_by_key: HashMap<String, f64>,
}

#[derive(Debug, Clone, Copy)]
struct StrategyExecutionLimit {
    cooldown_ms: u64,
    max_active_orders: u32,
}

fn normalize_market_label(market: MarketKind) -> &'static str {
    match market {
        MarketKind::Spot => "spot",
        MarketKind::Futures => "futures",
    }
}

fn symbol_limit_key(symbol: &str, market: MarketKind) -> String {
    format!(
        "{}:{}",
        symbol.trim().to_ascii_uppercase(),
        normalize_market_label(market)
    )
}

fn storage_symbol(symbol: &str, market: MarketKind) -> String {
    match market {
        MarketKind::Spot => symbol.to_string(),
        MarketKind::Futures => format!("{}#FUT", symbol),
    }
}

fn display_qty_for_history(status: &str, orig_qty: f64, executed_qty: f64) -> f64 {
    match status {
        "FILLED" | "PARTIALLY_FILLED" => executed_qty,
        _ => orig_qty,
    }
}

fn format_history_time(timestamp_ms: u64) -> String {
    chrono::Utc
        .timestamp_millis_opt(timestamp_ms as i64)
        .single()
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| "--:--:--".to_string())
}

fn format_order_history_row(
    timestamp_ms: u64,
    status: &str,
    side: &str,
    qty: f64,
    avg_price: f64,
    client_order_id: &str,
) -> String {
    format!(
        "{} {:<10} {:<4} {:.5} @ {:.2}  {}",
        format_history_time(timestamp_ms),
        status,
        side,
        qty,
        avg_price,
        client_order_id
    )
}

fn source_label_from_client_order_id(client_order_id: &str) -> String {
    if client_order_id.contains("-mnl-") {
        "MANUAL".to_string()
    } else if client_order_id.contains("-cfg-") {
        "MA(Config)".to_string()
    } else if client_order_id.contains("-fst-") {
        "MA(Fast 5/20)".to_string()
    } else if client_order_id.contains("-slw-") {
        "MA(Slow 20/60)".to_string()
    } else if let Some(source_tag) = parse_source_tag_from_client_order_id(client_order_id) {
        source_tag.to_ascii_lowercase()
    } else {
        "UNKNOWN".to_string()
    }
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

fn format_trade_history_row(t: &BinanceMyTrade, source: &str) -> String {
    let side = if t.is_buyer { "BUY" } else { "SELL" };
    format_order_history_row(
        t.time,
        "FILLED",
        side,
        t.qty,
        t.price,
        &format!("order#{}#T{} [{}]", t.order_id, t.id, source),
    )
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

#[derive(Clone, Copy, Default)]
struct LongPos {
    qty: f64,
    cost_quote: f64,
}

fn apply_spot_trade_with_fee(
    pos: &mut LongPos,
    stats: &mut OrderHistoryStats,
    t: &BinanceMyTrade,
    base_asset: &str,
    quote_asset: &str,
) {
    let qty = t.qty.max(0.0);
    if qty <= f64::EPSILON {
        return;
    }
    let fee_asset = t.commission_asset.as_str();
    let fee_is_base = !base_asset.is_empty() && fee_asset.eq_ignore_ascii_case(base_asset);
    let fee_is_quote = !quote_asset.is_empty() && fee_asset.eq_ignore_ascii_case(quote_asset);

    if t.is_buyer {
        let net_qty = (qty
            - if fee_is_base {
                t.commission.max(0.0)
            } else {
                0.0
            })
        .max(0.0);
        if net_qty <= f64::EPSILON {
            return;
        }
        let fee_quote = if fee_is_quote {
            t.commission.max(0.0)
        } else {
            0.0
        };
        pos.qty += net_qty;
        pos.cost_quote += qty * t.price + fee_quote;
        return;
    }

    // Spot sell: close against existing long inventory.
    if pos.qty <= f64::EPSILON {
        return;
    }
    let close_qty = qty.min(pos.qty);
    if close_qty <= f64::EPSILON {
        return;
    }
    let avg_cost = pos.cost_quote / pos.qty.max(f64::EPSILON);
    let fee_quote_total = if fee_is_quote {
        t.commission.max(0.0)
    } else if fee_is_base {
        // If fee is charged in base on sell, approximate its quote impact at fill price.
        t.commission.max(0.0) * t.price
    } else {
        0.0
    };
    let fee_quote = fee_quote_total * (close_qty / qty.max(f64::EPSILON));
    let pnl_delta = (close_qty * t.price - fee_quote) - (avg_cost * close_qty);
    if pnl_delta > 0.0 {
        stats.win_count += 1;
        stats.trade_count += 1;
    } else if pnl_delta < 0.0 {
        stats.lose_count += 1;
        stats.trade_count += 1;
    }
    stats.realized_pnl += pnl_delta;

    pos.qty -= close_qty;
    pos.cost_quote -= avg_cost * close_qty;
    if pos.qty <= f64::EPSILON {
        pos.qty = 0.0;
        pos.cost_quote = 0.0;
    }
}

fn compute_trade_state(
    mut trades: Vec<BinanceMyTrade>,
    symbol: &str,
) -> (OrderHistoryStats, LongPos) {
    trades.sort_by_key(|t| (t.time, t.id));
    let (base_asset, quote_asset) = split_symbol_assets(symbol);
    let mut pos = LongPos::default();
    let mut stats = OrderHistoryStats::default();
    for t in trades {
        apply_spot_trade_with_fee(&mut pos, &mut stats, &t, &base_asset, &quote_asset);
    }
    (stats, pos)
}

fn compute_futures_open_state(mut trades: Vec<BinanceMyTrade>) -> LongPos {
    trades.sort_by_key(|t| (t.time, t.id));
    let mut pos = LongPos::default();
    for t in trades {
        let qty = t.qty.max(0.0);
        if qty <= f64::EPSILON {
            continue;
        }
        if t.is_buyer {
            pos.qty += qty;
            pos.cost_quote += qty * t.price;
            continue;
        }
        if pos.qty <= f64::EPSILON {
            continue;
        }
        let close_qty = qty.min(pos.qty);
        let avg_cost = pos.cost_quote / pos.qty.max(f64::EPSILON);
        pos.qty -= close_qty;
        pos.cost_quote -= avg_cost * close_qty;
        if pos.qty <= f64::EPSILON {
            pos.qty = 0.0;
            pos.cost_quote = 0.0;
        }
    }
    pos
}

fn compute_trade_stats_by_source(
    mut trades: Vec<BinanceMyTrade>,
    order_source_by_id: &HashMap<u64, String>,
    symbol: &str,
) -> HashMap<String, OrderHistoryStats> {
    trades.sort_by_key(|t| (t.time, t.id));

    // Futures: realized_pnl is exchange-provided per fill.
    if symbol.ends_with("#FUT") {
        let mut stats_by_source: HashMap<String, OrderHistoryStats> = HashMap::new();
        for t in trades {
            let source = order_source_by_id
                .get(&t.order_id)
                .cloned()
                .unwrap_or_else(|| "UNKNOWN".to_string());
            let stats = stats_by_source.entry(source).or_default();
            if t.realized_pnl > 0.0 {
                stats.win_count += 1;
                stats.trade_count += 1;
            } else if t.realized_pnl < 0.0 {
                stats.lose_count += 1;
                stats.trade_count += 1;
            }
            stats.realized_pnl += t.realized_pnl;
        }
        return stats_by_source;
    }

    let (base_asset, quote_asset) = split_symbol_assets(symbol);
    let mut pos_by_source: HashMap<String, LongPos> = HashMap::new();
    let mut stats_by_source: HashMap<String, OrderHistoryStats> = HashMap::new();

    for t in trades {
        let source = order_source_by_id
            .get(&t.order_id)
            .cloned()
            .unwrap_or_else(|| "UNKNOWN".to_string());
        let pos = pos_by_source.entry(source.clone()).or_default();
        let stats = stats_by_source.entry(source).or_default();
        apply_spot_trade_with_fee(pos, stats, &t, &base_asset, &quote_asset);
    }

    stats_by_source
}

fn to_persistable_stats_map(
    strategy_stats: &HashMap<String, OrderHistoryStats>,
) -> HashMap<String, order_store::StrategyScopedStats> {
    strategy_stats
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                order_store::StrategyScopedStats {
                    trade_count: v.trade_count,
                    win_count: v.win_count,
                    lose_count: v.lose_count,
                    realized_pnl: v.realized_pnl,
                },
            )
        })
        .collect()
}

fn from_persisted_stats_map(
    persisted: HashMap<String, order_store::StrategyScopedStats>,
) -> HashMap<String, OrderHistoryStats> {
    persisted
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                OrderHistoryStats {
                    trade_count: v.trade_count,
                    win_count: v.win_count,
                    lose_count: v.lose_count,
                    realized_pnl: v.realized_pnl,
                },
            )
        })
        .collect()
}

impl OrderManager {
    /// Create a new order manager bound to a single symbol/market context.
    ///
    /// The instance keeps in-memory position, cached balances, and an embedded
    /// `RiskModule` that enforces pre-trade checks and global rate budget.
    ///
    /// # Caution
    /// This manager is stateful (`last_price`, balances, active orders). Reuse
    /// the same instance for a symbol stream instead of recreating per tick.
    pub fn new(
        rest_client: Arc<BinanceRestClient>,
        symbol: &str,
        market: MarketKind,
        order_amount_usdt: f64,
        risk_config: &RiskConfig,
    ) -> Self {
        let mut strategy_limits_by_tag = HashMap::new();
        let mut symbol_exposure_limit_by_key = HashMap::new();
        let default_strategy_cooldown_ms = risk_config.default_strategy_cooldown_ms;
        let default_strategy_max_active_orders =
            risk_config.default_strategy_max_active_orders.max(1);
        let default_symbol_max_exposure_usdt =
            risk_config.default_symbol_max_exposure_usdt.max(0.0);
        for profile in &risk_config.strategy_limits {
            let source_tag = profile.source_tag.trim().to_ascii_lowercase();
            if source_tag.is_empty() {
                continue;
            }
            strategy_limits_by_tag.insert(
                source_tag,
                StrategyExecutionLimit {
                    cooldown_ms: profile.cooldown_ms.unwrap_or(default_strategy_cooldown_ms),
                    max_active_orders: profile
                        .max_active_orders
                        .unwrap_or(default_strategy_max_active_orders)
                        .max(1),
                },
            );
        }
        for limit in &risk_config.symbol_exposure_limits {
            let symbol = limit.symbol.trim().to_ascii_uppercase();
            if symbol.is_empty() {
                continue;
            }
            let market = match limit
                .market
                .as_deref()
                .unwrap_or("spot")
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "spot" => MarketKind::Spot,
                "futures" | "future" | "fut" => MarketKind::Futures,
                _ => continue,
            };
            symbol_exposure_limit_by_key.insert(
                symbol_limit_key(&symbol, market),
                limit.max_exposure_usdt.max(0.0),
            );
        }
        Self {
            rest_client: rest_client.clone(),
            active_orders: HashMap::new(),
            position: Position::new(symbol.to_string()),
            symbol: symbol.to_string(),
            market,
            order_amount_usdt,
            balances: HashMap::new(),
            last_price: 0.0,
            risk_module: RiskModule::new(
                rest_client.clone(),
                risk_config.global_rate_limit_per_minute,
                EndpointRateLimits {
                    orders_per_minute: risk_config.endpoint_rate_limits.orders_per_minute,
                    account_per_minute: risk_config.endpoint_rate_limits.account_per_minute,
                    market_data_per_minute: risk_config.endpoint_rate_limits.market_data_per_minute,
                },
            ),
            default_strategy_cooldown_ms,
            default_strategy_max_active_orders,
            strategy_limits_by_tag,
            last_strategy_submit_ms: HashMap::new(),
            default_symbol_max_exposure_usdt,
            symbol_exposure_limit_by_key,
        }
    }

    /// Return current in-memory position snapshot.
    ///
    /// Values reflect fills processed by this process. They are not a full
    /// exchange reconciliation snapshot.
    pub fn position(&self) -> &Position {
        &self.position
    }

    pub fn market_kind(&self) -> MarketKind {
        self.market
    }

    /// Return latest cached free balances.
    ///
    /// Cache is updated by `refresh_balances`. Missing assets should be treated
    /// as zero balance.
    pub fn balances(&self) -> &HashMap<String, f64> {
        &self.balances
    }

    /// Update last price and recompute unrealized PnL.
    ///
    /// # Usage
    /// Call on every market data tick before `submit_order`, so risk checks use
    /// a valid `last_price`.
    pub fn update_unrealized_pnl(&mut self, current_price: f64) {
        self.last_price = current_price;
        self.position.update_unrealized_pnl(current_price);
    }

    /// Return current global rate-budget snapshot from the risk module.
    ///
    /// Intended for UI display and observability.
    pub fn rate_budget_snapshot(&self) -> RateBudgetSnapshot {
        self.risk_module.rate_budget_snapshot()
    }

    pub fn orders_rate_budget_snapshot(&self) -> RateBudgetSnapshot {
        self.risk_module
            .endpoint_budget_snapshot(ApiEndpointGroup::Orders)
    }

    pub fn account_rate_budget_snapshot(&self) -> RateBudgetSnapshot {
        self.risk_module
            .endpoint_budget_snapshot(ApiEndpointGroup::Account)
    }

    pub fn market_data_rate_budget_snapshot(&self) -> RateBudgetSnapshot {
        self.risk_module
            .endpoint_budget_snapshot(ApiEndpointGroup::MarketData)
    }

    fn strategy_limits_for(&self, source_tag: &str) -> StrategyExecutionLimit {
        self.strategy_limits_by_tag
            .get(source_tag)
            .copied()
            .unwrap_or(StrategyExecutionLimit {
                cooldown_ms: self.default_strategy_cooldown_ms,
                max_active_orders: self.default_strategy_max_active_orders,
            })
    }

    fn active_order_count_for_source(&self, source_tag: &str) -> u32 {
        let prefix = format!("sq-{}-", source_tag);
        self.active_orders
            .values()
            .filter(|o| !o.status.is_terminal() && o.client_order_id.starts_with(&prefix))
            .count() as u32
    }

    fn evaluate_strategy_limits(
        &self,
        source_tag: &str,
        created_at_ms: u64,
    ) -> Option<(String, String)> {
        let limits = self.strategy_limits_for(source_tag);
        let active_count = self.active_order_count_for_source(source_tag);
        if active_count >= limits.max_active_orders {
            return Some((
                RejectionReasonCode::RiskStrategyMaxActiveOrdersExceeded
                    .as_str()
                    .to_string(),
                format!(
                    "Strategy '{}' active order limit exceeded (active {}, limit {})",
                    source_tag, active_count, limits.max_active_orders
                ),
            ));
        }

        if limits.cooldown_ms > 0 {
            if let Some(last_submit_ms) = self.last_strategy_submit_ms.get(source_tag) {
                let elapsed = created_at_ms.saturating_sub(*last_submit_ms);
                if elapsed < limits.cooldown_ms {
                    let remaining = limits.cooldown_ms - elapsed;
                    return Some((
                        RejectionReasonCode::RiskStrategyCooldownActive
                            .as_str()
                            .to_string(),
                        format!(
                            "Strategy '{}' cooldown active ({}ms remaining)",
                            source_tag, remaining
                        ),
                    ));
                }
            }
        }

        None
    }

    fn mark_strategy_submit(&mut self, source_tag: &str, created_at_ms: u64) {
        self.last_strategy_submit_ms
            .insert(source_tag.to_string(), created_at_ms);
    }

    fn max_symbol_exposure_usdt(&self) -> f64 {
        self.symbol_exposure_limit_by_key
            .get(&symbol_limit_key(&self.symbol, self.market))
            .copied()
            .unwrap_or(self.default_symbol_max_exposure_usdt)
    }

    fn projected_notional_after_fill(&self, side: OrderSide, qty: f64) -> (f64, f64) {
        let price = self.last_price.max(0.0);
        if price <= f64::EPSILON {
            return (0.0, 0.0);
        }
        let current_qty_signed = match self.position.side {
            Some(OrderSide::Buy) => self.position.qty,
            Some(OrderSide::Sell) => -self.position.qty,
            None => 0.0,
        };
        let delta = match side {
            OrderSide::Buy => qty,
            OrderSide::Sell => -qty,
        };
        let projected_qty_signed = current_qty_signed + delta;
        (
            current_qty_signed.abs() * price,
            projected_qty_signed.abs() * price,
        )
    }

    fn evaluate_symbol_exposure_limit(
        &self,
        side: OrderSide,
        qty: f64,
    ) -> Option<(String, String)> {
        let max_exposure = self.max_symbol_exposure_usdt();
        if max_exposure <= f64::EPSILON {
            return None;
        }
        let (current_notional, projected_notional) = self.projected_notional_after_fill(side, qty);
        if projected_notional > max_exposure && projected_notional > current_notional + f64::EPSILON
        {
            return Some((
                RejectionReasonCode::RiskSymbolExposureLimitExceeded
                    .as_str()
                    .to_string(),
                format!(
                    "Symbol exposure limit exceeded for {} ({:?}): projected {:.2} USDT > limit {:.2} USDT",
                    self.symbol, self.market, projected_notional, max_exposure
                ),
            ));
        }
        None
    }

    /// Return whether a hypothetical fill would exceed symbol exposure limit.
    ///
    /// This is intended for validation and tests; it does not mutate state.
    pub fn would_exceed_symbol_exposure_limit(&self, side: OrderSide, qty: f64) -> bool {
        self.evaluate_symbol_exposure_limit(side, qty).is_some()
    }

    /// Fetch account balances from Binance and update internal state.
    ///
    /// Returns the map `asset -> free` for assets with non-zero total (spot) or
    /// non-trivial wallet balance (futures).
    ///
    /// # Usage
    /// Refresh before order submission cycles to reduce false "insufficient
    /// balance" rejections from stale cache.
    ///
    /// # Caution
    /// Network/API failures return `Err(_)` and leave previous cache untouched.
    pub async fn refresh_balances(&mut self) -> Result<HashMap<String, f64>> {
        if !self
            .risk_module
            .reserve_endpoint_budget(ApiEndpointGroup::Account)
        {
            return Err(anyhow::anyhow!(
                "Account endpoint budget exceeded; try again after reset"
            ));
        }
        if self.market == MarketKind::Futures {
            let account = self.rest_client.get_futures_account().await?;
            self.balances.clear();
            for a in &account.assets {
                if a.wallet_balance.abs() > f64::EPSILON {
                    self.balances.insert(a.asset.clone(), a.available_balance);
                }
            }
            return Ok(self.balances.clone());
        }
        let account = self.rest_client.get_account().await?;
        self.balances.clear();
        for b in &account.balances {
            let total = b.free + b.locked;
            if total > 0.0 {
                self.balances.insert(b.asset.clone(), b.free);
            }
        }
        tracing::info!(balances = ?self.balances, "Balances refreshed");
        Ok(self.balances.clone())
    }

    /// Fetch order history from exchange and format rows for UI display.
    ///
    /// This method combines order and trade endpoints, persists snapshots to
    /// local sqlite, and emits a best-effort history view even if one endpoint
    /// fails.
    ///
    /// # Caution
    /// `trade_data_complete = false` means derived PnL may be partial.
    pub async fn refresh_order_history(&mut self, limit: usize) -> Result<OrderHistorySnapshot> {
        if !self
            .risk_module
            .reserve_endpoint_budget(ApiEndpointGroup::Orders)
        {
            return Err(anyhow::anyhow!(
                "Orders endpoint budget exceeded; try again after reset"
            ));
        }
        if self.market == MarketKind::Futures {
            let fetch_started = Instant::now();
            let fetched_at_ms = chrono::Utc::now().timestamp_millis() as u64;
            let orders_result = self
                .rest_client
                .get_futures_all_orders(&self.symbol, limit)
                .await;
            let trades_result = self
                .rest_client
                .get_futures_my_trades_history(&self.symbol, limit.max(1))
                .await;
            let fetch_latency_ms = fetch_started.elapsed().as_millis() as u64;

            if orders_result.is_err() && trades_result.is_err() {
                let oe = orders_result.err().unwrap();
                let te = trades_result.err().unwrap();
                return Err(anyhow::anyhow!(
                    "futures order history fetch failed: allOrders={} | userTrades={}",
                    oe,
                    te
                ));
            }

            let mut orders = orders_result.unwrap_or_default();
            let trades = trades_result.unwrap_or_default();
            orders.sort_by_key(|o| o.update_time.max(o.time));

            let storage_key = storage_symbol(&self.symbol, self.market);
            if let Err(e) = order_store::persist_order_snapshot(&storage_key, &orders, &trades) {
                tracing::warn!(error = %e, "Failed to persist futures order snapshot to sqlite");
            }

            let mut order_source_by_id = HashMap::new();
            for o in &orders {
                order_source_by_id.insert(
                    o.order_id,
                    source_label_from_client_order_id(&o.client_order_id),
                );
            }
            let mut trades_for_stats = trades.clone();
            match order_store::load_persisted_trades(&storage_key) {
                Ok(saved) if !saved.is_empty() => {
                    trades_for_stats = saved.iter().map(|r| r.trade.clone()).collect();
                    for row in saved {
                        order_source_by_id.entry(row.trade.order_id).or_insert(row.source);
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Failed to load persisted futures trades; using API trades"
                    );
                }
            }

            let mut history = Vec::new();
            let mut fills = Vec::new();
            for t in &trades {
                let side = if t.is_buyer { "BUY" } else { "SELL" };
                let source = order_source_by_id
                    .get(&t.order_id)
                    .cloned()
                    .unwrap_or_else(|| "UNKNOWN".to_string());
                fills.push(OrderHistoryFill {
                    timestamp_ms: t.time,
                    side: if t.is_buyer {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    },
                    price: t.price,
                });
                history.push(format_order_history_row(
                    t.time,
                    "FILLED",
                    side,
                    t.qty,
                    t.price,
                    &format!("order#{}#T{} [{}]", t.order_id, t.id, source),
                ));
            }
            for o in &orders {
                if o.executed_qty <= 0.0 {
                    history.push(format_order_history_row(
                        o.update_time.max(o.time),
                        &o.status,
                        &o.side,
                        display_qty_for_history(&o.status, o.orig_qty, o.executed_qty),
                        if o.executed_qty > 0.0 {
                            o.cummulative_quote_qty / o.executed_qty
                        } else {
                            o.price
                        },
                        &o.client_order_id,
                    ));
                }
            }

            let mut stats = OrderHistoryStats::default();
            for t in &trades {
                if t.realized_pnl > 0.0 {
                    stats.win_count += 1;
                    stats.trade_count += 1;
                } else if t.realized_pnl < 0.0 {
                    stats.lose_count += 1;
                    stats.trade_count += 1;
                }
                stats.realized_pnl += t.realized_pnl;
            }
            let open_pos = compute_futures_open_state(trades_for_stats.clone());
            let open_entry_price = if open_pos.qty > f64::EPSILON {
                open_pos.cost_quote / open_pos.qty
            } else {
                0.0
            };
            self.position.side = if open_pos.qty > f64::EPSILON {
                Some(OrderSide::Buy)
            } else {
                None
            };
            self.position.qty = open_pos.qty;
            self.position.entry_price = open_entry_price;
            self.position.realized_pnl = stats.realized_pnl;
            if self.last_price > 0.0 {
                self.position.update_unrealized_pnl(self.last_price);
            } else {
                self.position.unrealized_pnl = 0.0;
            }
            let estimated_total_pnl_usdt = if self.last_price > 0.0 && open_pos.qty > f64::EPSILON {
                Some(stats.realized_pnl + (self.last_price - open_entry_price) * open_pos.qty)
            } else {
                Some(stats.realized_pnl)
            };
            let latest_order_event = orders.iter().map(|o| o.update_time.max(o.time)).max();
            let latest_trade_event = trades.iter().map(|t| t.time).max();
            let mut strategy_stats =
                compute_trade_stats_by_source(trades_for_stats, &order_source_by_id, &storage_key);
            let persisted_stats = to_persistable_stats_map(&strategy_stats);
            if let Err(e) = order_store::persist_strategy_symbol_stats(&storage_key, &persisted_stats)
            {
                tracing::warn!(error = %e, "Failed to persist strategy stats (futures)");
            }
            if strategy_stats.is_empty() {
                match order_store::load_strategy_symbol_stats(&storage_key) {
                    Ok(persisted) => {
                        strategy_stats = from_persisted_stats_map(persisted);
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Failed to load persisted strategy stats (futures)"
                        );
                    }
                }
            }
            return Ok(OrderHistorySnapshot {
                rows: history,
                stats,
                strategy_stats,
                fills,
                open_qty: open_pos.qty,
                open_entry_price,
                estimated_total_pnl_usdt,
                trade_data_complete: true,
                fetched_at_ms,
                fetch_latency_ms,
                latest_event_ms: latest_order_event.max(latest_trade_event),
            });
        }

        let fetch_started = Instant::now();
        let fetched_at_ms = chrono::Utc::now().timestamp_millis() as u64;
        let orders_result = self.rest_client.get_all_orders(&self.symbol, limit).await;
        let storage_key = storage_symbol(&self.symbol, self.market);
        let last_trade_id = order_store::load_last_trade_id(&storage_key).ok().flatten();
        let persisted_trade_count = order_store::load_trade_count(&storage_key).unwrap_or(0);
        let need_backfill = persisted_trade_count < limit;
        let trades_result = match (need_backfill, last_trade_id) {
            (true, _) => {
                self.rest_client
                    .get_my_trades_history(&self.symbol, limit.max(1))
                    .await
            }
            (false, Some(last_id)) => {
                self.rest_client
                    .get_my_trades_since(&self.symbol, last_id.saturating_add(1), 10)
                    .await
            }
            (false, None) => {
                self.rest_client
                    .get_my_trades_history(&self.symbol, limit.max(1))
                    .await
            }
        };
        let fetch_latency_ms = fetch_started.elapsed().as_millis() as u64;
        let trade_data_complete = trades_result.is_ok();

        if orders_result.is_err() && trades_result.is_err() {
            let oe = orders_result.err().unwrap();
            let te = trades_result.err().unwrap();
            return Err(anyhow::anyhow!(
                "order history fetch failed: allOrders={} | myTrades={}",
                oe,
                te
            ));
        }

        let mut orders = match orders_result {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch allOrders; falling back to trade-only history");
                Vec::new()
            }
        };
        let recent_trades = match trades_result {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch myTrades; falling back to order-only history");
                Vec::new()
            }
        };
        let mut trades = recent_trades.clone();
        orders.sort_by_key(|o| o.update_time.max(o.time));

        if let Err(e) = order_store::persist_order_snapshot(&storage_key, &orders, &recent_trades) {
            tracing::warn!(error = %e, "Failed to persist order snapshot to sqlite");
        }
        let mut persisted_source_by_order_id: HashMap<u64, String> = HashMap::new();
        match order_store::load_persisted_trades(&storage_key) {
            Ok(saved) => {
                if !saved.is_empty() {
                    trades = saved.iter().map(|r| r.trade.clone()).collect();
                    for row in saved {
                        persisted_source_by_order_id
                            .entry(row.trade.order_id)
                            .or_insert(row.source);
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load persisted trades; using recent API trades");
            }
        }

        let (stats, open_pos) = compute_trade_state(trades.clone(), &self.symbol);
        self.position.side = if open_pos.qty > f64::EPSILON {
            Some(OrderSide::Buy)
        } else {
            None
        };
        self.position.qty = open_pos.qty;
        self.position.entry_price = if open_pos.qty > f64::EPSILON {
            open_pos.cost_quote / open_pos.qty
        } else {
            0.0
        };
        self.position.realized_pnl = stats.realized_pnl;
        if self.last_price > 0.0 {
            self.position.update_unrealized_pnl(self.last_price);
        } else {
            self.position.unrealized_pnl = 0.0;
        }
        let estimated_total_pnl_usdt = if self.last_price > 0.0 {
            Some(stats.realized_pnl + (open_pos.qty * self.last_price - open_pos.cost_quote))
        } else {
            Some(stats.realized_pnl)
        };
        let latest_order_event = orders.iter().map(|o| o.update_time.max(o.time)).max();
        let latest_trade_event = trades.iter().map(|t| t.time).max();
        let latest_event_ms = latest_order_event.max(latest_trade_event);

        let mut trades_by_order_id: HashMap<u64, Vec<BinanceMyTrade>> = HashMap::new();
        for trade in &trades {
            trades_by_order_id
                .entry(trade.order_id)
                .or_default()
                .push(trade.clone());
        }
        for bucket in trades_by_order_id.values_mut() {
            bucket.sort_by_key(|t| t.time);
        }

        let mut order_source_by_id = HashMap::new();
        for o in &orders {
            order_source_by_id.insert(
                o.order_id,
                source_label_from_client_order_id(&o.client_order_id),
            );
        }
        for (order_id, source) in persisted_source_by_order_id {
            order_source_by_id.entry(order_id).or_insert(source);
        }
        let mut strategy_stats =
            compute_trade_stats_by_source(trades.clone(), &order_source_by_id, &self.symbol);
        let persisted_stats = to_persistable_stats_map(&strategy_stats);
        if let Err(e) = order_store::persist_strategy_symbol_stats(&storage_key, &persisted_stats) {
            tracing::warn!(error = %e, "Failed to persist strategy+symbol scoped stats");
        }
        if strategy_stats.is_empty() {
            match order_store::load_strategy_symbol_stats(&storage_key) {
                Ok(persisted) => {
                    strategy_stats = from_persisted_stats_map(persisted);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to load persisted strategy+symbol stats");
                }
            }
        }

        let mut history = Vec::new();
        let mut fills = Vec::new();
        let mut used_trade_ids = std::collections::HashSet::new();

        if orders.is_empty() && !trades.is_empty() {
            let mut sorted = trades;
            sorted.sort_by_key(|t| (t.time, t.id));
            history.extend(sorted.iter().map(|t| {
                fills.push(OrderHistoryFill {
                    timestamp_ms: t.time,
                    side: if t.is_buyer {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    },
                    price: t.price,
                });
                format_trade_history_row(
                    t,
                    order_source_by_id
                        .get(&t.order_id)
                        .map(String::as_str)
                        .unwrap_or("UNKNOWN"),
                )
            }));
            return Ok(OrderHistorySnapshot {
                rows: history,
                stats,
                strategy_stats,
                fills,
                open_qty: open_pos.qty,
                open_entry_price: if open_pos.qty > f64::EPSILON {
                    open_pos.cost_quote / open_pos.qty
                } else {
                    0.0
                },
                estimated_total_pnl_usdt,
                trade_data_complete,
                fetched_at_ms,
                fetch_latency_ms,
                latest_event_ms,
            });
        }

        for o in orders {
            if o.executed_qty > 0.0 {
                if let Some(order_trades) = trades_by_order_id.get(&o.order_id) {
                    for t in order_trades {
                        used_trade_ids.insert(t.id);
                        let side = if t.is_buyer { "BUY" } else { "SELL" };
                        fills.push(OrderHistoryFill {
                            timestamp_ms: t.time,
                            side: if t.is_buyer {
                                OrderSide::Buy
                            } else {
                                OrderSide::Sell
                            },
                            price: t.price,
                        });
                        history.push(format_order_history_row(
                            t.time,
                            "FILLED",
                            side,
                            t.qty,
                            t.price,
                            &format!(
                                "{}#T{} [{}]",
                                o.client_order_id,
                                t.id,
                                source_label_from_client_order_id(&o.client_order_id)
                            ),
                        ));
                    }
                    continue;
                }
            }

            let avg_price = if o.executed_qty > 0.0 {
                o.cummulative_quote_qty / o.executed_qty
            } else {
                o.price
            };
            history.push(format_order_history_row(
                o.update_time.max(o.time),
                &o.status,
                &o.side,
                display_qty_for_history(&o.status, o.orig_qty, o.executed_qty),
                avg_price,
                &o.client_order_id,
            ));
        }

        // Include trades that did not match fetched order pages.
        for bucket in trades_by_order_id.values() {
            for t in bucket {
                if !used_trade_ids.contains(&t.id) {
                    fills.push(OrderHistoryFill {
                        timestamp_ms: t.time,
                        side: if t.is_buyer {
                            OrderSide::Buy
                        } else {
                            OrderSide::Sell
                        },
                        price: t.price,
                    });
                    history.push(format_trade_history_row(
                        t,
                        order_source_by_id
                            .get(&t.order_id)
                            .map(String::as_str)
                            .unwrap_or("UNKNOWN"),
                    ));
                }
            }
        }
        Ok(OrderHistorySnapshot {
            rows: history,
            stats,
            strategy_stats,
            fills,
            open_qty: open_pos.qty,
            open_entry_price: if open_pos.qty > f64::EPSILON {
                open_pos.cost_quote / open_pos.qty
            } else {
                0.0
            },
            estimated_total_pnl_usdt,
            trade_data_complete,
            fetched_at_ms,
            fetch_latency_ms,
            latest_event_ms,
        })
    }

    /// Build an order intent, run risk checks, and submit to broker when approved.
    ///
    /// # Behavior
    /// - `Signal::Hold` returns `Ok(None)`.
    /// - For buy/sell signals, this method:
    ///   1. Builds `OrderIntent`.
    ///   2. Calls `RiskModule::evaluate_intent`.
    ///   3. Reserves one global rate token via `reserve_rate_budget`.
    ///   4. Submits market order to spot/futures broker endpoint.
    /// - Rejections are returned as `Ok(Some(OrderUpdate::Rejected { .. }))`
    ///   with structured `reason_code`.
    ///
    /// # Usage
    /// Recommended sequence:
    /// 1. `update_unrealized_pnl(last_price)`
    /// 2. `refresh_balances()` (periodic or before trading loop)
    /// 3. `submit_order(signal, source_tag)`
    ///
    /// # Caution
    /// - Spot sell requires base-asset balance (e.g. `ETH` for `ETHUSDT`).
    /// - If balances are stale, you may see "No position to sell" or
    ///   `"Insufficient <asset>"` even though exchange state changed recently.
    /// - This method returns transport/runtime errors as `Err(_)`; business
    ///   rejections are encoded in `OrderUpdate::Rejected`.
    pub async fn submit_order(
        &mut self,
        signal: Signal,
        source_tag: &str,
    ) -> Result<Option<OrderUpdate>> {
        let side = match &signal {
            Signal::Buy => OrderSide::Buy,
            Signal::Sell => OrderSide::Sell,
            Signal::Hold => return Ok(None),
        };
        let source_tag = source_tag.to_ascii_lowercase();
        let intent = OrderIntent {
            intent_id: format!("intent-{}", &uuid::Uuid::new_v4().to_string()[..8]),
            source_tag: source_tag.clone(),
            symbol: self.symbol.clone(),
            market: self.market,
            side,
            order_amount_usdt: self.order_amount_usdt,
            last_price: self.last_price,
            created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        };
        if let Some((reason_code, reason)) =
            self.evaluate_strategy_limits(&intent.source_tag, intent.created_at_ms)
        {
            return Ok(Some(OrderUpdate::Rejected {
                intent_id: intent.intent_id.clone(),
                client_order_id: "n/a".to_string(),
                reason_code,
                reason,
            }));
        }
        let decision = self
            .risk_module
            .evaluate_intent(&intent, &self.balances)
            .await?;
        if !decision.approved {
            return Ok(Some(OrderUpdate::Rejected {
                intent_id: intent.intent_id.clone(),
                client_order_id: "n/a".to_string(),
                reason_code: decision
                    .reason_code
                    .unwrap_or_else(|| RejectionReasonCode::RiskUnknown.as_str().to_string()),
                reason: decision
                    .reason
                    .unwrap_or_else(|| "Rejected by RiskModule".to_string()),
            }));
        }
        if !self.risk_module.reserve_rate_budget() {
            return Ok(Some(OrderUpdate::Rejected {
                intent_id: intent.intent_id.clone(),
                client_order_id: "n/a".to_string(),
                reason_code: RejectionReasonCode::RateGlobalBudgetExceeded
                    .as_str()
                    .to_string(),
                reason: "Global rate budget exceeded; try again after reset".to_string(),
            }));
        }
        if !self
            .risk_module
            .reserve_endpoint_budget(ApiEndpointGroup::Orders)
        {
            return Ok(Some(OrderUpdate::Rejected {
                intent_id: intent.intent_id.clone(),
                client_order_id: "n/a".to_string(),
                reason_code: RejectionReasonCode::RateEndpointBudgetExceeded
                    .as_str()
                    .to_string(),
                reason: "Orders endpoint budget exceeded; try again after reset".to_string(),
            }));
        }
        let qty = decision.normalized_qty;
        if let Some((reason_code, reason)) = self.evaluate_symbol_exposure_limit(side, qty) {
            return Ok(Some(OrderUpdate::Rejected {
                intent_id: intent.intent_id.clone(),
                client_order_id: "n/a".to_string(),
                reason_code,
                reason,
            }));
        }
        self.mark_strategy_submit(&intent.source_tag, intent.created_at_ms);

        let client_order_id = format!(
            "sq-{}-{}",
            intent.source_tag,
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        let order = Order {
            client_order_id: client_order_id.clone(),
            server_order_id: None,
            symbol: self.symbol.clone(),
            side,
            order_type: OrderType::Market,
            quantity: qty,
            price: None,
            status: OrderStatus::PendingSubmit,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            fills: vec![],
        };

        self.active_orders.insert(client_order_id.clone(), order);

        tracing::info!(
            side = %side,
            qty,
            usdt_amount = intent.order_amount_usdt,
            price = intent.last_price,
            intent_id = %intent.intent_id,
            created_at_ms = intent.created_at_ms,
            "Submitting order"
        );

        let submit_res = if self.market == MarketKind::Futures {
            self.rest_client
                .place_futures_market_order(&self.symbol, side, qty, &client_order_id)
                .await
        } else {
            self.rest_client
                .place_market_order(&self.symbol, side, qty, &client_order_id)
                .await
        };

        match submit_res {
            Ok(response) => {
                let update = self.process_order_response(
                    &intent.intent_id,
                    &client_order_id,
                    side,
                    &response,
                );

                // Refresh balances after fill
                if matches!(update, OrderUpdate::Filled { .. }) {
                    if let Err(e) = self.refresh_balances().await {
                        tracing::warn!(error = %e, "Failed to refresh balances after fill");
                    }
                }

                Ok(Some(update))
            }
            Err(e) => {
                tracing::error!(
                    client_order_id,
                    error = %e,
                    "Order rejected"
                );
                if let Some(order) = self.active_orders.get_mut(&client_order_id) {
                    order.status = OrderStatus::Rejected;
                    order.updated_at = chrono::Utc::now();
                }
                Ok(Some(OrderUpdate::Rejected {
                    intent_id: intent.intent_id.clone(),
                    client_order_id,
                    reason_code: RejectionReasonCode::BrokerSubmitFailed.as_str().to_string(),
                    reason: e.to_string(),
                }))
            }
        }
    }

    fn process_order_response(
        &mut self,
        intent_id: &str,
        client_order_id: &str,
        side: OrderSide,
        response: &BinanceOrderResponse,
    ) -> OrderUpdate {
        let fills: Vec<Fill> = response
            .fills
            .iter()
            .map(|f| Fill {
                price: f.price,
                qty: f.qty,
                commission: f.commission,
                commission_asset: f.commission_asset.clone(),
            })
            .collect();

        let status = OrderStatus::from_binance_str(&response.status);

        if let Some(order) = self.active_orders.get_mut(client_order_id) {
            order.server_order_id = Some(response.order_id);
            order.status = status;
            order.fills = fills.clone();
            order.updated_at = chrono::Utc::now();
        }

        if status == OrderStatus::Filled || status == OrderStatus::PartiallyFilled {
            self.position.apply_fill(side, &fills);

            let avg_price = if fills.is_empty() {
                0.0
            } else {
                let total_value: f64 = fills.iter().map(|f| f.price * f.qty).sum();
                let total_qty: f64 = fills.iter().map(|f| f.qty).sum();
                total_value / total_qty
            };

            tracing::info!(
                client_order_id,
                order_id = response.order_id,
                side = %side,
                avg_price,
                filled_qty = response.executed_qty,
                "Order filled"
            );

            OrderUpdate::Filled {
                intent_id: intent_id.to_string(),
                client_order_id: client_order_id.to_string(),
                side,
                fills,
                avg_price,
            }
        } else {
            OrderUpdate::Submitted {
                intent_id: intent_id.to_string(),
                client_order_id: client_order_id.to_string(),
                server_order_id: response.order_id,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compute_trade_stats_by_source, display_qty_for_history, split_symbol_assets, OrderManager,
    };
    use crate::binance::types::BinanceMyTrade;
    use crate::binance::rest::BinanceRestClient;
    use crate::config::{EndpointRateLimitConfig, RiskConfig, SymbolExposureLimitConfig};
    use crate::model::order::{Order, OrderSide, OrderStatus, OrderType};
    use std::sync::Arc;

    fn build_test_order_manager() -> OrderManager {
        let rest = Arc::new(BinanceRestClient::new(
            "https://demo-api.binance.com",
            "https://demo-fapi.binance.com",
            "k",
            "s",
            "fk",
            "fs",
            5000,
        ));
        let risk = RiskConfig {
            global_rate_limit_per_minute: 600,
            default_strategy_cooldown_ms: 3_000,
            default_strategy_max_active_orders: 1,
            default_symbol_max_exposure_usdt: 200.0,
            strategy_limits: vec![],
            symbol_exposure_limits: vec![SymbolExposureLimitConfig {
                symbol: "BTCUSDT".to_string(),
                market: Some("spot".to_string()),
                max_exposure_usdt: 150.0,
            }],
            endpoint_rate_limits: EndpointRateLimitConfig {
                orders_per_minute: 240,
                account_per_minute: 180,
                market_data_per_minute: 360,
            },
        };
        OrderManager::new(
            rest,
            "BTCUSDT",
            crate::order_manager::MarketKind::Spot,
            10.0,
            &risk,
        )
    }

    #[test]
    fn valid_state_transitions() {
        // PendingSubmit -> Submitted
        let from = OrderStatus::PendingSubmit;
        let to = OrderStatus::Submitted;
        assert!(!from.is_terminal());
        assert!(!to.is_terminal());

        // Submitted -> Filled
        let to = OrderStatus::Filled;
        assert!(to.is_terminal());

        // Submitted -> Rejected
        let to = OrderStatus::Rejected;
        assert!(to.is_terminal());

        // Submitted -> Cancelled
        let to = OrderStatus::Cancelled;
        assert!(to.is_terminal());
    }

    #[test]
    fn from_binance_str_mapping() {
        assert_eq!(OrderStatus::from_binance_str("NEW"), OrderStatus::Submitted);
        assert_eq!(OrderStatus::from_binance_str("FILLED"), OrderStatus::Filled);
        assert_eq!(
            OrderStatus::from_binance_str("CANCELED"),
            OrderStatus::Cancelled
        );
        assert_eq!(
            OrderStatus::from_binance_str("REJECTED"),
            OrderStatus::Rejected
        );
        assert_eq!(
            OrderStatus::from_binance_str("EXPIRED"),
            OrderStatus::Expired
        );
        assert_eq!(
            OrderStatus::from_binance_str("PARTIALLY_FILLED"),
            OrderStatus::PartiallyFilled
        );
    }

    #[test]
    fn order_history_uses_executed_qty_for_filled_states() {
        assert!((display_qty_for_history("FILLED", 1.0, 0.4) - 0.4).abs() < f64::EPSILON);
        assert!((display_qty_for_history("PARTIALLY_FILLED", 1.0, 0.4) - 0.4).abs() < f64::EPSILON);
    }

    #[test]
    fn order_history_uses_orig_qty_for_non_filled_states() {
        assert!((display_qty_for_history("NEW", 1.0, 0.4) - 1.0).abs() < f64::EPSILON);
        assert!((display_qty_for_history("CANCELED", 1.0, 0.4) - 1.0).abs() < f64::EPSILON);
        assert!((display_qty_for_history("REJECTED", 1.0, 0.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn split_symbol_assets_parses_known_quote_suffixes() {
        assert_eq!(
            split_symbol_assets("ETHUSDT"),
            ("ETH".to_string(), "USDT".to_string())
        );
        assert_eq!(
            split_symbol_assets("ETHBTC"),
            ("ETH".to_string(), "BTC".to_string())
        );
    }

    #[test]
    fn split_symbol_assets_falls_back_when_quote_unknown() {
        assert_eq!(
            split_symbol_assets("FOOBAR"),
            ("FOOBAR".to_string(), String::new())
        );
    }

    #[test]
    fn strategy_limit_rejects_when_active_orders_reach_limit() {
        let mut mgr = build_test_order_manager();
        let client_order_id = "sq-cfg-abcdef12".to_string();
        mgr.active_orders.insert(
            client_order_id.clone(),
            Order {
                client_order_id,
                server_order_id: None,
                symbol: "BTCUSDT".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                quantity: 0.1,
                price: None,
                status: OrderStatus::Submitted,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                fills: vec![],
            },
        );

        let rejected = mgr
            .evaluate_strategy_limits("cfg", chrono::Utc::now().timestamp_millis() as u64)
            .expect("must be rejected");
        assert_eq!(
            rejected.0,
            "risk.strategy_max_active_orders_exceeded".to_string()
        );
    }

    #[test]
    fn strategy_limit_rejects_during_cooldown_window() {
        let mut mgr = build_test_order_manager();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        mgr.mark_strategy_submit("cfg", now);

        let rejected = mgr
            .evaluate_strategy_limits("cfg", now + 500)
            .expect("must be rejected");
        assert_eq!(rejected.0, "risk.strategy_cooldown_active".to_string());
    }

    #[test]
    fn symbol_exposure_limit_rejects_when_projected_notional_exceeds_limit() {
        let mut mgr = build_test_order_manager();
        mgr.last_price = 100.0;
        // Buy 2.0 -> projected notional 200, but configured spot BTCUSDT limit is 150.
        let rejected = mgr
            .evaluate_symbol_exposure_limit(OrderSide::Buy, 2.0)
            .expect("must be rejected");
        assert_eq!(
            rejected.0,
            "risk.symbol_exposure_limit_exceeded".to_string()
        );
    }

    #[test]
    fn symbol_exposure_limit_allows_risk_reducing_order() {
        let mut mgr = build_test_order_manager();
        mgr.last_price = 100.0;
        mgr.position.side = Some(OrderSide::Buy);
        mgr.position.qty = 2.0; // current notional 200 > limit 150

        // Sell reduces exposure to 100; should be allowed.
        let rejected = mgr.evaluate_symbol_exposure_limit(OrderSide::Sell, 1.0);
        assert!(rejected.is_none());
    }

    #[test]
    fn futures_trade_stats_by_source_use_realized_pnl() {
        let trades = vec![
            BinanceMyTrade {
                symbol: "XRPUSDT".to_string(),
                id: 1,
                order_id: 1001,
                price: 1.0,
                qty: 100.0,
                commission: 0.0,
                commission_asset: "USDT".to_string(),
                time: 1,
                is_buyer: false,
                is_maker: false,
                realized_pnl: 5.0,
            },
            BinanceMyTrade {
                symbol: "XRPUSDT".to_string(),
                id: 2,
                order_id: 1002,
                price: 1.0,
                qty: 100.0,
                commission: 0.0,
                commission_asset: "USDT".to_string(),
                time: 2,
                is_buyer: false,
                is_maker: false,
                realized_pnl: -2.5,
            },
        ];
        let mut source_by_order = std::collections::HashMap::new();
        source_by_order.insert(1001, "c20".to_string());
        source_by_order.insert(1002, "c20".to_string());

        let stats = compute_trade_stats_by_source(trades, &source_by_order, "XRPUSDT#FUT");
        let c20 = stats.get("c20").expect("source tag must exist");
        assert_eq!(c20.trade_count, 2);
        assert_eq!(c20.win_count, 1);
        assert_eq!(c20.lose_count, 1);
        assert!((c20.realized_pnl - 2.5).abs() < f64::EPSILON);
    }
}
