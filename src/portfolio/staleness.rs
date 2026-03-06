#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StalenessState {
    Fresh,
    MarketDataStale,
    AccountStateStale,
    ReconciliationStale,
}
