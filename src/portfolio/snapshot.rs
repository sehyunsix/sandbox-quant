use std::collections::BTreeMap;

use crate::domain::balance::BalanceSnapshot;
use crate::domain::instrument::Instrument;
use crate::domain::order::OpenOrder;
use crate::domain::position::PositionSnapshot;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PortfolioStateSnapshot {
    pub balances: Vec<BalanceSnapshot>,
    pub positions: BTreeMap<Instrument, PositionSnapshot>,
    pub open_orders: BTreeMap<Instrument, Vec<OpenOrder>>,
}
