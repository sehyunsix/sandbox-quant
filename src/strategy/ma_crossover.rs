use crate::indicator::sma::Sma;
use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct MaCrossover {
    fast_sma: Sma,
    slow_sma: Sma,
    prev_fast: Option<f64>,
    prev_slow: Option<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl MaCrossover {
    pub fn new(fast_period: usize, slow_period: usize, min_ticks: u64) -> Self {
        assert!(
            fast_period < slow_period,
            "fast_period must be less than slow_period"
        );
        Self {
            fast_sma: Sma::new(fast_period),
            slow_sma: Sma::new(slow_period),
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
        let fast = self.fast_sma.push(tick.price);
        let slow = self.slow_sma.push(tick.price);

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

    pub fn fast_sma_value(&self) -> Option<f64> {
        self.fast_sma.value()
    }

    pub fn slow_sma_value(&self) -> Option<f64> {
        self.slow_sma.value()
    }

    pub fn is_long(&self) -> bool {
        self.position == PositionState::Long
    }
}
