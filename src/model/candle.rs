#[derive(Debug, Clone)]
pub struct Candle {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub open_time: u64,
    pub close_time: u64,
}

impl Candle {
    pub fn is_bullish(&self) -> bool {
        self.close >= self.open
    }
}

/// Aggregates real-time trade ticks into a single candle over a time interval.
#[derive(Debug, Clone)]
pub struct CandleBuilder {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub open_time: u64,
    pub close_time: u64,
}

impl CandleBuilder {
    /// Start a new candle. The bucket is aligned to the interval.
    pub fn new(price: f64, timestamp_ms: u64, interval_ms: u64) -> Self {
        assert!(interval_ms > 0, "interval_ms must be > 0");
        let open_time = timestamp_ms - (timestamp_ms % interval_ms);
        Self {
            open: price,
            high: price,
            low: price,
            close: price,
            open_time,
            close_time: open_time + interval_ms,
        }
    }

    /// Update the candle with a new trade price.
    pub fn update(&mut self, price: f64) {
        self.high = self.high.max(price);
        self.low = self.low.min(price);
        self.close = price;
    }

    /// Check if a timestamp belongs to this candle's time bucket.
    pub fn contains(&self, timestamp_ms: u64) -> bool {
        timestamp_ms >= self.open_time && timestamp_ms < self.close_time
    }

    /// Finalize into an immutable Candle.
    pub fn finish(&self) -> Candle {
        Candle {
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            open_time: self.open_time,
            close_time: self.close_time,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candle_builder_basics() {
        let mut cb = CandleBuilder::new(100.0, 60_500, 60_000);
        assert_eq!(cb.open_time, 60_000);
        assert_eq!(cb.close_time, 120_000);
        assert!(cb.contains(60_500));
        assert!(cb.contains(119_999));
        assert!(!cb.contains(120_000));

        cb.update(105.0);
        cb.update(95.0);
        cb.update(102.0);

        let candle = cb.finish();
        assert!((candle.open - 100.0).abs() < f64::EPSILON);
        assert!((candle.high - 105.0).abs() < f64::EPSILON);
        assert!((candle.low - 95.0).abs() < f64::EPSILON);
        assert!((candle.close - 102.0).abs() < f64::EPSILON);
        assert!(candle.is_bullish());
    }

    #[test]
    fn bearish_candle() {
        let candle = Candle {
            open: 100.0,
            high: 105.0,
            low: 90.0,
            close: 95.0,
            open_time: 0,
            close_time: 60_000,
        };
        assert!(!candle.is_bullish());
    }

    #[test]
    #[should_panic(expected = "interval_ms must be > 0")]
    fn candle_builder_rejects_zero_interval() {
        let _ = CandleBuilder::new(100.0, 60_500, 0);
    }
}
