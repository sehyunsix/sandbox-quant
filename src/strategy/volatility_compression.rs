use std::collections::VecDeque;

use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct VolatilityCompressionStrategy {
    period: usize,
    compression_threshold: f64,
    prices: VecDeque<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
    mean: Option<f64>,
}

impl VolatilityCompressionStrategy {
    pub fn new(period: usize, threshold_bps: usize, min_ticks_between_signals: u64) -> Self {
        Self {
            period: period.max(2),
            compression_threshold: (threshold_bps.clamp(10, 5000) as f64) / 10_000.0,
            prices: VecDeque::new(),
            position: PositionState::Flat,
            min_ticks_between_signals: min_ticks_between_signals.max(1),
            last_signal_tick: 0,
            tick_count: 0,
            mean: None,
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

        let n = self.prices.len() as f64;
        let mean = self.prices.iter().sum::<f64>() / n;
        self.mean = Some(mean);
        let variance = self
            .prices
            .iter()
            .map(|p| {
                let d = *p - mean;
                d * d
            })
            .sum::<f64>()
            / n.max(1.0);
        let std_dev = variance.sqrt();
        let bandwidth = (2.0 * std_dev) / mean.abs().max(f64::EPSILON);
        let upper_trigger = mean + std_dev;
        let cooldown_ok =
            self.tick_count.saturating_sub(self.last_signal_tick) >= self.min_ticks_between_signals;

        if self.position == PositionState::Flat
            && bandwidth <= self.compression_threshold
            && tick.price > upper_trigger
            && cooldown_ok
        {
            self.position = PositionState::Long;
            self.last_signal_tick = self.tick_count;
            Signal::Buy
        } else if self.position == PositionState::Long && tick.price < mean && cooldown_ok {
            self.position = PositionState::Flat;
            self.last_signal_tick = self.tick_count;
            Signal::Sell
        } else {
            Signal::Hold
        }
    }

    pub fn mean_value(&self) -> Option<f64> {
        self.mean
    }
}
