use crate::v1::domain::instrument::Instrument;
use crate::v1::domain::market::Market;
use crate::v1::exchange::symbol_rules::SymbolRules;
use crate::v1::exchange::types::{AuthoritativeSnapshot, CloseOrderAccepted, CloseOrderRequest};

pub trait ExchangeFacade {
    type Error;

    fn load_authoritative_snapshot(&self) -> Result<AuthoritativeSnapshot, Self::Error>;
    fn load_symbol_rules(
        &self,
        instrument: &Instrument,
        market: Market,
    ) -> Result<SymbolRules, Self::Error>;
    fn submit_close_order(
        &self,
        request: CloseOrderRequest,
    ) -> Result<CloseOrderAccepted, Self::Error>;
}
