/// Simple Moving Average using a ring buffer for O(1) push.
#[derive(Debug, Clone)]
pub struct Sma {
    period: usize,
    buffer: Vec<f64>,
    head: usize,
    count: usize,
    sum: f64,
}

impl Sma {
    pub fn new(period: usize) -> Self {
        assert!(period > 0, "SMA period must be > 0");
        Self {
            period,
            buffer: vec![0.0; period],
            head: 0,
            count: 0,
            sum: 0.0,
        }
    }

    /// Push a new value, return the current SMA if enough data.
    pub fn push(&mut self, value: f64) -> Option<f64> {
        if self.count >= self.period {
            self.sum -= self.buffer[self.head];
        }
        self.buffer[self.head] = value;
        self.sum += value;
        self.head = (self.head + 1) % self.period;
        if self.count < self.period {
            self.count += 1;
        }

        if self.count >= self.period {
            Some(self.sum / self.period as f64)
        } else {
            None
        }
    }

    pub fn value(&self) -> Option<f64> {
        if self.count >= self.period {
            Some(self.sum / self.period as f64)
        } else {
            None
        }
    }

    pub fn is_ready(&self) -> bool {
        self.count >= self.period
    }

    pub fn period(&self) -> usize {
        self.period
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_sma() {
        let mut sma = Sma::new(3);
        assert_eq!(sma.push(1.0), None);
        assert_eq!(sma.push(2.0), None);
        assert!(!sma.is_ready());

        let v = sma.push(3.0).unwrap();
        assert!((v - 2.0).abs() < f64::EPSILON);

        let v = sma.push(4.0).unwrap();
        assert!((v - 3.0).abs() < f64::EPSILON);

        let v = sma.push(5.0).unwrap();
        assert!((v - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn single_period() {
        let mut sma = Sma::new(1);
        assert!((sma.push(42.0).unwrap() - 42.0).abs() < f64::EPSILON);
        assert!((sma.push(99.0).unwrap() - 99.0).abs() < f64::EPSILON);
    }

    #[test]
    fn ring_buffer_wraps_correctly() {
        let mut sma = Sma::new(3);
        // Fill the buffer
        sma.push(10.0);
        sma.push(20.0);
        sma.push(30.0); // avg = 20
                        // Wrap around
        let v = sma.push(40.0).unwrap(); // [40, 20, 30] -> avg = 30
        assert!((v - 30.0).abs() < f64::EPSILON);

        let v = sma.push(50.0).unwrap(); // [40, 50, 30] -> avg = 40
        assert!((v - 40.0).abs() < f64::EPSILON);

        let v = sma.push(60.0).unwrap(); // [40, 50, 60] -> avg = 50
        assert!((v - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn value_without_push() {
        let mut sma = Sma::new(2);
        assert_eq!(sma.value(), None);
        sma.push(10.0);
        assert_eq!(sma.value(), None);
        sma.push(20.0);
        assert!((sma.value().unwrap() - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn no_drift_after_many_pushes() {
        let mut sma = Sma::new(10);
        let mut naive_buf: Vec<f64> = Vec::new();

        for i in 0..10_000u64 {
            let val = (i as f64) * 0.1 + 0.01;
            sma.push(val);
            naive_buf.push(val);
            if naive_buf.len() > 10 {
                naive_buf.remove(0);
            }

            if let Some(ring_avg) = sma.value() {
                let naive_avg: f64 = naive_buf.iter().sum::<f64>() / naive_buf.len() as f64;
                assert!(
                    (ring_avg - naive_avg).abs() < 1e-8,
                    "Drift at i={}: ring={} naive={}",
                    i,
                    ring_avg,
                    naive_avg
                );
            }
        }
    }

    #[test]
    #[should_panic(expected = "SMA period must be > 0")]
    fn zero_period_panics() {
        Sma::new(0);
    }
}
