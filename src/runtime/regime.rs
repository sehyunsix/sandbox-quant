use crate::event::{MarketRegime, MarketRegimeSignal};
use crate::indicator::ema::Ema;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct RegimeDetectorConfig {
    pub fast_period: usize,
    pub slow_period: usize,
    pub vol_window: usize,
    pub range_vol_threshold: f64,
}

impl Default for RegimeDetectorConfig {
    fn default() -> Self {
        Self {
            fast_period: 10,
            slow_period: 30,
            vol_window: 20,
            range_vol_threshold: 0.0045,
        }
    }
}

#[derive(Debug)]
pub struct RegimeDetector {
    fast_ema: Ema,
    slow_ema: Ema,
    closes: VecDeque<f64>,
    returns: VecDeque<f64>,
    prev_fast: Option<f64>,
    config: RegimeDetectorConfig,
}

impl Default for RegimeDetector {
    fn default() -> Self {
        Self::new(RegimeDetectorConfig::default())
    }
}

impl RegimeDetector {
    pub fn new(config: RegimeDetectorConfig) -> Self {
        let fast = Ema::new(config.fast_period.max(2));
        let slow = Ema::new(config.slow_period.max(config.fast_period.max(2)).max(2));
        Self {
            fast_ema: fast,
            slow_ema: slow,
            closes: VecDeque::new(),
            returns: VecDeque::new(),
            prev_fast: None,
            config,
        }
    }

    pub fn update(&mut self, price: f64, now_ms: u64) -> MarketRegimeSignal {
        if !price.is_finite() || price <= f64::EPSILON {
            return MarketRegimeSignal {
                regime: MarketRegime::Unknown,
                confidence: 0.0,
                ema_fast: 0.0,
                ema_slow: 0.0,
                vol_ratio: 0.0,
                slope: 0.0,
                updated_at_ms: now_ms,
            };
        }

        let fast = self.fast_ema.push(price).unwrap_or(price);
        let slow = self.slow_ema.push(price).unwrap_or(price);
        self.closes.push_back(price);
        if self.closes.len() > self.config.slow_period {
            let _ = self.closes.pop_front();
        }

        if self.closes.len() >= 2 {
            if let Some(prev) = self.closes.get(self.closes.len() - 2).copied() {
                let ret = (price / prev - 1.0).abs();
                self.returns.push_back(ret);
                while self.returns.len() > self.config.vol_window {
                    let _ = self.returns.pop_front();
                }
            }
        }
        let slope = self
            .prev_fast
            .and_then(|prev| ((fast - prev) / prev.max(f64::EPSILON)).into())
            .unwrap_or(0.0);
        self.prev_fast = Some(fast);

        if !self.fast_ema.is_ready() || !self.slow_ema.is_ready() {
            return MarketRegimeSignal {
                regime: MarketRegime::Unknown,
                confidence: 0.0,
                ema_fast: fast,
                ema_slow: slow,
                vol_ratio: 0.0,
                slope,
                updated_at_ms: now_ms,
            };
        }

        let vol_ratio = if self.returns.is_empty() {
            0.0
        } else {
            let mean_abs = self.returns.iter().copied().sum::<f64>() / self.returns.len() as f64;
            let centered = self
                .returns
                .iter()
                .map(|v| {
                    let d = *v - mean_abs;
                    d * d
                })
                .sum::<f64>()
                / self.returns.len() as f64;
            let stdev = centered.sqrt();
            if mean_abs > 0.0 {
                stdev / mean_abs
            } else {
                0.0
            }
        };

        let regime = if vol_ratio <= self.config.range_vol_threshold {
            MarketRegime::Range
        } else if fast > slow && slope > 0.0 {
            MarketRegime::TrendUp
        } else if fast < slow && slope < 0.0 {
            MarketRegime::TrendDown
        } else {
            MarketRegime::Range
        };

        let trend_gap = (fast - slow).abs() / slow.abs().max(f64::EPSILON);
        let confidence = if matches!(regime, MarketRegime::Range) {
            (1.0 - (vol_ratio / self.config.range_vol_threshold.max(f64::EPSILON)))
                .max(0.0)
                .min(1.0)
        } else {
            (trend_gap * 50.0).max(slope.abs() * 1500.0).min(1.0)
        };

        MarketRegimeSignal {
            regime,
            confidence,
            ema_fast: fast,
            ema_slow: slow,
            vol_ratio,
            slope,
            updated_at_ms: now_ms,
        }
    }
}
