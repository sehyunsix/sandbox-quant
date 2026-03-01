use std::collections::VecDeque;

use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct StochasticReversionStrategy {
    lookback: usize,
    lower: f64,
    upper: f64,
    prices: VecDeque<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl StochasticReversionStrategy {
    pub fn new(lookback: usize, upper_threshold: usize, min_ticks_between_signals: u64) -> Self {
        let upper = upper_threshold.clamp(51, 95) as f64;
        let lower = 100.0 - upper;
        Self {
            lookback: lookback.max(2),
            lower,
            upper,
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
        while self.prices.len() > self.lookback {
            let _ = self.prices.pop_front();
        }
        if self.prices.len() < self.lookback {
            return Signal::Hold;
        }

        let low = self.prices.iter().fold(f64::MAX, |acc, p| acc.min(*p));
        let high = self.prices.iter().fold(f64::MIN, |acc, p| acc.max(*p));
        let range = (high - low).max(f64::EPSILON);
        let k = ((tick.price - low) / range) * 100.0;
        let cooldown_ok =
            self.tick_count.saturating_sub(self.last_signal_tick) >= self.min_ticks_between_signals;

        if k <= self.lower && self.position == PositionState::Flat && cooldown_ok {
            self.position = PositionState::Long;
            self.last_signal_tick = self.tick_count;
            Signal::Buy
        } else if k >= self.upper && self.position == PositionState::Long && cooldown_ok {
            self.position = PositionState::Flat;
            self.last_signal_tick = self.tick_count;
            Signal::Sell
        } else {
            Signal::Hold
        }
    }
}
