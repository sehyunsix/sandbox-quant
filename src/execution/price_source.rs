use crate::domain::instrument::Instrument;

pub trait PriceSource {
    fn current_price(&self, instrument: &Instrument) -> Option<f64>;
}
