use std::sync::Arc;

use crate::domain::instrument::Instrument;
use crate::domain::market::Market;
use crate::error::exchange_error::ExchangeError;
use crate::exchange::binance::account::RawAccountState;
use crate::exchange::binance::mapper::BinanceMapper;
use crate::exchange::binance::orders::{RawCloseOrderAck, RawCloseOrderRequest, RawSymbolRules};
use crate::exchange::facade::ExchangeFacade;
use crate::exchange::symbol_rules::SymbolRules;
use crate::exchange::types::{AuthoritativeSnapshot, CloseOrderAccepted, CloseOrderRequest};

pub trait BinanceTransport: Send + Sync {
    fn load_account_state(&self, market: Market) -> Result<RawAccountState, ExchangeError>;
    fn load_symbol_rules(
        &self,
        symbol: &str,
        market: Market,
    ) -> Result<RawSymbolRules, ExchangeError>;
    fn submit_close_order(
        &self,
        request: RawCloseOrderRequest,
    ) -> Result<RawCloseOrderAck, ExchangeError>;
}

#[derive(Clone)]
pub struct BinanceExchange {
    transport: Arc<dyn BinanceTransport>,
    mapper: BinanceMapper,
}

impl BinanceExchange {
    pub fn new(transport: Arc<dyn BinanceTransport>) -> Self {
        Self {
            transport,
            mapper: BinanceMapper,
        }
    }
}

impl ExchangeFacade for BinanceExchange {
    type Error = ExchangeError;

    fn load_authoritative_snapshot(&self) -> Result<AuthoritativeSnapshot, Self::Error> {
        let mut spot = self
            .mapper
            .map_account_snapshot(Market::Spot, self.transport.load_account_state(Market::Spot)?);
        let futures = self.mapper.map_account_snapshot(
            Market::Futures,
            self.transport.load_account_state(Market::Futures)?,
        );
        spot.positions.extend(futures.positions);
        spot.balances.extend(futures.balances);
        Ok(spot)
    }

    fn load_symbol_rules(
        &self,
        instrument: &Instrument,
        market: Market,
    ) -> Result<SymbolRules, Self::Error> {
        let rules = self.transport.load_symbol_rules(&instrument.0, market)?;
        Ok(self.mapper.map_symbol_rules(rules))
    }

    fn submit_close_order(
        &self,
        request: CloseOrderRequest,
    ) -> Result<CloseOrderAccepted, Self::Error> {
        let raw = self.mapper.map_close_request(request);
        let ack = self.transport.submit_close_order(raw)?;
        Ok(self.mapper.map_close_ack(ack))
    }
}
