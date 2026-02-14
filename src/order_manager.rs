use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::TimeZone;

use crate::binance::rest::BinanceRestClient;
use crate::binance::types::BinanceOrderResponse;
use crate::model::order::{Fill, Order, OrderSide, OrderStatus, OrderType};
use crate::model::position::Position;
use crate::model::signal::Signal;

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

#[derive(Debug, Clone)]
pub struct HistoricalFill {
    pub timestamp_ms: u64,
    pub side: OrderSide,
    pub qty: f64,
    pub avg_price: f64,
}

#[derive(Debug, Clone)]
pub struct OrderHistorySnapshot {
    pub rows: Vec<String>,
    pub fills: Vec<HistoricalFill>,
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

    /// Fetch full order history from exchange and build UI rows + fill marker metadata.
    /// `page_size` is per-request size for paginated exchange calls.
    pub async fn refresh_order_history(&self, page_size: usize) -> Result<OrderHistorySnapshot> {
        let mut orders = self
            .rest_client
            .get_all_orders(&self.symbol, page_size)
            .await?;
        orders.sort_by_key(|o| o.update_time.max(o.time));

        let mut history = Vec::with_capacity(orders.len());
        let mut fills = Vec::new();
        for o in orders {
            let ts = o.update_time.max(o.time);
            let avg_price = if o.executed_qty > 0.0 {
                o.cummulative_quote_qty / o.executed_qty
            } else {
                o.price
            };
            let side = match o.side.as_str() {
                "BUY" => Some(OrderSide::Buy),
                "SELL" => Some(OrderSide::Sell),
                _ => None,
            };

            if o.status == "FILLED" && o.executed_qty > 0.0 {
                if let Some(side) = side {
                    fills.push(HistoricalFill {
                        timestamp_ms: ts,
                        side,
                        qty: o.executed_qty,
                        avg_price,
                    });
                }
            }

            let time_str = chrono::Utc
                .timestamp_millis_opt(ts as i64)
                .single()
                .map(|dt| {
                    dt.with_timezone(&chrono::Local)
                        .format("%H:%M:%S")
                        .to_string()
                })
                .unwrap_or_else(|| "--:--:--".to_string());

            history.push(format!(
                "{} {:<10} {:<4} {:.5} @ {:.2}  {}",
                time_str,
                o.status,
                o.side,
                display_qty_for_history(&o.status, o.orig_qty, o.executed_qty),
                avg_price,
                o.client_order_id
            ));
        }

        Ok(OrderHistorySnapshot {
            rows: history,
            fills,
        })
    }

    pub async fn submit_order(&mut self, signal: Signal) -> Result<Option<OrderUpdate>> {
        let (side, trace_id) = match signal {
            Signal::Buy { trace_id } => (OrderSide::Buy, trace_id),
            Signal::Sell { trace_id } => (OrderSide::Sell, trace_id),
            Signal::Hold => return Ok(None),
        };
        let client_order_id = trace_id.to_string();

        if self.last_price <= 0.0 {
            return Ok(Some(OrderUpdate::Rejected {
                client_order_id,
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
                        client_order_id,
                        reason: "No position to sell".to_string(),
                    }));
                }
                // Round position qty to 5 decimal places
                (self.position.qty * 100_000.0).floor() / 100_000.0
            }
        };

        if qty <= 0.0 {
            return Ok(Some(OrderUpdate::Rejected {
                client_order_id,
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
                        client_order_id,
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
                        client_order_id,
                        reason: format!("Insufficient BTC: need {:.5}, have {:.5}", qty, btc_free),
                    }));
                }
            }
        }

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
            trace_id = %trace_id,
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
                        tracing::warn!(trace_id = %trace_id, error = %e, "Failed to refresh balances after fill");
                    }
                }

                Ok(Some(update))
            }
            Err(e) => {
                tracing::error!(
                    trace_id = %trace_id,
                    error = %e,
                    "Order rejected by exchange"
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
                trace_id = client_order_id,
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
            tracing::info!(
                trace_id = client_order_id,
                order_id = response.order_id,
                status = response.status,
                "Order submitted"
            );
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
        assert!(
            (display_qty_for_history("PARTIALLY_FILLED", 1.0, 0.4) - 0.4).abs() < f64::EPSILON
        );
    }

    #[test]
    fn order_history_uses_orig_qty_for_non_filled_states() {
        assert!((display_qty_for_history("NEW", 1.0, 0.4) - 1.0).abs() < f64::EPSILON);
        assert!((display_qty_for_history("CANCELED", 1.0, 0.4) - 1.0).abs() < f64::EPSILON);
        assert!((display_qty_for_history("REJECTED", 1.0, 0.0) - 1.0).abs() < f64::EPSILON);
    }
}
