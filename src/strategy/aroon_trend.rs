use std::collections::VecDeque;

use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct AroonTrendStrategy {
    period: usize,
    threshold: f64,
    prices: VecDeque<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl AroonTrendStrategy {
    pub fn new(period: usize, threshold: usize, min_ticks_between_signals: u64) -> Self {
        Self {
            period: period.max(5),
            threshold: threshold.clamp(50, 90) as f64,
            prices: VecDeque::new(),
            position: PositionState::Flat,
            min_ticks_between_signals: min_ticks_between_signals.max(1),
            last_signal_tick: 0,
            tick_count: 0,
        }
    }

    pub fn on_tick(&mut self, tick: &Tick) -> Signal {
        self.tick_count += 1;
        self.prices.push_back(tick.price);
        while self.prices.len() > self.period {
            let _ = self.prices.pop_front();
        }
        if self.prices.len() < self.period {
            return Signal::Hold;
        }

        let mut high_idx = 0usize;
        let mut low_idx = 0usize;
        let mut high = f64::MIN;
        let mut low = f64::MAX;

        for (i, p) in self.prices.iter().enumerate() {
            if *p >= high {
                high = *p;
                high_idx = i;
            }
            if *p <= low {
                low = *p;
                low_idx = i;
            }
        }

        let lookback = (self.period - 1) as f64;
        let periods_since_high = (self.period - 1 - high_idx) as f64;
        let periods_since_low = (self.period - 1 - low_idx) as f64;
        let aroon_up = 100.0 * (lookback - periods_since_high) / lookback.max(1.0);
        let aroon_down = 100.0 * (lookback - periods_since_low) / lookback.max(1.0);

        let cooldown_ok = self.tick_count.saturating_sub(self.last_signal_tick)
            >= self.min_ticks_between_signals;

        if aroon_up >= self.threshold
            && aroon_down <= 100.0 - self.threshold
            && self.position == PositionState::Flat
            && cooldown_ok
        {
            self.position = PositionState::Long;
            self.last_signal_tick = self.tick_count;
            Signal::Buy
        } else if aroon_down >= self.threshold
            && aroon_up <= 100.0 - self.threshold
            && self.position == PositionState::Long
            && cooldown_ok
        {
            self.position = PositionState::Flat;
            self.last_signal_tick = self.tick_count;
            Signal::Sell
        } else {
            Signal::Hold
        }
    }
}
