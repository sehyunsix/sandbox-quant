use crate::domain::balance::BalanceSnapshot;
use crate::domain::instrument::Instrument;
use crate::domain::market::Market;
use crate::domain::order::{OpenOrder, OrderStatus};
use crate::domain::position::PositionSnapshot;
use crate::domain::position::Side;
use crate::exchange::symbol_rules::SymbolRules;
use crate::exchange::types::{AuthoritativeSnapshot, CloseOrderAccepted, CloseOrderRequest};

use crate::exchange::binance::account::{RawAccountState, RawBalance, RawPosition};
use crate::exchange::binance::orders::{
    RawCloseOrderAck, RawCloseOrderRequest, RawOpenOrder, RawSymbolRules,
};

#[derive(Debug, Default, Clone)]
pub struct BinanceMapper;

impl BinanceMapper {
    pub fn map_account_snapshot(
        &self,
        market: Market,
        state: RawAccountState,
    ) -> AuthoritativeSnapshot {
        AuthoritativeSnapshot {
            balances: state.balances.into_iter().map(map_balance).collect(),
            positions: state
                .positions
                .into_iter()
                .map(|position| map_position(market, position))
                .collect(),
            open_orders: state.open_orders.into_iter().map(map_open_order).collect(),
        }
    }

    pub fn map_symbol_rules(&self, rules: RawSymbolRules) -> SymbolRules {
        SymbolRules {
            min_qty: rules.min_qty,
            max_qty: rules.max_qty,
            step_size: rules.step_size,
        }
    }

    pub fn map_close_request(&self, request: CloseOrderRequest) -> RawCloseOrderRequest {
        RawCloseOrderRequest {
            symbol: request.instrument.0,
            market: request.market,
            side: match request.side {
                crate::domain::position::Side::Buy => "BUY",
                crate::domain::position::Side::Sell => "SELL",
            },
            qty: request.qty_text,
            order_type: request.order_type,
            reduce_only: request.reduce_only,
        }
    }

    pub fn map_close_ack(&self, ack: RawCloseOrderAck) -> CloseOrderAccepted {
        CloseOrderAccepted {
            remote_order_id: ack.remote_order_id,
        }
    }
}

fn map_balance(balance: RawBalance) -> BalanceSnapshot {
    BalanceSnapshot {
        asset: balance.asset,
        free: balance.free,
        locked: balance.locked,
    }
}

fn map_position(market: Market, position: RawPosition) -> PositionSnapshot {
    PositionSnapshot {
        instrument: Instrument::new(position.symbol),
        market,
        signed_qty: position.signed_qty,
        entry_price: position.entry_price,
    }
}

fn map_open_order(order: RawOpenOrder) -> OpenOrder {
    OpenOrder {
        order_id: order
            .order_id
            .and_then(|value| value.parse::<u64>().ok())
            .map(crate::domain::identifiers::OrderId),
        client_order_id: order.client_order_id,
        instrument: Instrument::new(order.symbol),
        market: order.market,
        side: match order.side {
            "SELL" => Side::Sell,
            _ => Side::Buy,
        },
        orig_qty: order.orig_qty,
        executed_qty: order.executed_qty,
        reduce_only: order.reduce_only,
        status: map_order_status(&order.status),
    }
}

fn map_order_status(status: &str) -> OrderStatus {
    match status {
        "NEW" | "ACCEPTED" => OrderStatus::Submitted,
        "FILLED" => OrderStatus::Filled,
        "CANCELED" | "CANCELLED" => OrderStatus::Cancelled,
        "REJECTED" => OrderStatus::Rejected,
        _ => OrderStatus::PendingSubmit,
    }
}
