use std::collections::BTreeMap;

use crate::domain::instrument::Instrument;
use crate::exchange::types::AuthoritativeSnapshot;
use crate::portfolio::snapshot::PortfolioStateSnapshot;

pub fn apply_authoritative_snapshot(snapshot: AuthoritativeSnapshot) -> PortfolioStateSnapshot {
    let positions = snapshot
        .positions
        .into_iter()
        .map(|position| (position.instrument.clone(), position))
        .collect();

    let mut open_orders: BTreeMap<Instrument, Vec<_>> = BTreeMap::new();
    for order in snapshot.open_orders {
        open_orders
            .entry(order.instrument.clone())
            .or_default()
            .push(order);
    }

    PortfolioStateSnapshot {
        balances: snapshot.balances,
        positions,
        open_orders,
        market_prices: BTreeMap::new(),
    }
}
