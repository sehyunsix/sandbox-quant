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
            return Signal::Buy;
        }
        if self.position_delta_ratio < -threshold {
            return Signal::Sell;
        }
        Signal::Hold
    }
}

pub fn decide_portfolio_action_from_alpha(
    symbol: &str,
    now_ms: u64,
    is_flat: bool,
    alpha_mu: f64,
    order_amount_usdt: f64,
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
    if is_flat {
        if alpha_mu > 0.0 {
            return PortfolioDecision {
                alpha,
                target_position_ratio: target_ratio,
                execution_signal: Signal::Buy,
                reason: "portfolio.alpha.entry",
            };
        }
        return PortfolioDecision {
            alpha,
            target_position_ratio: 0.0,
            execution_signal: Signal::Hold,
            reason: "portfolio.alpha.hold_flat",
        };
    }
    if alpha_mu <= 0.0 {
        return PortfolioDecision {
            alpha,
            target_position_ratio: 0.0,
            execution_signal: Signal::Sell,
            reason: "portfolio.alpha.exit",
        };
    }
    PortfolioDecision {
        alpha,
        target_position_ratio: target_ratio,
        execution_signal: Signal::Hold,
        reason: "portfolio.alpha.hold",
    }
}

fn confidence_from_strength(strength: f64) -> f64 {
    strength.clamp(0.0, 1.0)
}

fn target_ratio_from_strength(strength: f64) -> f64 {
    // Stepwise sizing keeps behavior predictable while enabling future
    // portfolio sizing upgrades.
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
