use crate::indicator::ema::Ema;
use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct AtrExpansionStrategy {
    atr_ema: Ema,
    threshold_mult: f64,
    prev_price: Option<f64>,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl AtrExpansionStrategy {
    pub fn new(period: usize, threshold_x100: usize, min_ticks_between_signals: u64) -> Self {
        let threshold_mult = (threshold_x100.clamp(110, 500) as f64) / 100.0;
        Self {
            atr_ema: Ema::new(period.max(2)),
            threshold_mult,
            prev_price: None,
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
        let atr = self.atr_ema.push(delta.abs());
        let Some(atr) = atr else {
            return Signal::Hold;
        };
        let threshold = atr * self.threshold_mult;
        let cooldown_ok =
            self.tick_count.saturating_sub(self.last_signal_tick) >= self.min_ticks_between_signals;
        if delta > threshold && self.position == PositionState::Flat && cooldown_ok {
            self.position = PositionState::Long;
            self.last_signal_tick = self.tick_count;
            Signal::Buy
        } else if delta < -threshold && self.position == PositionState::Long && cooldown_ok {
            self.position = PositionState::Flat;
            self.last_signal_tick = self.tick_count;
            Signal::Sell
        } else {
            Signal::Hold
        }
    }
}
