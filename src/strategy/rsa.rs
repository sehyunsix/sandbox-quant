use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct RsaStrategy {
    period: usize,
    lower: f64,
    upper: f64,
    prev_price: Option<f64>,
    warmup_count: usize,
    gain_sum: f64,
    loss_sum: f64,
    avg_gain: Option<f64>,
    avg_loss: Option<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl RsaStrategy {
    pub fn new(period: usize, lower: f64, upper: f64, min_ticks_between_signals: u64) -> Self {
        let period = period.max(2);
        let mut lower = lower.clamp(1.0, 99.0);
        let mut upper = upper.clamp(1.0, 99.0);
        if lower >= upper {
            lower = 30.0;
            upper = 70.0;
        }
        Self {
            period,
            lower,
            upper,
            prev_price: None,
            warmup_count: 0,
            gain_sum: 0.0,
            loss_sum: 0.0,
            avg_gain: None,
            avg_loss: None,
            position: PositionState::Flat,
            min_ticks_between_signals: min_ticks_between_signals.max(1),
            last_signal_tick: 0,
            tick_count: 0,
        }
    }

    pub fn on_tick(&mut self, tick: &Tick) -> Signal {
        self.tick_count += 1;
        let Some(prev) = self.prev_price.replace(tick.price) else {
            return Signal::Hold;
        };

        let delta = tick.price - prev;
        let gain = delta.max(0.0);
        let loss = (-delta).max(0.0);

        if self.avg_gain.is_none() || self.avg_loss.is_none() {
            self.gain_sum += gain;
            self.loss_sum += loss;
            self.warmup_count += 1;
            if self.warmup_count >= self.period {
                self.avg_gain = Some(self.gain_sum / self.period as f64);
                self.avg_loss = Some(self.loss_sum / self.period as f64);
            }
            return Signal::Hold;
        }

        let prev_avg_gain = self.avg_gain.unwrap_or(0.0);
        let prev_avg_loss = self.avg_loss.unwrap_or(0.0);
        let period = self.period as f64;
        let next_avg_gain = ((prev_avg_gain * (period - 1.0)) + gain) / period;
        let next_avg_loss = ((prev_avg_loss * (period - 1.0)) + loss) / period;
        self.avg_gain = Some(next_avg_gain);
        self.avg_loss = Some(next_avg_loss);

        let cooldown_ok =
            self.tick_count.saturating_sub(self.last_signal_tick) >= self.min_ticks_between_signals;
        let rsi = self.rsi_value().unwrap_or(50.0);
        if rsi <= self.lower && self.position == PositionState::Flat && cooldown_ok {
            self.position = PositionState::Long;
            self.last_signal_tick = self.tick_count;
            Signal::Buy
        } else if rsi >= self.upper && self.position == PositionState::Long && cooldown_ok {
            self.position = PositionState::Flat;
            self.last_signal_tick = self.tick_count;
            Signal::Sell
        } else {
            Signal::Hold
        }
    }

    pub fn rsi_value(&self) -> Option<f64> {
        let avg_gain = self.avg_gain?;
        let avg_loss = self.avg_loss?;
        if avg_loss <= f64::EPSILON {
            return Some(100.0);
        }
        let rs = avg_gain / avg_loss;
        Some(100.0 - (100.0 / (1.0 + rs)))
    }
}
