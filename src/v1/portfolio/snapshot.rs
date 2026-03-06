use std::collections::BTreeMap;

use crate::v1::domain::balance::BalanceSnapshot;
use crate::v1::domain::instrument::Instrument;
use crate::v1::domain::order::OpenOrder;
use crate::v1::domain::position::PositionSnapshot;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PortfolioStateSnapshot {
    pub balances: Vec<BalanceSnapshot>,
    pub positions: BTreeMap<Instrument, PositionSnapshot>,
    pub open_orders: BTreeMap<Instrument, Vec<OpenOrder>>,
}
