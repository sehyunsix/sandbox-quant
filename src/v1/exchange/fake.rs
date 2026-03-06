use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::v1::domain::instrument::Instrument;
use crate::v1::domain::market::Market;
use crate::v1::error::exchange_error::ExchangeError;
use crate::v1::exchange::facade::ExchangeFacade;
use crate::v1::exchange::symbol_rules::SymbolRules;
use crate::v1::exchange::types::{
    AuthoritativeSnapshot, CloseOrderAccepted, CloseOrderRequest,
};

#[derive(Debug)]
pub struct FakeExchange {
    snapshot: Mutex<AuthoritativeSnapshot>,
    symbol_rules: Mutex<BTreeMap<(Instrument, Market), SymbolRules>>,
    close_requests: Mutex<Vec<CloseOrderRequest>>,
    next_submit_result: Mutex<Option<Result<CloseOrderAccepted, ExchangeError>>>,
}

impl FakeExchange {
    pub fn new(snapshot: AuthoritativeSnapshot) -> Self {
        Self {
            snapshot: Mutex::new(snapshot),
            symbol_rules: Mutex::new(BTreeMap::new()),
            close_requests: Mutex::new(Vec::new()),
            next_submit_result: Mutex::new(None),
        }
    }

    pub fn set_symbol_rules(
        &self,
        instrument: Instrument,
        market: Market,
        rules: SymbolRules,
    ) {
        self.symbol_rules
            .lock()
            .expect("lock symbol_rules")
            .insert((instrument, market), rules);
    }

    pub fn set_next_submit_result(
        &self,
        result: Result<CloseOrderAccepted, ExchangeError>,
    ) {
        *self
            .next_submit_result
            .lock()
            .expect("lock next_submit_result") = Some(result);
    }

    pub fn close_requests(&self) -> Vec<CloseOrderRequest> {
        self.close_requests
            .lock()
            .expect("lock close_requests")
            .clone()
    }

    pub fn replace_snapshot(&self, snapshot: AuthoritativeSnapshot) {
        *self.snapshot.lock().expect("lock snapshot") = snapshot;
    }
}

impl ExchangeFacade for FakeExchange {
    type Error = ExchangeError;

    fn load_authoritative_snapshot(&self) -> Result<AuthoritativeSnapshot, Self::Error> {
        Ok(self.snapshot.lock().expect("lock snapshot").clone())
    }

    fn load_symbol_rules(
        &self,
        instrument: &Instrument,
        market: Market,
    ) -> Result<SymbolRules, Self::Error> {
        self.symbol_rules
            .lock()
            .expect("lock symbol_rules")
            .get(&(instrument.clone(), market))
            .copied()
            .ok_or(ExchangeError::InvalidResponse)
    }

    fn submit_close_order(
        &self,
        request: CloseOrderRequest,
    ) -> Result<CloseOrderAccepted, Self::Error> {
        self.close_requests
            .lock()
            .expect("lock close_requests")
            .push(request);

        if let Some(result) = self
            .next_submit_result
            .lock()
            .expect("lock next_submit_result")
            .take()
        {
            result
        } else {
            Ok(CloseOrderAccepted {
                remote_order_id: "fake-close-1".to_string(),
            })
        }
    }
}
