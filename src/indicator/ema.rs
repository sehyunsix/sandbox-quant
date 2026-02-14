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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_ema() {
        let mut ema = Ema::new(3);
        assert_eq!(ema.push(2.0), None);
        assert_eq!(ema.push(5.0), None);
        assert!(!ema.is_ready());

        // 1. First value is SMA(2, 5, 8) = 5
        let v = ema.push(8.0).unwrap();
        assert!((v - 5.0).abs() < f64::EPSILON);
        assert!(ema.is_ready());

        // 2. Multiplier = 2 / (3 + 1) = 0.5
        // EMA = (11.0 - 5.0) * 0.5 + 5.0 = 3.0 + 5.0 = 8.0
        let v = ema.push(11.0).unwrap();
        assert!((v - 8.0).abs() < f64::EPSILON);

        // 3. EMA = (14.0 - 8.0) * 0.5 + 8.0 = 3.0 + 8.0 = 11.0
        let v = ema.push(14.0).unwrap();
        assert!((v - 11.0).abs() < f64::EPSILON);
    }

    #[test]
    fn single_period() {
        let mut ema = Ema::new(1);
        // SMA(1) is just the value. Multiplier is 2 / (1+1) = 1
        // EMA_1 = 42.0
        let v = ema.push(42.0).unwrap();
        assert!((v - 42.0).abs() < f64::EPSILON);

        // EMA_2 = (99.0 - 42.0) * 1 + 42.0 = 99.0
        let v = ema.push(99.0).unwrap();
        assert!((v - 99.0).abs() < f64::EPSILON);
    }

    #[test]
    fn value_without_push() {
        let mut ema = Ema::new(2);
        assert_eq!(ema.value(), None);
        ema.push(10.0);
        assert_eq!(ema.value(), None);
        ema.push(20.0); // SMA(10, 20) = 15
        assert!((ema.value().unwrap() - 15.0).abs() < f64::EPSILON);
        ema.push(30.0); // EMA = (30-15)*(2/3)+15 = 10+15=25
        assert!((ema.value().unwrap() - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    #[should_panic(expected = "EMA period must be > 0")]
    fn zero_period_panics() {
        Ema::new(0);
    }
}
