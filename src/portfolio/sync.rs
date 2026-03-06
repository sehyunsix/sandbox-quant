use crate::error::exchange_error::ExchangeError;
use crate::exchange::facade::ExchangeFacade;
use crate::portfolio::store::PortfolioStateStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncReport {
    pub positions: usize,
    pub open_order_groups: usize,
    pub balances: usize,
}

#[derive(Debug, Default)]
pub struct PortfolioSyncService;

impl PortfolioSyncService {
    /// Refreshes the authoritative portfolio snapshot from the exchange.
    ///
    /// Example:
    /// - stale local store with outdated positions
    /// - exchange returns fresh snapshot
    /// - store is overwritten and marked `Fresh`
    pub fn refresh_authoritative<E: ExchangeFacade<Error = ExchangeError>>(
        &self,
        exchange: &E,
        store: &mut PortfolioStateStore,
    ) -> Result<SyncReport, ExchangeError> {
        store.refresh_from_exchange(exchange)?;
        Ok(SyncReport {
            positions: store.snapshot.positions.len(),
            open_order_groups: store.snapshot.open_orders.len(),
            balances: store.snapshot.balances.len(),
        })
    }

    pub fn mark_market_data_stale(&self, store: &mut PortfolioStateStore) {
        store.mark_market_data_stale();
    }

    pub fn mark_account_state_stale(&self, store: &mut PortfolioStateStore) {
        store.mark_account_state_stale();
    }

    pub fn mark_reconciliation_stale(&self, store: &mut PortfolioStateStore) {
        store.mark_reconciliation_stale();
    }
}
