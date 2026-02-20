use crate::indicator::sma::Sma;
use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct EnsembleVoteStrategy {
    fast_sma: Sma,
    slow_sma: Sma,
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

impl EnsembleVoteStrategy {
    pub fn new(fast_period: usize, slow_period: usize, min_ticks_between_signals: u64) -> Self {
        let fast = fast_period.max(2);
        let slow = slow_period.max(fast + 1);
        Self {
            fast_sma: Sma::new(fast),
            slow_sma: Sma::new(slow),
            period: fast.max(2),
            lower: 30.0,
            upper: 70.0,
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
        let prev_fast = self.fast_sma.value();
        let prev_slow = self.slow_sma.value();
        let fast = self.fast_sma.push(tick.price);
        let slow = self.slow_sma.push(tick.price);
        self.update_rsi(tick.price);

        let cooldown_ok = self.tick_count.saturating_sub(self.last_signal_tick)
            >= self.min_ticks_between_signals;
        let score = self.vote_score(fast, slow, prev_fast, prev_slow, tick.price);
        let rsi = self.rsi_value();

        let buy_ok = score >= 2 || (score >= 1 && rsi.map(|v| v <= 45.0).unwrap_or(false));
        let sell_ok = score <= -2 || (score <= -1 && rsi.map(|v| v >= 55.0).unwrap_or(false));

        if buy_ok && self.position == PositionState::Flat && cooldown_ok {
            self.position = PositionState::Long;
            self.last_signal_tick = self.tick_count;
            Signal::Buy
        } else if sell_ok && self.position == PositionState::Long && cooldown_ok {
            self.position = PositionState::Flat;
            self.last_signal_tick = self.tick_count;
            Signal::Sell
        } else {
            Signal::Hold
        }
    }

    fn vote_score(
        &self,
        fast: Option<f64>,
        slow: Option<f64>,
        prev_fast: Option<f64>,
        prev_slow: Option<f64>,
        price: f64,
    ) -> i32 {
        let mut score = 0;

        // Voter 1: MA crossover direction
        if let (Some(f), Some(s), Some(pf), Some(ps)) = (fast, slow, prev_fast, prev_slow) {
            if pf <= ps && f > s {
                score += 1;
            } else if pf >= ps && f < s {
                score -= 1;
            }
        }

        // Voter 2: RSI overbought/oversold
        if let Some(rsi) = self.rsi_value() {
            if rsi <= self.lower {
                score += 1;
            } else if rsi >= self.upper {
                score -= 1;
            }
        }

        // Voter 3: price vs fast trend proxy
        if let Some(fast_mean) = fast {
            if price > fast_mean {
                score += 1;
            } else if price < fast_mean {
                score -= 1;
            }
        }

        score
    }

    fn update_rsi(&mut self, price: f64) {
        let Some(prev) = self.prev_price.replace(price) else {
            return;
        };

        let delta = price - prev;
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
        } else {
            let prev_avg_gain = self.avg_gain.unwrap_or(0.0);
            let prev_avg_loss = self.avg_loss.unwrap_or(0.0);
            let period = self.period as f64;
            self.avg_gain = Some(((prev_avg_gain * (period - 1.0)) + gain) / period);
            self.avg_loss = Some(((prev_avg_loss * (period - 1.0)) + loss) / period);
        }

    }

    fn rsi_value(&self) -> Option<f64> {
        let avg_gain = self.avg_gain?;
        let avg_loss = self.avg_loss?;
        if avg_loss <= f64::EPSILON {
            return Some(100.0);
        }
        let rs = avg_gain / avg_loss;
        Some(100.0 - (100.0 / (1.0 + rs)))
    }
}
