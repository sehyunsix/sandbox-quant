use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use chrono::TimeZone;

use crate::binance::rest::BinanceRestClient;
use crate::binance::types::{BinanceMyTrade, BinanceOrderResponse};
use crate::model::order::{Fill, Order, OrderSide, OrderStatus, OrderType};
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketKind {
    Spot,
    Futures,
}

#[derive(Debug, Clone)]
pub enum OrderUpdate {
    Submitted {
        client_order_id: String,
        server_order_id: u64,
    },
    Filled {
        client_order_id: String,
        side: OrderSide,
        fills: Vec<Fill>,
        avg_price: f64,
    },
    Rejected {
        client_order_id: String,
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
}

fn floor_to_step(value: f64, step: f64) -> f64 {
    if !value.is_finite() || !step.is_finite() || step <= 0.0 {
        return 0.0;
    }
    let units = (value / step).floor();
    let floored = units * step;
    if floored < 0.0 {
        0.0
    } else {
        floored
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

fn source_label_from_client_order_id(client_order_id: &str) -> &'static str {
    if client_order_id.contains("-mnl-") {
        "MANUAL"
    } else if client_order_id.contains("-cfg-") {
        "MA(Config)"
    } else if client_order_id.contains("-fst-") {
        "MA(Fast 5/20)"
    } else if client_order_id.contains("-slw-") {
        "MA(Slow 20/60)"
    } else {
        "UNKNOWN"
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

fn compute_trade_stats(mut trades: Vec<BinanceMyTrade>, symbol: &str) -> OrderHistoryStats {
    trades.sort_by_key(|t| (t.time, t.id));
    let (base_asset, quote_asset) = split_symbol_assets(symbol);
    let mut pos = LongPos::default();
    let mut stats = OrderHistoryStats::default();
    for t in trades {
        apply_spot_trade_with_fee(&mut pos, &mut stats, &t, &base_asset, &quote_asset);
    }
    stats
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

fn compute_trade_stats_by_source(
    mut trades: Vec<BinanceMyTrade>,
    order_source_by_id: &HashMap<u64, String>,
    symbol: &str,
) -> HashMap<String, OrderHistoryStats> {
    trades.sort_by_key(|t| (t.time, t.id));
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

impl OrderManager {
    pub fn new(
        rest_client: Arc<BinanceRestClient>,
        symbol: &str,
        market: MarketKind,
        order_amount_usdt: f64,
    ) -> Self {
        Self {
            rest_client,
            active_orders: HashMap::new(),
            position: Position::new(symbol.to_string()),
            symbol: symbol.to_string(),
            market,
            order_amount_usdt,
            balances: HashMap::new(),
            last_price: 0.0,
        }
    }

    pub fn position(&self) -> &Position {
        &self.position
    }

    pub fn balances(&self) -> &HashMap<String, f64> {
        &self.balances
    }

    pub fn update_unrealized_pnl(&mut self, current_price: f64) {
        self.last_price = current_price;
        self.position.update_unrealized_pnl(current_price);
    }

    /// Fetch account balances from Binance and update internal state.
    /// Returns the balances map (asset → free balance) for non-zero balances.
    pub async fn refresh_balances(&mut self) -> Result<HashMap<String, f64>> {
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
    pub async fn refresh_order_history(&self, limit: usize) -> Result<OrderHistorySnapshot> {
        if self.market == MarketKind::Futures {
            let now_ms = chrono::Utc::now().timestamp_millis() as u64;
            return Ok(OrderHistorySnapshot {
                rows: Vec::new(),
                stats: OrderHistoryStats::default(),
                strategy_stats: HashMap::new(),
                fills: Vec::new(),
                open_qty: 0.0,
                open_entry_price: 0.0,
                estimated_total_pnl_usdt: Some(0.0),
                trade_data_complete: true,
                fetched_at_ms: now_ms,
                fetch_latency_ms: 0,
                latest_event_ms: None,
            });
        }

        let fetch_started = Instant::now();
        let fetched_at_ms = chrono::Utc::now().timestamp_millis() as u64;
        let orders_result = self.rest_client.get_all_orders(&self.symbol, limit).await;
        let last_trade_id = order_store::load_last_trade_id(&self.symbol).ok().flatten();
        let persisted_trade_count = order_store::load_trade_count(&self.symbol).unwrap_or(0);
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

        if let Err(e) = order_store::persist_order_snapshot(&self.symbol, &orders, &recent_trades) {
            tracing::warn!(error = %e, "Failed to persist order snapshot to sqlite");
        }
        let mut persisted_source_by_order_id: HashMap<u64, String> = HashMap::new();
        match order_store::load_persisted_trades(&self.symbol) {
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
                source_label_from_client_order_id(&o.client_order_id).to_string(),
            );
        }
        for (order_id, source) in persisted_source_by_order_id {
            order_source_by_id.entry(order_id).or_insert(source);
        }
        let strategy_stats =
            compute_trade_stats_by_source(trades.clone(), &order_source_by_id, &self.symbol);

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

        if self.last_price <= 0.0 {
            return Ok(Some(OrderUpdate::Rejected {
                client_order_id: "n/a".to_string(),
                reason: "No price data yet".to_string(),
            }));
        }

        // Calculate quantity based on side
        let raw_qty = match side {
            OrderSide::Buy => {
                // Calculate BTC qty from USDT amount
                self.order_amount_usdt / self.last_price
            }
            OrderSide::Sell => {
                if self.market == MarketKind::Spot {
                    // Spot: sell only what we have.
                    if self.position.is_flat() {
                        return Ok(Some(OrderUpdate::Rejected {
                            client_order_id: "n/a".to_string(),
                            reason: "No position to sell".to_string(),
                        }));
                    }
                    self.position.qty
                } else {
                    // Futures: SELL may open/increase short, use notional sizing.
                    self.order_amount_usdt / self.last_price
                }
            }
        };

        let rules = if self.market == MarketKind::Futures {
            self.rest_client
                .get_futures_symbol_order_rules(&self.symbol)
                .await?
        } else {
            self.rest_client
                .get_spot_symbol_order_rules(&self.symbol)
                .await?
        };

        let qty = floor_to_step(raw_qty, rules.step_size);

        if qty <= 0.0 {
            return Ok(Some(OrderUpdate::Rejected {
                client_order_id: "n/a".to_string(),
                reason: format!(
                    "Calculated qty too small after step normalization (raw {:.8}, step {:.8})",
                    raw_qty, rules.step_size
                ),
            }));
        }
        if qty < rules.min_qty {
            return Ok(Some(OrderUpdate::Rejected {
                client_order_id: "n/a".to_string(),
                reason: format!(
                    "Qty below minQty (qty {:.8} < min {:.8}, step {:.8})",
                    qty, rules.min_qty, rules.step_size
                ),
            }));
        }
        if rules.max_qty > 0.0 && qty > rules.max_qty {
            return Ok(Some(OrderUpdate::Rejected {
                client_order_id: "n/a".to_string(),
                reason: format!(
                    "Qty above maxQty (qty {:.8} > max {:.8})",
                    qty, rules.max_qty
                ),
            }));
        }

        // Check balance before placing order
        if self.market == MarketKind::Spot {
            match side {
                OrderSide::Buy => {
                    let usdt_free = self.balances.get("USDT").copied().unwrap_or(0.0);
                    let order_value = qty * self.last_price;
                    if usdt_free < order_value {
                        return Ok(Some(OrderUpdate::Rejected {
                            client_order_id: "n/a".to_string(),
                            reason: format!(
                                "Insufficient USDT: need {:.2}, have {:.2}",
                                order_value, usdt_free
                            ),
                        }));
                    }
                }
                OrderSide::Sell => {
                    let btc_free = self.balances.get("BTC").copied().unwrap_or(0.0);
                    if btc_free < qty {
                        return Ok(Some(OrderUpdate::Rejected {
                            client_order_id: "n/a".to_string(),
                            reason: format!(
                                "Insufficient BTC: need {:.5}, have {:.5}",
                                qty, btc_free
                            ),
                        }));
                    }
                }
            }
        }

        let client_order_id = format!(
            "sq-{}-{}",
            source_tag.to_ascii_lowercase(),
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
            usdt_amount = self.order_amount_usdt,
            price = self.last_price,
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
                let update = self.process_order_response(&client_order_id, side, &response);

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
                    client_order_id,
                    reason: e.to_string(),
                }))
            }
        }
    }

    fn process_order_response(
        &mut self,
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
                client_order_id: client_order_id.to_string(),
                side,
                fills,
                avg_price,
            }
        } else {
            OrderUpdate::Submitted {
                client_order_id: client_order_id.to_string(),
                server_order_id: response.order_id,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{display_qty_for_history, floor_to_step};
    use crate::model::order::OrderStatus;

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
    fn quantity_is_floored_to_exchange_step() {
        assert!((floor_to_step(0.123456, 0.001) - 0.123).abs() < 1e-12);
        assert!((floor_to_step(0.123456, 0.0001) - 0.1234).abs() < 1e-12);
        assert!((floor_to_step(0.0009, 0.001) - 0.0).abs() < 1e-12);
    }
}
