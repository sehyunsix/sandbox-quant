use super::sma::Sma;

/// Exponential Moving Average.
#[derive(Debug, Clone)]
pub struct Ema {
    period: usize,
    multiplier: f64,
    ema: Option<f64>,
    // Used to calculate the first EMA value (which is an SMA)
    initial_sma: Sma,
    is_initialized: bool,
}

impl Ema {
    pub fn new(period: usize) -> Self {
        assert!(period > 0, "EMA period must be > 0");
        Self {
            period,
            multiplier: 2.0 / (period as f64 + 1.0),
            ema: None,
            initial_sma: Sma::new(period),
            is_initialized: false,
        }
    }

    /// Push a new value, return the current EMA if enough data.
    pub fn push(&mut self, value: f64) -> Option<f64> {
        if self.is_initialized {
            // Standard EMA calculation
            let prev_ema = self.ema.unwrap(); // Should be safe here
            let new_ema = (value - prev_ema) * self.multiplier + prev_ema;
            self.ema = Some(new_ema);
        } else {
            // Push to SMA until it's ready
            if let Some(first_ema) = self.initial_sma.push(value) {
                // SMA is ready, this is our first EMA value
                self.ema = Some(first_ema);
                self.is_initialized = true;
            }
        }
        self.ema
    }

    pub fn value(&self) -> Option<f64> {
        self.ema
    }

    pub fn is_ready(&self) -> bool {
        self.is_initialized
    }

    pub fn period(&self) -> usize {
        self.period
    }
}
