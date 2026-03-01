use std::collections::VecDeque;

use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct BollingerReversionStrategy {
    period: usize,
    band_mult: f64,
    prices: VecDeque<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
    mean: Option<f64>,
}

impl BollingerReversionStrategy {
    pub fn new(period: usize, band_mult_x100: usize, min_ticks_between_signals: u64) -> Self {
        Self {
            period: period.max(2),
            band_mult: (band_mult_x100.clamp(50, 400) as f64) / 100.0,
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

        let lower = mean - self.band_mult * std_dev;
        let cooldown_ok =
            self.tick_count.saturating_sub(self.last_signal_tick) >= self.min_ticks_between_signals;

        if tick.price <= lower && self.position == PositionState::Flat && cooldown_ok {
            self.position = PositionState::Long;
            self.last_signal_tick = self.tick_count;
            Signal::Buy
        } else if tick.price >= mean && self.position == PositionState::Long && cooldown_ok {
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
