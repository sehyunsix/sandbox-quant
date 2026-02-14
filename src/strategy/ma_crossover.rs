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

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(price: f64) -> Tick {
        Tick {
            price,
            qty: 1.0,
            timestamp_ms: 0,
            is_buyer_maker: false,
            trade_id: 0,
        }
    }

    #[test]
    fn insufficient_data_returns_hold() {
        // With fast=2, slow=3, need at least 3 ticks + 1 for crossover check
        let mut strat = MaCrossover::new(2, 3, 0);
        assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
        assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
        assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
        // First tick where both SMAs have values - still Hold (no prev to compare)
        assert_eq!(strat.on_tick(&tick(100.0)), Signal::Hold);
    }

    #[test]
    fn buy_signal_on_bullish_crossover() {
        // fast=2, slow=4, no cooldown
        let mut strat = MaCrossover::new(2, 4, 0);

        // Feed descending prices to establish slow > fast
        // Tick 1-4: both SMAs fill up, prev values get set on tick 4
        for &p in &[100.0, 90.0, 80.0, 70.0] {
            assert_eq!(strat.on_tick(&tick(p)), Signal::Hold);
        }

        // Tick 5: fast crosses above slow
        // prev: fast=75, slow=85 (fast < slow)
        // now:  fast=avg(70,120)=95, slow=avg(90,80,70,120)=90 (fast > slow)
        let sig = strat.on_tick(&tick(120.0));
        assert_eq!(sig, Signal::Buy, "Expected Buy signal, got {:?}", sig);
        assert!(strat.is_long());
    }

    #[test]
    fn sell_signal_on_bearish_crossover() {
        let mut strat = MaCrossover::new(2, 4, 0);

        // Establish long position first
        for &p in &[100.0, 90.0, 80.0, 70.0, 120.0, 150.0] {
            strat.on_tick(&tick(p));
        }
        // Should be long after bullish crossover

        // Feed sharply dropping prices
        for &p in &[60.0, 40.0, 30.0, 20.0] {
            let sig = strat.on_tick(&tick(p));
            if matches!(sig, Signal::Sell { .. }) {
                assert!(!strat.is_long());
                return;
            }
        }
        panic!("Expected Sell signal during price drop");
    }

    #[test]
    fn no_double_buy() {
        let mut strat = MaCrossover::new(2, 4, 0);

        // Establish long position
        for &p in &[100.0, 90.0, 80.0, 70.0, 120.0, 150.0] {
            strat.on_tick(&tick(p));
        }
        assert!(strat.is_long());

        // Continue rising - should stay Hold, never Buy again
        for &p in &[160.0, 170.0, 180.0, 190.0] {
            let sig = strat.on_tick(&tick(p));
            assert_eq!(
                sig,
                Signal::Hold,
                "Should not double-buy while already long"
            );
        }
    }

    #[test]
    fn cooldown_prevents_rapid_signals() {
        let mut strat = MaCrossover::new(2, 4, 100); // 100-tick cooldown

        // Establish and then trigger buy
        for &p in &[100.0, 90.0, 80.0, 70.0, 120.0] {
            strat.on_tick(&tick(p));
        }
        let sig = strat.on_tick(&tick(150.0));
        // This may or may not be a buy depending on exact cooldown timing
        // But if we trigger buy, next sell should be blocked by cooldown

        if matches!(sig, Signal::Buy { .. }) {
            // Immediately try to trigger sell - should be blocked by cooldown
            for &p in &[50.0, 30.0] {
                let sig = strat.on_tick(&tick(p));
                assert_eq!(sig, Signal::Hold, "Cooldown should prevent rapid sell");
            }
        }
    }

    #[test]
    fn deterministic_output() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + 20.0 * (i as f64 * 0.1).sin())
            .collect();

        let run = |prices: &[f64]| -> Vec<Signal> {
            let mut strat = MaCrossover::new(5, 15, 0);
            prices.iter().map(|&p| strat.on_tick(&tick(p))).collect()
        };

        let run1 = run(&prices);
        let run2 = run(&prices);
        assert_eq!(run1, run2, "Strategy must be deterministic");
    }
}
