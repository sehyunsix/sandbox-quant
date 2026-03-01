use crate::indicator::sma::Sma;
use crate::model::signal::Signal;
use crate::model::tick::Tick;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionState {
    Flat,
    Long,
}

#[derive(Debug)]
pub struct MaReversionStrategy {
    sma: Sma,
    entry_threshold: f64,
    position: PositionState,
    min_ticks_between_signals: u64,
    last_signal_tick: u64,
    tick_count: u64,
}

impl MaReversionStrategy {
    pub fn new(period: usize, threshold_bps: usize, min_ticks_between_signals: u64) -> Self {
        Self {
            sma: Sma::new(period.max(2)),
            entry_threshold: (threshold_bps.clamp(10, 3000) as f64) / 10_000.0,
            position: PositionState::Flat,
            min_ticks_between_signals: min_ticks_between_signals.max(1),
            last_signal_tick: 0,
            tick_count: 0,
        }
    }

    pub fn on_tick(&mut self, tick: &Tick) -> Signal {
        self.tick_count += 1;
        let Some(mean) = self.sma.push(tick.price) else {
            return Signal::Hold;
        };

        let cooldown_ok =
            self.tick_count.saturating_sub(self.last_signal_tick) >= self.min_ticks_between_signals;
        let buy_line = mean * (1.0 - self.entry_threshold);

        if tick.price <= buy_line && self.position == PositionState::Flat && cooldown_ok {
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
        self.sma.value()
    }
}
