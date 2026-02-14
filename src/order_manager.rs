use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::TimeZone;

use crate::binance::rest::BinanceRestClient;
use crate::binance::types::{BinanceMyTrade, BinanceOrderResponse};
use crate::model::order::{Fill, Order, OrderSide, OrderStatus, OrderType};
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_store;

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
    pub fetched_at_ms: u64,
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
    order_amount_usdt: f64,
    balances: HashMap<String, f64>,
    last_price: f64,
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

fn compute_trade_stats(mut trades: Vec<BinanceMyTrade>) -> OrderHistoryStats {
    trades.sort_by_key(|t| (t.time, t.id));
    let mut side: Option<OrderSide> = None;
    let mut qty = 0.0_f64;
    let mut entry_price = 0.0_f64;
    let mut stats = OrderHistoryStats::default();

    for t in trades {
        let fill_side = if t.is_buyer {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let fill_qty = t.qty.max(0.0);
        if fill_qty <= f64::EPSILON {
            continue;
        }
        let fill_price = t.price;

        match side {
            None => {
                side = Some(fill_side);
                qty = fill_qty;
                entry_price = fill_price;
            }
            Some(pos_side) if pos_side == fill_side => {
                let total_cost = entry_price * qty + fill_price * fill_qty;
                qty += fill_qty;
                if qty > f64::EPSILON {
                    entry_price = total_cost / qty;
                }
            }
            Some(pos_side) => {
                let close_qty = fill_qty.min(qty);
                let pnl_delta = match pos_side {
                    OrderSide::Buy => (fill_price - entry_price) * close_qty,
                    OrderSide::Sell => (entry_price - fill_price) * close_qty,
                };
                if pnl_delta > 0.0 {
                    stats.win_count += 1;
                    stats.trade_count += 1;
                } else if pnl_delta < 0.0 {
                    stats.lose_count += 1;
                    stats.trade_count += 1;
                }
                stats.realized_pnl += pnl_delta;

                qty -= close_qty;
                let remain_open_qty = fill_qty - close_qty;
                if qty <= f64::EPSILON {
                    side = None;
                    qty = 0.0;
                    entry_price = 0.0;
                }
                if remain_open_qty > f64::EPSILON {
                    side = Some(fill_side);
                    qty = remain_open_qty;
                    entry_price = fill_price;
                }
            }
        }
    }

    stats
}

fn compute_trade_stats_by_source(
    mut trades: Vec<BinanceMyTrade>,
    order_source_by_id: &HashMap<u64, String>,
) -> HashMap<String, OrderHistoryStats> {
    #[derive(Clone, Copy, Default)]
    struct SourcePos {
        side: Option<OrderSide>,
        qty: f64,
        entry_price: f64,
    }

    trades.sort_by_key(|t| (t.time, t.id));
    let mut pos_by_source: HashMap<String, SourcePos> = HashMap::new();
    let mut stats_by_source: HashMap<String, OrderHistoryStats> = HashMap::new();

    for t in trades {
        let source = order_source_by_id
            .get(&t.order_id)
            .cloned()
            .unwrap_or_else(|| "UNKNOWN".to_string());
        let fill_side = if t.is_buyer {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let fill_qty = t.qty.max(0.0);
        if fill_qty <= f64::EPSILON {
            continue;
        }

        let pos = pos_by_source.entry(source.clone()).or_default();
        let stats = stats_by_source.entry(source).or_default();

        match pos.side {
            None => {
                pos.side = Some(fill_side);
                pos.qty = fill_qty;
                pos.entry_price = t.price;
            }
            Some(pos_side) if pos_side == fill_side => {
                let total_cost = pos.entry_price * pos.qty + t.price * fill_qty;
                pos.qty += fill_qty;
                if pos.qty > f64::EPSILON {
                    pos.entry_price = total_cost / pos.qty;
                }
            }
            Some(pos_side) => {
                let close_qty = fill_qty.min(pos.qty);
                let pnl_delta = match pos_side {
                    OrderSide::Buy => (t.price - pos.entry_price) * close_qty,
                    OrderSide::Sell => (pos.entry_price - t.price) * close_qty,
                };
                if pnl_delta > 0.0 {
                    stats.win_count += 1;
                    stats.trade_count += 1;
                } else if pnl_delta < 0.0 {
                    stats.lose_count += 1;
                    stats.trade_count += 1;
                }
                stats.realized_pnl += pnl_delta;

                pos.qty -= close_qty;
                let remain = fill_qty - close_qty;
                if pos.qty <= f64::EPSILON {
                    pos.side = None;
                    pos.qty = 0.0;
                    pos.entry_price = 0.0;
                }
                if remain > f64::EPSILON {
                    pos.side = Some(fill_side);
                    pos.qty = remain;
                    pos.entry_price = t.price;
                }
            }
        }
    }

    stats_by_source
}

impl OrderManager {
    pub fn new(rest_client: Arc<BinanceRestClient>, symbol: &str, order_amount_usdt: f64) -> Self {
        Self {
            rest_client,
            active_orders: HashMap::new(),
            position: Position::new(symbol.to_string()),
            symbol: symbol.to_string(),
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
        let fetched_at_ms = chrono::Utc::now().timestamp_millis() as u64;
        let orders_result = self.rest_client.get_all_orders(&self.symbol, limit).await;
        let trades_result = self.rest_client.get_my_trades(&self.symbol, limit).await;

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
        let trades = match trades_result {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch myTrades; falling back to order-only history");
                Vec::new()
            }
        };
        orders.sort_by_key(|o| o.update_time.max(o.time));
        let stats = compute_trade_stats(trades.clone());
        let latest_order_event = orders.iter().map(|o| o.update_time.max(o.time)).max();
        let latest_trade_event = trades.iter().map(|t| t.time).max();
        let latest_event_ms = latest_order_event.max(latest_trade_event);

        if let Err(e) = order_store::persist_order_snapshot(&self.symbol, &orders, &trades) {
            tracing::warn!(error = %e, "Failed to persist order snapshot to sqlite");
        }

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
        let strategy_stats = compute_trade_stats_by_source(trades.clone(), &order_source_by_id);

        let mut history = Vec::new();
        let mut fills = Vec::new();
        let mut used_trade_ids = std::collections::HashSet::new();

        if orders.is_empty() && !trades.is_empty() {
            let mut sorted = trades;
            sorted.sort_by_key(|t| (t.time, t.id));
            history.extend(
                sorted.iter().map(|t| {
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
                }),
            );
            return Ok(OrderHistorySnapshot {
                rows: history,
                stats,
                strategy_stats,
                fills,
                fetched_at_ms,
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
            fetched_at_ms,
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
        let qty = match side {
            OrderSide::Buy => {
                // Calculate BTC qty from USDT amount
                let raw_qty = self.order_amount_usdt / self.last_price;
                // Round to 5 decimal places (BTCUSDT step size)
                (raw_qty * 100_000.0).floor() / 100_000.0
            }
            OrderSide::Sell => {
                // Sell what we have
                if self.position.is_flat() {
                    return Ok(Some(OrderUpdate::Rejected {
                        client_order_id: "n/a".to_string(),
                        reason: "No position to sell".to_string(),
                    }));
                }
                // Round position qty to 5 decimal places
                (self.position.qty * 100_000.0).floor() / 100_000.0
            }
        };

        if qty <= 0.0 {
            return Ok(Some(OrderUpdate::Rejected {
                client_order_id: "n/a".to_string(),
                reason: format!(
                    "Calculated qty too small ({}USDT / {:.2} = {:.8} BTC)",
                    self.order_amount_usdt, self.last_price, qty
                ),
            }));
        }

        // Check balance before placing order
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
                        reason: format!("Insufficient BTC: need {:.5}, have {:.5}", qty, btc_free),
                    }));
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

        match self
            .rest_client
            .place_market_order(&self.symbol, side, qty, &client_order_id)
            .await
        {
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
    use super::display_qty_for_history;
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
}
