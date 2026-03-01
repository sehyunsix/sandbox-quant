use std::collections::VecDeque;

use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct ChannelBreakoutStrategy {
    entry_window: usize,
    exit_window: usize,
    prices: VecDeque<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl ChannelBreakoutStrategy {
    pub fn new(entry_window: usize, exit_window: usize, min_ticks_between_signals: u64) -> Self {
        Self {
            entry_window: entry_window.max(2),
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
        let max_window = self.entry_window.max(self.exit_window);
        let signal = if self.prices.len() >= max_window {
            let entry_high = self
                .prices
                .iter()
                .rev()
                .take(self.entry_window)
                .fold(f64::MIN, |acc, p| acc.max(*p));
            let exit_low = self
                .prices
                .iter()
                .rev()
                .take(self.exit_window)
                .fold(f64::MAX, |acc, p| acc.min(*p));
            let cooldown_ok = self.tick_count.saturating_sub(self.last_signal_tick)
                >= self.min_ticks_between_signals;
            if tick.price > entry_high && self.position == PositionState::Flat && cooldown_ok {
                self.position = PositionState::Long;
                self.last_signal_tick = self.tick_count;
                Signal::Buy
            } else if tick.price < exit_low && self.position == PositionState::Long && cooldown_ok {
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
        while self.prices.len() > max_window {
            let _ = self.prices.pop_front();
        }
        signal
    }
}
