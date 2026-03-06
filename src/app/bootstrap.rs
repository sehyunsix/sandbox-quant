use std::env;
use std::sync::Arc;

use crate::error::exchange_error::ExchangeError;
use crate::exchange::binance::auth::BinanceAuth;
use crate::exchange::binance::client::{BinanceExchange, BinanceHttpTransport, BinanceTransport};
use crate::exchange::binance::demo::BinanceDemoHttpTransport;
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
    /// - `BINANCE_MODE`
    pub fn from_env(portfolio_store: PortfolioStateStore) -> Result<Self, ExchangeError> {
        let config = BinanceEnvConfig::from_env()?;
        Ok(Self::new(BinanceExchange::new(config.build_transport()), portfolio_store))
    }

    pub fn switch_mode(&mut self, mode: BinanceMode) -> Result<(), ExchangeError> {
        let mut config = BinanceEnvConfig::from_env()?;
        config.mode = mode;
        config.spot_base_url = None;
        config.futures_base_url = None;
        self.exchange = BinanceExchange::new(config.build_transport());
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinanceMode {
    Real,
    Demo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinanceEnvConfig {
    pub api_key: String,
    pub secret_key: String,
    pub mode: BinanceMode,
    pub spot_base_url: Option<String>,
    pub futures_base_url: Option<String>,
}

impl BinanceEnvConfig {
    pub fn from_env() -> Result<Self, ExchangeError> {
        let api_key = env::var("BINANCE_API_KEY")
            .map_err(|_| ExchangeError::MissingConfiguration("BINANCE_API_KEY"))?;
        let secret_key = env::var("BINANCE_SECRET_KEY")
            .map_err(|_| ExchangeError::MissingConfiguration("BINANCE_SECRET_KEY"))?;
        let mode = match env::var("BINANCE_MODE")
            .unwrap_or_else(|_| "real".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "demo" => BinanceMode::Demo,
            _ => BinanceMode::Real,
        };
        Ok(Self {
            api_key,
            secret_key,
            mode,
            spot_base_url: env::var("BINANCE_SPOT_BASE_URL").ok(),
            futures_base_url: env::var("BINANCE_FUTURES_BASE_URL").ok(),
        })
    }

    pub fn build_transport(&self) -> Arc<dyn BinanceTransport> {
        let auth = BinanceAuth::new(self.api_key.clone(), self.secret_key.clone());
        match (&self.spot_base_url, &self.futures_base_url) {
            (Some(spot), Some(futures)) => {
                Arc::new(BinanceHttpTransport::with_base_urls(auth, spot.clone(), futures.clone()))
            }
            _ => match self.mode {
                BinanceMode::Real => Arc::new(BinanceHttpTransport::new(auth)),
                BinanceMode::Demo => Arc::new(BinanceDemoHttpTransport::new(auth)),
            },
        }
    }
}
