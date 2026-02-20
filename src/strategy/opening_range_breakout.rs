use std::collections::VecDeque;

use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct OpeningRangeBreakoutStrategy {
    opening_window: usize,
    exit_window: usize,
    prices: VecDeque<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl OpeningRangeBreakoutStrategy {
    pub fn new(opening_window: usize, exit_window: usize, min_ticks_between_signals: u64) -> Self {
        Self {
            opening_window: opening_window.max(2),
            exit_window: exit_window.max(2),
            prices: VecDeque::new(),
            position: PositionState::Flat,
            min_ticks_between_signals: min_ticks_between_signals.max(1),
            last_signal_tick: 0,
            tick_count: 0,
        }
    }

    pub fn on_tick(&mut self, tick: &Tick) -> Signal {
        self.tick_count += 1;
        let required = self.opening_window + self.exit_window;
        let signal = if self.prices.len() >= required {
            let opening_high = self
                .prices
                .iter()
                .take(self.opening_window)
                .fold(f64::MIN, |acc, p| acc.max(*p));
            let trailing_low = self
                .prices
                .iter()
                .rev()
                .take(self.exit_window)
                .fold(f64::MAX, |acc, p| acc.min(*p));
            let cooldown_ok = self.tick_count.saturating_sub(self.last_signal_tick)
                >= self.min_ticks_between_signals;

            if tick.price > opening_high && self.position == PositionState::Flat && cooldown_ok {
                self.position = PositionState::Long;
                self.last_signal_tick = self.tick_count;
                Signal::Buy
            } else if tick.price < trailing_low
                && self.position == PositionState::Long
                && cooldown_ok
            {
                self.position = PositionState::Flat;
                self.last_signal_tick = self.tick_count;
                Signal::Sell
            } else {
                Signal::Hold
            }
        } else {
            Signal::Hold
        };

        self.prices.push_back(tick.price);
        while self.prices.len() > required {
            let _ = self.prices.pop_front();
        }
        signal
    }
}
