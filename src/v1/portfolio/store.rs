use crate::v1::error::exchange_error::ExchangeError;
use crate::v1::exchange::facade::ExchangeFacade;
use crate::v1::exchange::types::AuthoritativeSnapshot;
use crate::v1::portfolio::reconcile::apply_authoritative_snapshot;
use crate::v1::portfolio::snapshot::PortfolioStateSnapshot;
use crate::v1::portfolio::staleness::StalenessState;

#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioStateStore {
    pub snapshot: PortfolioStateSnapshot,
    pub staleness: StalenessState,
}

impl Default for PortfolioStateStore {
    fn default() -> Self {
        Self {
            snapshot: PortfolioStateSnapshot::default(),
            staleness: StalenessState::Fresh,
        }
    }
}

impl PortfolioStateStore {
    pub fn apply_snapshot(&mut self, snapshot: AuthoritativeSnapshot) {
        self.snapshot = apply_authoritative_snapshot(snapshot);
        self.staleness = StalenessState::Fresh;
    }

    pub fn mark_market_data_stale(&mut self) {
        self.staleness = StalenessState::MarketDataStale;
    }

    pub fn mark_account_state_stale(&mut self) {
        self.staleness = StalenessState::AccountStateStale;
    }

    pub fn mark_reconciliation_stale(&mut self) {
        self.staleness = StalenessState::ReconciliationStale;
    }

    /// Exchange snapshots always win here.
    /// Local runtime cache is authoritative only until a fresher exchange snapshot arrives.
    pub fn refresh_from_exchange<E: ExchangeFacade<Error = ExchangeError>>(
        &mut self,
        exchange: &E,
    ) -> Result<(), ExchangeError> {
        let snapshot = exchange.load_authoritative_snapshot()?;
        self.apply_snapshot(snapshot);
        Ok(())
    }
}
