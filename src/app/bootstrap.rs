use std::env;

use crate::error::exchange_error::ExchangeError;
use crate::exchange::binance::auth::BinanceAuth;
use crate::exchange::binance::client::{BinanceExchange, BinanceHttpTransport};
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

impl AppBootstrap<BinanceExchange> {
    /// Builds the real Binance-backed app bootstrap from environment variables.
    ///
    /// Required:
    /// - `BINANCE_API_KEY`
    /// - `BINANCE_SECRET_KEY`
    ///
    /// Optional:
    /// - `BINANCE_SPOT_BASE_URL`
    /// - `BINANCE_FUTURES_BASE_URL`
    pub fn from_env(portfolio_store: PortfolioStateStore) -> Result<Self, ExchangeError> {
        let api_key = env::var("BINANCE_API_KEY")
            .map_err(|_| ExchangeError::MissingConfiguration("BINANCE_API_KEY"))?;
        let secret_key = env::var("BINANCE_SECRET_KEY")
            .map_err(|_| ExchangeError::MissingConfiguration("BINANCE_SECRET_KEY"))?;
        let auth = BinanceAuth::new(api_key, secret_key);
        let transport = match (
            env::var("BINANCE_SPOT_BASE_URL").ok(),
            env::var("BINANCE_FUTURES_BASE_URL").ok(),
        ) {
            (Some(spot), Some(futures)) => {
                BinanceHttpTransport::with_base_urls(auth, spot, futures)
            }
            _ => BinanceHttpTransport::new(auth),
        };
        Ok(Self::new(
            BinanceExchange::new(std::sync::Arc::new(transport)),
            portfolio_store,
        ))
    }
}
