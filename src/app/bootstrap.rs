use crate::exchange::facade::ExchangeFacade;
use crate::execution::service::ExecutionService;
use crate::market_data::price_store::PriceStore;
use crate::market_data::service::MarketDataService;
use crate::portfolio::store::PortfolioStateStore;
use crate::portfolio::sync::PortfolioSyncService;
use crate::storage::event_log::EventLog;

#[derive(Debug)]
pub struct AppBootstrap<E: ExchangeFacade> {
    pub exchange: E,
    pub portfolio_store: PortfolioStateStore,
    pub price_store: PriceStore,
    pub event_log: EventLog,
    pub execution: ExecutionService,
    pub portfolio_sync: PortfolioSyncService,
    pub market_data: MarketDataService,
}

impl<E: ExchangeFacade> AppBootstrap<E> {
    pub fn new(exchange: E, portfolio_store: PortfolioStateStore) -> Self {
        Self {
            exchange,
            portfolio_store,
            price_store: PriceStore::default(),
            event_log: EventLog::default(),
            execution: ExecutionService::default(),
            portfolio_sync: PortfolioSyncService,
            market_data: MarketDataService,
        }
    }
}
