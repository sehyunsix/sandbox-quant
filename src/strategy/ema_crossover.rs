use crate::indicator::ema::Ema;
use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct EmaCrossover {
    fast_ema: Ema,
    slow_ema: Ema,
    prev_fast: Option<f64>,
    prev_slow: Option<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl EmaCrossover {
    pub fn new(fast_period: usize, slow_period: usize, min_ticks: u64) -> Self {
        assert!(
            fast_period < slow_period,
            "fast_period must be less than slow_period"
        );
        Self {
            fast_ema: Ema::new(fast_period),
            slow_ema: Ema::new(slow_period),
            prev_fast: None,
            prev_slow: None,
            position: PositionState::Flat,
            min_ticks_between_signals: min_ticks,
            last_signal_tick: 0,
            tick_count: 0,
        }
    }

    pub fn on_tick(&mut self, tick: &Tick) -> Signal {
        self.tick_count += 1;
        let fast = self.fast_ema.push(tick.price);
        let slow = self.slow_ema.push(tick.price);

        let signal = match (fast, slow, self.prev_fast, self.prev_slow) {
            (Some(f), Some(s), Some(pf), Some(ps)) => {
                let cooldown_ok =
                    self.tick_count - self.last_signal_tick >= self.min_ticks_between_signals;
                if pf <= ps && f > s && self.position == PositionState::Flat && cooldown_ok {
                    self.position = PositionState::Long;
                    self.last_signal_tick = self.tick_count;
                    Signal::Buy
                } else if pf >= ps && f < s && self.position == PositionState::Long && cooldown_ok {
                    self.position = PositionState::Flat;
                    self.last_signal_tick = self.tick_count;
                    Signal::Sell
                } else {
                    Signal::Hold
                }
            }
            _ => Signal::Hold,
        };

        self.prev_fast = fast;
        self.prev_slow = slow;
        signal
    }

    pub fn fast_ema_value(&self) -> Option<f64> {
        self.fast_ema.value()
    }

    pub fn slow_ema_value(&self) -> Option<f64> {
        self.slow_ema.value()
    }
}
