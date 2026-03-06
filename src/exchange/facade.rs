use crate::domain::instrument::Instrument;
use crate::domain::market::Market;
use crate::exchange::symbol_rules::SymbolRules;
use crate::exchange::types::{
    AuthoritativeSnapshot, CloseOrderAccepted, CloseOrderRequest, SubmitOrderAccepted,
};

pub trait ExchangeFacade {
    type Error;

    fn load_authoritative_snapshot(&self) -> Result<AuthoritativeSnapshot, Self::Error>;
    fn load_last_price(
        &self,
        instrument: &Instrument,
        market: Market,
    ) -> Result<f64, Self::Error>;
    fn load_symbol_rules(
        &self,
        instrument: &Instrument,
        market: Market,
    ) -> Result<SymbolRules, Self::Error>;
    fn submit_close_order(
        &self,
        request: CloseOrderRequest,
    ) -> Result<CloseOrderAccepted, Self::Error>;
    fn submit_order(&self, request: CloseOrderRequest) -> Result<SubmitOrderAccepted, Self::Error>;
}
