use crate::event::{MarketRegime, MarketRegimeSignal};
use crate::model::signal::Signal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaSideBias {
    Long,
    Flat,
}

#[derive(Debug, Clone)]
pub struct AlphaSignal {
    pub symbol: String,
    pub side_bias: AlphaSideBias,
    pub strength: f64,
    pub expected_return_usdt: f64,
    pub risk_estimate_usdt: f64,
    pub horizon_ms: u64,
    pub confidence: f64,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PortfolioDecision {
    pub alpha: AlphaSignal,
    pub target_position_ratio: f64,
    pub execution_signal: Signal,
    pub regime: MarketRegime,
    pub regime_confidence: f64,
    pub reason: &'static str,
}

#[derive(Debug, Clone)]
pub struct PortfolioExecutionIntent {
    pub symbol: String,
    pub source_tag: String,
    pub target_position_ratio: f64,
    pub position_delta_ratio: f64,
    pub desired_notional_usdt: f64,
    pub expected_return_usdt: f64,
    pub strength: f64,
    pub reason: &'static str,
    pub timestamp_ms: u64,
}

impl PortfolioDecision {
    pub fn to_intent(
        &self,
        source_tag: &str,
        order_amount_usdt: f64,
        current_position_ratio: f64,
    ) -> PortfolioExecutionIntent {
        PortfolioExecutionIntent {
            symbol: self.alpha.symbol.clone(),
            source_tag: source_tag.to_string(),
            target_position_ratio: self.target_position_ratio,
            position_delta_ratio: self.target_position_ratio - current_position_ratio,
            desired_notional_usdt: order_amount_usdt.max(0.0) * self.target_position_ratio.max(0.0),
            expected_return_usdt: self.alpha.expected_return_usdt,
            strength: self.alpha.strength,
            reason: self.reason,
            timestamp_ms: self.alpha.timestamp_ms,
        }
    }
}

impl PortfolioExecutionIntent {
    pub fn effective_signal(&self, min_delta_ratio: f64) -> Signal {
        let threshold = min_delta_ratio.max(0.0);
        if self.position_delta_ratio > threshold {
            Signal::Buy
        } else if self.position_delta_ratio < -threshold {
            Signal::Sell
        } else {
            Signal::Hold
        }
    }
}

pub fn decide_portfolio_action_from_alpha(
    symbol: &str,
    now_ms: u64,
    is_flat: bool,
    alpha_mu: f64,
    order_amount_usdt: f64,
    regime: MarketRegimeSignal,
) -> PortfolioDecision {
    let strength = alpha_mu.abs().clamp(0.0, 1.0);
    let target_ratio = target_ratio_from_strength(strength);
    let alpha = AlphaSignal {
        symbol: symbol.to_string(),
        side_bias: if alpha_mu > 0.0 {
            AlphaSideBias::Long
        } else {
            AlphaSideBias::Flat
        },
        strength,
        expected_return_usdt: alpha_mu * order_amount_usdt.max(0.0),
        risk_estimate_usdt: order_amount_usdt.max(0.0),
        horizon_ms: 60_000,
        confidence: confidence_from_strength(strength),
        timestamp_ms: now_ms,
    };

    let regime_confidence = regime.confidence.max(0.0).min(1.0);
    let regime_target_ratio = target_ratio * regime_entry_multiplier(&regime) * regime_confidence;

    if is_flat {
        if alpha_mu <= 0.0 {
            return PortfolioDecision {
                alpha,
                target_position_ratio: 0.0,
                execution_signal: Signal::Hold,
                regime: regime.regime,
                regime_confidence,
                reason: "portfolio.alpha.hold_flat",
            };
        }
        if regime.regime == MarketRegime::TrendDown || regime.regime == MarketRegime::Unknown {
            return PortfolioDecision {
                alpha,
                target_position_ratio: 0.0,
                execution_signal: Signal::Hold,
                regime: regime.regime,
                regime_confidence,
                reason: "portfolio.regime.blocked",
            };
        }
        if regime_target_ratio < PORTFOLIO_MIN_ENTRY_RATIO {
            return PortfolioDecision {
                alpha,
                target_position_ratio: 0.0,
                execution_signal: Signal::Hold,
                regime: regime.regime,
                regime_confidence,
                reason: "portfolio.regime.too_small",
            };
        }
        return PortfolioDecision {
            alpha,
            target_position_ratio: regime_target_ratio.min(1.0),
            execution_signal: Signal::Buy,
            regime: regime.regime,
            regime_confidence,
            reason: "portfolio.alpha.entry",
        };
    }

    if alpha_mu <= 0.0 {
        return PortfolioDecision {
            alpha,
            target_position_ratio: 0.0,
            execution_signal: Signal::Sell,
            regime: regime.regime,
            regime_confidence,
            reason: "portfolio.alpha.exit",
        };
    }

    let hold_ratio =
        target_ratio * regime_hold_multiplier(&regime) * regime_confidence.clamp(0.2, 1.0);
    PortfolioDecision {
        alpha,
        target_position_ratio: hold_ratio.min(1.0),
        execution_signal: Signal::Hold,
        regime: regime.regime,
        regime_confidence,
        reason: "portfolio.alpha.hold",
    }
}

fn regime_entry_multiplier(regime: &MarketRegimeSignal) -> f64 {
    match regime.regime {
        MarketRegime::TrendUp => 1.0,
        MarketRegime::TrendDown => 0.0,
        MarketRegime::Range => 0.5,
        MarketRegime::Unknown => 0.0,
    }
}

fn regime_hold_multiplier(regime: &MarketRegimeSignal) -> f64 {
    match regime.regime {
        MarketRegime::TrendUp => 1.0,
        MarketRegime::TrendDown => 0.75,
        MarketRegime::Range => 0.5,
        MarketRegime::Unknown => 0.75,
    }
}

fn confidence_from_strength(strength: f64) -> f64 {
    strength.clamp(0.0, 1.0)
}

fn target_ratio_from_strength(strength: f64) -> f64 {
    if strength >= 0.8 {
        1.0
    } else if strength >= 0.6 {
        0.75
    } else if strength >= 0.4 {
        0.5
    } else if strength >= 0.2 {
        0.25
    } else {
        0.1
    }
}

const PORTFOLIO_MIN_ENTRY_RATIO: f64 = 0.15;
