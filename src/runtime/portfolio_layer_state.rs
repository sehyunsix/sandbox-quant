use std::collections::HashMap;

use crate::event::{AssetPnlEntry, PortfolioStateSnapshot};
use crate::model::order::OrderSide;
use crate::order_manager::{MarketKind, OrderManager};

#[derive(Debug, Clone, Default)]
pub struct PortfolioLayerState {
    pub by_symbol: HashMap<String, AssetPnlEntry>,
    pub total_realized_pnl_usdt: f64,
    pub total_unrealized_pnl_usdt: f64,
    pub open_orders_count: usize,
    pub reserved_cash_usdt: f64,
    pub gross_exposure_usdt: f64,
    pub net_exposure_usdt: f64,
}

impl PortfolioLayerState {
    pub fn to_snapshot(&self) -> PortfolioStateSnapshot {
        PortfolioStateSnapshot {
            by_symbol: self.by_symbol.clone(),
            total_realized_pnl_usdt: self.total_realized_pnl_usdt,
            total_unrealized_pnl_usdt: self.total_unrealized_pnl_usdt,
            open_orders_count: self.open_orders_count,
            reserved_cash_usdt: self.reserved_cash_usdt,
            gross_exposure_usdt: self.gross_exposure_usdt,
            net_exposure_usdt: self.net_exposure_usdt,
        }
    }
}

pub fn build_portfolio_layer_state(
    order_managers: &HashMap<String, OrderManager>,
    realized_pnl_by_symbol: &HashMap<String, f64>,
    live_futures_positions: &HashMap<String, AssetPnlEntry>,
) -> PortfolioLayerState {
    let mut by_symbol: HashMap<String, AssetPnlEntry> = order_managers
        .iter()
        .map(|(symbol, mgr)| {
            (
                symbol.clone(),
                AssetPnlEntry {
                    is_futures: mgr.market_kind() == MarketKind::Futures,
                    side: mgr.position().side,
                    position_qty: mgr.position().qty,
                    entry_price: mgr.position().entry_price,
                    realized_pnl_usdt: realized_pnl_by_symbol.get(symbol).copied().unwrap_or(0.0),
                    unrealized_pnl_usdt: mgr.position().unrealized_pnl,
                },
            )
        })
        .collect();

    // Exchange-reported futures positions override local reconstructed state when present.
    for (symbol, entry) in live_futures_positions {
        by_symbol.insert(symbol.clone(), entry.clone());
    }

    let total_realized_pnl_usdt = by_symbol.values().map(|e| e.realized_pnl_usdt).sum::<f64>();
    let total_unrealized_pnl_usdt = by_symbol
        .values()
        .map(|e| e.unrealized_pnl_usdt)
        .sum::<f64>();
    let open_orders_count = order_managers
        .values()
        .map(OrderManager::open_order_count)
        .sum::<usize>();
    let reserved_cash_usdt = order_managers
        .values()
        .map(OrderManager::reserved_cash_usdt)
        .sum::<f64>();

    let mut gross_exposure_usdt = 0.0;
    let mut net_exposure_usdt = 0.0;
    for (symbol, entry) in &by_symbol {
        let mark_price = order_managers
            .get(symbol)
            .and_then(OrderManager::last_price)
            .unwrap_or(entry.entry_price)
            .max(0.0);
        let signed_notional = entry.position_qty * mark_price;
        gross_exposure_usdt += signed_notional.abs();
        net_exposure_usdt += match entry.side {
            Some(OrderSide::Sell) => -signed_notional.abs(),
            Some(OrderSide::Buy) => signed_notional.abs(),
            None => 0.0,
        };
    }

    PortfolioLayerState {
        by_symbol,
        total_realized_pnl_usdt,
        total_unrealized_pnl_usdt,
        open_orders_count,
        reserved_cash_usdt,
        gross_exposure_usdt,
        net_exposure_usdt,
    }
}
