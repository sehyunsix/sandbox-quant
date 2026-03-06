use std::collections::BTreeMap;

use crate::domain::instrument::Instrument;
use crate::execution::price_source::PriceSource;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PriceStore {
    prices: BTreeMap<Instrument, f64>,
}

impl PriceStore {
    pub fn set_price(&mut self, instrument: Instrument, price: f64) {
        if price > f64::EPSILON {
            self.prices.insert(instrument, price);
        }
    }
}

impl PriceSource for PriceStore {
    fn current_price(&self, instrument: &Instrument) -> Option<f64> {
        self.prices.get(instrument).copied()
    }
}
