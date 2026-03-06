use crate::domain::instrument::Instrument;
use crate::execution::price_source::PriceSource;
use crate::market_data::price_store::PriceStore;

#[derive(Debug, Default)]
pub struct MarketDataService;

impl MarketDataService {
    /// Updates the latest known execution price for an instrument.
    ///
    /// Example:
    /// - incoming BTCUSDT tick at `50000.0`
    /// - stored as the current execution price context for BTCUSDT
    pub fn apply_price(&self, store: &mut PriceStore, instrument: Instrument, price: f64) {
        store.set_price(instrument, price);
    }

    pub fn current_price(&self, store: &impl PriceSource, instrument: &Instrument) -> Option<f64> {
        store.current_price(instrument)
    }
}
