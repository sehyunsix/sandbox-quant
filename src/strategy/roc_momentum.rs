use std::collections::VecDeque;

use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct RocMomentumStrategy {
    lookback: usize,
    threshold: f64,
    prices: VecDeque<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl RocMomentumStrategy {
    pub fn new(lookback: usize, threshold_bps: usize, min_ticks_between_signals: u64) -> Self {
        Self {
            lookback: lookback.max(2),
            threshold: (threshold_bps.clamp(5, 1_000) as f64) / 10_000.0,
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
        while self.prices.len() > self.lookback + 1 {
            let _ = self.prices.pop_front();
        }
        if self.prices.len() < self.lookback + 1 {
            return Signal::Hold;
        }

        let base = self.prices.front().copied().unwrap_or(tick.price);
        let roc = (tick.price - base) / base.abs().max(f64::EPSILON);
        let cooldown_ok =
            self.tick_count.saturating_sub(self.last_signal_tick) >= self.min_ticks_between_signals;

        if roc >= self.threshold && self.position == PositionState::Flat && cooldown_ok {
            self.position = PositionState::Long;
            self.last_signal_tick = self.tick_count;
            Signal::Buy
        } else if roc <= -self.threshold && self.position == PositionState::Long && cooldown_ok {
            self.position = PositionState::Flat;
            self.last_signal_tick = self.tick_count;
            Signal::Sell
        } else {
            Signal::Hold
        }
    }
}
