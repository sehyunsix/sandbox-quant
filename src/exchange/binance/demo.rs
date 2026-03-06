use crate::domain::market::Market;
use crate::error::exchange_error::ExchangeError;
use crate::exchange::binance::account::RawAccountState;
use crate::exchange::binance::auth::BinanceAuth;
use crate::exchange::binance::client::{BinanceHttpTransport, BinanceTransport};
use crate::exchange::binance::orders::{RawCloseOrderAck, RawCloseOrderRequest, RawSymbolRules};

pub const BINANCE_DEMO_SPOT_BASE_URL: &str = "https://demo-api.binance.com";
pub const BINANCE_DEMO_FUTURES_BASE_URL: &str = "https://demo-fapi.binance.com";

#[derive(Debug, Clone)]
pub struct BinanceDemoHttpTransport {
    inner: BinanceHttpTransport,
}

impl BinanceDemoHttpTransport {
    pub fn new(auth: BinanceAuth) -> Self {
        Self {
            inner: BinanceHttpTransport::with_base_urls(
                auth,
                BINANCE_DEMO_SPOT_BASE_URL,
                BINANCE_DEMO_FUTURES_BASE_URL,
            ),
        }
    }
}

impl BinanceTransport for BinanceDemoHttpTransport {
    fn load_account_state(&self, market: Market) -> Result<RawAccountState, ExchangeError> {
        self.inner.load_account_state(market)
    }

    fn load_last_price(&self, symbol: &str, market: Market) -> Result<f64, ExchangeError> {
        self.inner.load_last_price(symbol, market)
    }

    fn load_symbol_rules(
        &self,
        symbol: &str,
        market: Market,
    ) -> Result<RawSymbolRules, ExchangeError> {
        self.inner.load_symbol_rules(symbol, market)
    }

    fn submit_close_order(
        &self,
        request: RawCloseOrderRequest,
    ) -> Result<RawCloseOrderAck, ExchangeError> {
        self.inner.submit_close_order(request)
    }
}
