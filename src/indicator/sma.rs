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

