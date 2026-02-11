use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

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

pub struct OrderManager {
    rest_client: Arc<BinanceRestClient>,
    active_orders: HashMap<String, Order>,
    position: Position,
    symbol: String,
}

impl OrderManager {
    pub fn new(rest_client: Arc<BinanceRestClient>, symbol: &str) -> Self {
        Self {
            rest_client,
            active_orders: HashMap::new(),
            position: Position::new(symbol.to_string()),
            symbol: symbol.to_string(),
        }
    }

    pub fn position(&self) -> &Position {
        &self.position
    }

    pub fn update_unrealized_pnl(&mut self, current_price: f64) {
        self.position.update_unrealized_pnl(current_price);
    }

    pub async fn submit_order(&mut self, signal: Signal) -> Result<Option<OrderUpdate>> {
        let (side, qty) = match &signal {
            Signal::Buy { qty } => (OrderSide::Buy, *qty),
            Signal::Sell { qty } => (OrderSide::Sell, *qty),
            Signal::Hold => return Ok(None),
        };

        let client_order_id = format!("sq-{}", &uuid::Uuid::new_v4().to_string()[..8]);

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

        self.active_orders
            .insert(client_order_id.clone(), order);

        match self
            .rest_client
            .place_market_order(&self.symbol, side, qty, &client_order_id)
            .await
        {
            Ok(response) => {
                let update = self.process_order_response(&client_order_id, side, &response);
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
        assert_eq!(
            OrderStatus::from_binance_str("NEW"),
            OrderStatus::Submitted
        );
        assert_eq!(
            OrderStatus::from_binance_str("FILLED"),
            OrderStatus::Filled
        );
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
}
