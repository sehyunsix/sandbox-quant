use std::env;
use std::fs::File;
use std::path::Path;
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
use crate::record::manager::RecordManager;
use crate::storage::event_log::EventLog;
use crate::strategy::store::StrategyStore;

#[derive(Debug)]
pub struct AppBootstrap<E: ExchangeFacade> {
    pub exchange: E,
    pub mode: BinanceMode,
    pub portfolio_store: PortfolioStateStore,
    pub price_store: PriceStore,
    pub event_log: EventLog,
    pub execution: ExecutionService,
    pub portfolio_sync: PortfolioSyncService,
    pub market_data: MarketDataService,
    pub record_manager: RecordManager,
    pub strategy_store: StrategyStore,
}

impl<E: ExchangeFacade> AppBootstrap<E> {
    pub fn new(exchange: E, portfolio_store: PortfolioStateStore) -> Self {
        Self {
            exchange,
            mode: BinanceMode::Demo,
            portfolio_store,
            price_store: PriceStore::default(),
            event_log: EventLog::default(),
            execution: ExecutionService::default(),
            portfolio_sync: PortfolioSyncService,
            market_data: MarketDataService,
            record_manager: RecordManager::default(),
            strategy_store: StrategyStore::default(),
        }
    }
}

impl AppBootstrap<BinanceExchange> {
    /// Builds the real Binance-backed app bootstrap from environment variables.
    ///
    /// Required:
    /// - `BINANCE_DEMO_API_KEY` and `BINANCE_DEMO_SECRET_KEY` when `BINANCE_MODE=demo`
    /// - `BINANCE_REAL_API_KEY` and `BINANCE_REAL_SECRET_KEY` when `BINANCE_MODE=real`
    ///
    /// Optional:
    /// - `BINANCE_API_KEY`
    /// - `BINANCE_SECRET_KEY`
    /// - `BINANCE_SPOT_BASE_URL`
    /// - `BINANCE_FUTURES_BASE_URL`
    /// - `BINANCE_OPTIONS_BASE_URL`
    /// - `BINANCE_MODE`
    pub fn from_env(portfolio_store: PortfolioStateStore) -> Result<Self, ExchangeError> {
        let config = BinanceEnvConfig::from_env()?;
        let mut app = Self::new(
            BinanceExchange::new(config.build_transport()),
            portfolio_store,
        );
        app.mode = config.mode;
        Ok(app)
    }

    pub fn switch_mode(&mut self, mode: BinanceMode) -> Result<(), ExchangeError> {
        let mut config = BinanceEnvConfig::from_mode(mode)?;
        config.spot_base_url = None;
        config.futures_base_url = None;
        config.options_base_url = None;
        self.exchange = BinanceExchange::new(config.build_transport());
        self.mode = mode;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    pub options_base_url: Option<String>,
}

impl BinanceEnvConfig {
    pub fn from_env() -> Result<Self, ExchangeError> {
        Self::from_mode(Self::mode_from_env())
    }

    pub fn from_mode(mode: BinanceMode) -> Result<Self, ExchangeError> {
        let (api_key_var, secret_key_var) = mode.credentials_env_names();
        let api_key = Self::read_required_env(api_key_var, "BINANCE_API_KEY")?;
        let secret_key = Self::read_required_env(secret_key_var, "BINANCE_SECRET_KEY")?;
        Ok(Self {
            api_key,
            secret_key,
            mode,
            spot_base_url: Self::read_env_value("BINANCE_SPOT_BASE_URL"),
            futures_base_url: Self::read_env_value("BINANCE_FUTURES_BASE_URL"),
            options_base_url: Self::read_env_value("BINANCE_OPTIONS_BASE_URL"),
        })
    }

    fn mode_from_env() -> BinanceMode {
        let mode = Self::read_env_value("BINANCE_MODE").unwrap_or_else(|| "demo".to_string());
        match mode.to_ascii_lowercase().as_str() {
            "demo" => BinanceMode::Demo,
            _ => BinanceMode::Real,
        }
    }

    fn read_required_env(
        primary: &'static str,
        fallback: &'static str,
    ) -> Result<String, ExchangeError> {
        Self::read_env_value(primary)
            .or_else(|| Self::read_env_value(fallback))
            .ok_or(ExchangeError::MissingConfiguration(primary))
    }

    fn read_env_value(key: &'static str) -> Option<String> {
        env::var(key).ok().or_else(|| Self::read_dotenv_value(key))
    }

    fn read_dotenv_value(key: &'static str) -> Option<String> {
        if env::var_os("SANDBOX_QUANT_DISABLE_DOTENV").is_some() {
            return None;
        }

        Self::find_in_dotenv_iter(dotenvy::from_filename_iter(".env").ok(), key).or_else(|| {
            let manifest_dotenv = Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
            Self::find_in_dotenv_iter(dotenvy::from_path_iter(&manifest_dotenv).ok(), key)
        })
    }

    fn find_in_dotenv_iter(iter: Option<dotenvy::Iter<File>>, key: &'static str) -> Option<String> {
        iter.and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .find_map(|(entry_key, entry_value)| {
                    if entry_key == key {
                        Some(entry_value)
                    } else {
                        None
                    }
                })
        })
    }

    pub fn build_transport(&self) -> Arc<dyn BinanceTransport> {
        let auth = BinanceAuth::new(self.api_key.clone(), self.secret_key.clone());
        match (
            &self.spot_base_url,
            &self.futures_base_url,
            &self.options_base_url,
        ) {
            (Some(spot), Some(futures), Some(options)) => {
                Arc::new(BinanceHttpTransport::with_base_urls(
                    auth,
                    spot.clone(),
                    futures.clone(),
                    options.clone(),
                ))
            }
            _ => match self.mode {
                BinanceMode::Real => Arc::new(BinanceHttpTransport::new(auth)),
                BinanceMode::Demo => Arc::new(BinanceDemoHttpTransport::new(auth)),
            },
        }
    }
}

impl BinanceMode {
    fn credentials_env_names(self) -> (&'static str, &'static str) {
        match self {
            Self::Real => ("BINANCE_REAL_API_KEY", "BINANCE_REAL_SECRET_KEY"),
            Self::Demo => ("BINANCE_DEMO_API_KEY", "BINANCE_DEMO_SECRET_KEY"),
        }
    }
}
