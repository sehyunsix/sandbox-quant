use crate::indicator::ema::Ema;
use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct MacdCrossoverStrategy {
    fast_ema: Ema,
    slow_ema: Ema,
    signal_ema: Ema,
    prev_macd: Option<f64>,
    prev_signal: Option<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl MacdCrossoverStrategy {
    pub fn new(fast_period: usize, slow_period: usize, min_ticks_between_signals: u64) -> Self {
        let fast = fast_period.max(2);
        let slow = slow_period.max(fast + 1);
        let signal_period = (slow / 2).clamp(2, 9);
        Self {
            fast_ema: Ema::new(fast),
            slow_ema: Ema::new(slow),
            signal_ema: Ema::new(signal_period),
            prev_macd: None,
            prev_signal: None,
            position: PositionState::Flat,
            min_ticks_between_signals: min_ticks_between_signals.max(1),
            last_signal_tick: 0,
            tick_count: 0,
        }
    }

    pub fn on_tick(&mut self, tick: &Tick) -> Signal {
        self.tick_count += 1;
        let Some(fast) = self.fast_ema.push(tick.price) else {
            return Signal::Hold;
        };
        let Some(slow) = self.slow_ema.push(tick.price) else {
            return Signal::Hold;
        };

        let macd = fast - slow;
        let Some(signal) = self.signal_ema.push(macd) else {
            self.prev_macd = Some(macd);
            return Signal::Hold;
        };

        let cooldown_ok = self.tick_count.saturating_sub(self.last_signal_tick)
            >= self.min_ticks_between_signals;

        let out = match (self.prev_macd, self.prev_signal) {
            (Some(pm), Some(ps)) if pm <= ps && macd > signal => {
                if self.position == PositionState::Flat && cooldown_ok {
                    self.position = PositionState::Long;
                    self.last_signal_tick = self.tick_count;
                    Signal::Buy
                } else {
                    Signal::Hold
                }
            }
            (Some(pm), Some(ps)) if pm >= ps && macd < signal => {
                if self.position == PositionState::Long && cooldown_ok {
                    self.position = PositionState::Flat;
                    self.last_signal_tick = self.tick_count;
                    Signal::Sell
                } else {
                    Signal::Hold
                }
            }
            _ if macd > signal && self.position == PositionState::Flat && cooldown_ok => {
                self.position = PositionState::Long;
                self.last_signal_tick = self.tick_count;
                Signal::Buy
            }
            _ if macd < signal && self.position == PositionState::Long && cooldown_ok => {
                self.position = PositionState::Flat;
                self.last_signal_tick = self.tick_count;
                Signal::Sell
            }
            _ => Signal::Hold,
        };

        self.prev_macd = Some(macd);
        self.prev_signal = Some(signal);
        out
    }
}
