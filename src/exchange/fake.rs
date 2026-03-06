use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::domain::instrument::Instrument;
use crate::domain::market::Market;
use crate::error::exchange_error::ExchangeError;
use crate::exchange::facade::ExchangeFacade;
use crate::exchange::symbol_rules::SymbolRules;
use crate::exchange::types::{
    AuthoritativeSnapshot, CloseOrderAccepted, CloseOrderRequest, SubmitOrderAccepted,
};

#[derive(Debug)]
pub struct FakeExchange {
    snapshot: Mutex<AuthoritativeSnapshot>,
    symbol_rules: Mutex<BTreeMap<(Instrument, Market), SymbolRules>>,
    last_prices: Mutex<BTreeMap<(Instrument, Market), f64>>,
    close_requests: Mutex<Vec<CloseOrderRequest>>,
    submit_requests: Mutex<Vec<CloseOrderRequest>>,
    next_close_submit_result: Mutex<Option<Result<CloseOrderAccepted, ExchangeError>>>,
    next_order_submit_result: Mutex<Option<Result<SubmitOrderAccepted, ExchangeError>>>,
}

impl FakeExchange {
    pub fn new(snapshot: AuthoritativeSnapshot) -> Self {
        Self {
            snapshot: Mutex::new(snapshot),
            symbol_rules: Mutex::new(BTreeMap::new()),
            last_prices: Mutex::new(BTreeMap::new()),
            close_requests: Mutex::new(Vec::new()),
            submit_requests: Mutex::new(Vec::new()),
            next_close_submit_result: Mutex::new(None),
            next_order_submit_result: Mutex::new(None),
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

    pub fn set_last_price(&self, instrument: Instrument, market: Market, price: f64) {
        self.last_prices
            .lock()
            .expect("lock last_prices")
            .insert((instrument, market), price);
    }

    pub fn set_next_submit_result(
        &self,
        result: Result<CloseOrderAccepted, ExchangeError>,
    ) {
        *self
            .next_close_submit_result
            .lock()
            .expect("lock next_close_submit_result") = Some(result);
    }

    pub fn set_next_order_submit_result(
        &self,
        result: Result<SubmitOrderAccepted, ExchangeError>,
    ) {
        *self
            .next_order_submit_result
            .lock()
            .expect("lock next_order_submit_result") = Some(result);
    }

    pub fn close_requests(&self) -> Vec<CloseOrderRequest> {
        self.close_requests
            .lock()
            .expect("lock close_requests")
            .clone()
    }

    pub fn submit_requests(&self) -> Vec<CloseOrderRequest> {
        self.submit_requests
            .lock()
            .expect("lock submit_requests")
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

    fn load_last_price(
        &self,
        instrument: &Instrument,
        market: Market,
    ) -> Result<f64, Self::Error> {
        self.last_prices
            .lock()
            .expect("lock last_prices")
            .get(&(instrument.clone(), market))
            .copied()
            .ok_or(ExchangeError::InvalidResponse)
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
            .next_close_submit_result
            .lock()
            .expect("lock next_close_submit_result")
            .take()
        {
            result
        } else {
            Ok(CloseOrderAccepted {
                remote_order_id: "fake-close-1".to_string(),
            })
        }
    }

    fn submit_order(&self, request: CloseOrderRequest) -> Result<SubmitOrderAccepted, Self::Error> {
        self.submit_requests
            .lock()
            .expect("lock submit_requests")
            .push(request);
        if let Some(result) = self
            .next_order_submit_result
            .lock()
            .expect("lock next_order_submit_result")
            .take()
        {
            result
        } else {
            Ok(SubmitOrderAccepted {
                remote_order_id: "fake-submit-1".to_string(),
            })
        }
    }
}
