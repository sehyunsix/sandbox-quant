use crate::v1::exchange::facade::ExchangeFacade;
use crate::v1::portfolio::store::PortfolioStateStore;

#[derive(Debug)]
pub struct AppBootstrap<E: ExchangeFacade> {
    pub exchange: E,
    pub store: PortfolioStateStore,
}

impl<E: ExchangeFacade> AppBootstrap<E> {
    pub fn new(exchange: E, store: PortfolioStateStore) -> Self {
        Self { exchange, store }
    }
}
