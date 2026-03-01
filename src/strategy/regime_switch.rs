use std::collections::VecDeque;

use crate::indicator::sma::Sma;
use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct RegimeSwitchStrategy {
    fast_sma: Sma,
    slow_sma: Sma,
    prev_fast: Option<f64>,
    prev_slow: Option<f64>,
    prices: VecDeque<f64>,
    slow_period: usize,
    volatility_threshold: f64,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl RegimeSwitchStrategy {
    pub fn new(fast_period: usize, slow_period: usize, min_ticks_between_signals: u64) -> Self {
        let fast = fast_period.max(2);
        let slow = slow_period.max(fast + 1);
        Self {
            fast_sma: Sma::new(fast),
            slow_sma: Sma::new(slow),
            prev_fast: None,
            prev_slow: None,
            prices: VecDeque::new(),
            slow_period: slow,
            volatility_threshold: 0.006,
            position: PositionState::Flat,
            min_ticks_between_signals: min_ticks_between_signals.max(1),
            last_signal_tick: 0,
            tick_count: 0,
        }
    }

    pub fn on_tick(&mut self, tick: &Tick) -> Signal {
        self.tick_count += 1;

        let fast = self.fast_sma.push(tick.price);
        let slow = self.slow_sma.push(tick.price);

        self.prices.push_back(tick.price);
        while self.prices.len() > self.slow_period {
            let _ = self.prices.pop_front();
        }

        let cooldown_ok =
            self.tick_count.saturating_sub(self.last_signal_tick) >= self.min_ticks_between_signals;

        let signal = match (fast, slow, self.prev_fast, self.prev_slow) {
            (Some(f), Some(s), Some(pf), Some(ps)) if self.prices.len() >= self.slow_period => {
                let n = self.prices.len() as f64;
                let mean = self.prices.iter().sum::<f64>() / n;
                let variance = self
                    .prices
                    .iter()
                    .map(|p| {
                        let d = *p - mean;
                        d * d
                    })
                    .sum::<f64>()
                    / n;
                let std_dev = variance.sqrt();
                let vol_ratio = std_dev / mean.abs().max(f64::EPSILON);

                if vol_ratio >= self.volatility_threshold {
                    // Trend regime: MA cross in/out
                    if pf <= ps && f > s && self.position == PositionState::Flat && cooldown_ok {
                        self.position = PositionState::Long;
                        self.last_signal_tick = self.tick_count;
                        Signal::Buy
                    } else if pf >= ps
                        && f < s
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
                    // Mean-reversion regime: buy at -1σ, sell at +1σ
                    let lower = mean - std_dev;
                    let upper = mean + std_dev;
                    if tick.price <= lower && self.position == PositionState::Flat && cooldown_ok {
                        self.position = PositionState::Long;
                        self.last_signal_tick = self.tick_count;
                        Signal::Buy
                    } else if tick.price >= upper
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
            _ => Signal::Hold,
        };

        self.prev_fast = fast;
        self.prev_slow = slow;
        signal
    }
}
