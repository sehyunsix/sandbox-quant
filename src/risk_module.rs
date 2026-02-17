use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::binance::rest::BinanceRestClient;
use crate::model::order::OrderSide;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketKind {
    Spot,
    Futures,
}

/// Stable taxonomy for order rejection reasons emitted by the risk path.
#[derive(Debug, Clone, Copy)]
pub enum RejectionReasonCode {
    RiskNoPriceData,
    RiskNoSpotBaseBalance,
    RiskQtyTooSmall,
    RiskQtyBelowMin,
    RiskQtyAboveMax,
    RiskInsufficientQuoteBalance,
    RiskInsufficientBaseBalance,
    RateGlobalBudgetExceeded,
    BrokerSubmitFailed,
    RiskUnknown,
}

impl RejectionReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RiskNoPriceData => "risk.no_price_data",
            Self::RiskNoSpotBaseBalance => "risk.no_spot_base_balance",
            Self::RiskQtyTooSmall => "risk.qty_too_small",
            Self::RiskQtyBelowMin => "risk.qty_below_min",
            Self::RiskQtyAboveMax => "risk.qty_above_max",
            Self::RiskInsufficientQuoteBalance => "risk.insufficient_quote_balance",
            Self::RiskInsufficientBaseBalance => "risk.insufficient_base_balance",
            Self::RateGlobalBudgetExceeded => "rate.global_budget_exceeded",
            Self::BrokerSubmitFailed => "broker.submit_failed",
            Self::RiskUnknown => "risk.unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderIntent {
    /// Globally unique ID for this intent.
    pub intent_id: String,
    /// Source strategy tag (e.g. `cfg`, `fst`, `mnl`).
    pub source_tag: String,
    /// Trading symbol (e.g. `BTCUSDT`).
    pub symbol: String,
    /// Spot/Futures market kind.
    pub market: MarketKind,
    /// Intended order side.
    pub side: OrderSide,
    /// Notional size basis in USDT.
    pub order_amount_usdt: f64,
    /// Last known mark/last trade price.
    pub last_price: f64,
    /// Millisecond timestamp when intent was created.
    pub created_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct RiskDecision {
    /// `true` if intent passed checks and can be submitted.
    pub approved: bool,
    /// Quantity after exchange/risk normalization.
    pub normalized_qty: f64,
    /// Machine-readable reason code when rejected.
    pub reason_code: Option<String>,
    /// Human-readable rejection reason.
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct RateBudgetSnapshot {
    /// Consumed request budget in current minute window.
    pub used: u32,
    /// Total budget limit in current minute window.
    pub limit: u32,
    /// Milliseconds until budget window reset.
    pub reset_in_ms: u64,
}

pub struct RiskModule {
    rest_client: Arc<BinanceRestClient>,
    rate_budget_window_started_at: Instant,
    rate_budget_used: u32,
    rate_budget_limit_per_minute: u32,
}

impl RiskModule {
    /// Build a risk module with a per-minute global rate budget.
    pub fn new(rest_client: Arc<BinanceRestClient>, global_rate_limit_per_minute: u32) -> Self {
        Self {
            rest_client,
            rate_budget_window_started_at: Instant::now(),
            rate_budget_used: 0,
            rate_budget_limit_per_minute: global_rate_limit_per_minute.max(1),
        }
    }

    /// Return current global rate-budget usage.
    pub fn rate_budget_snapshot(&self) -> RateBudgetSnapshot {
        let elapsed = self.rate_budget_window_started_at.elapsed();
        let reset = Duration::from_secs(60).saturating_sub(elapsed);
        RateBudgetSnapshot {
            used: self.rate_budget_used,
            limit: self.rate_budget_limit_per_minute,
            reset_in_ms: reset.as_millis() as u64,
        }
    }

    /// Reserve one unit from the global rate budget.
    /// Returns `false` when the current minute budget is exhausted.
    pub fn reserve_rate_budget(&mut self) -> bool {
        if self.rate_budget_window_started_at.elapsed() >= Duration::from_secs(60) {
            self.rate_budget_window_started_at = Instant::now();
            self.rate_budget_used = 0;
        }
        if self.rate_budget_used >= self.rate_budget_limit_per_minute {
            return false;
        }
        self.rate_budget_used += 1;
        true
    }

    /// Evaluate an order intent against risk rules and exchange filters.
    ///
    /// This performs quantity normalization, min/max checks, and spot balance checks.
    pub async fn evaluate_intent(
        &self,
        intent: &OrderIntent,
        balances: &HashMap<String, f64>,
    ) -> Result<RiskDecision> {
        if intent.last_price <= 0.0 {
            return Ok(RiskDecision {
                approved: false,
                normalized_qty: 0.0,
                reason_code: Some(RejectionReasonCode::RiskNoPriceData.as_str().to_string()),
                reason: Some("No price data yet".to_string()),
            });
        }

        let raw_qty = match intent.side {
            OrderSide::Buy => intent.order_amount_usdt / intent.last_price,
            OrderSide::Sell => {
                if intent.market == MarketKind::Spot {
                    let (base_asset, _) = split_symbol_assets(&intent.symbol);
                    let base_free = balances.get(base_asset.as_str()).copied().unwrap_or(0.0);
                    if base_free <= f64::EPSILON {
                        return Ok(RiskDecision {
                            approved: false,
                            normalized_qty: 0.0,
                            reason_code: Some(
                                RejectionReasonCode::RiskNoSpotBaseBalance
                                    .as_str()
                                    .to_string(),
                            ),
                            reason: Some(format!("No {} balance to sell", base_asset)),
                        });
                    }
                    base_free
                } else {
                    intent.order_amount_usdt / intent.last_price
                }
            }
        };

        let rules = if intent.market == MarketKind::Futures {
            self.rest_client
                .get_futures_symbol_order_rules(&intent.symbol)
                .await?
        } else {
            self.rest_client
                .get_spot_symbol_order_rules(&intent.symbol)
                .await?
        };

        let qty = if intent.market == MarketKind::Futures {
            let mut required = rules.min_qty.max(raw_qty);
            if let Some(min_notional) = rules.min_notional {
                if min_notional > 0.0 && intent.last_price > 0.0 {
                    required = required.max(min_notional / intent.last_price);
                }
            }
            ceil_to_step(required, rules.step_size)
        } else {
            floor_to_step(raw_qty, rules.step_size)
        };

        if qty <= 0.0 {
            return Ok(RiskDecision {
                approved: false,
                normalized_qty: 0.0,
                reason_code: Some(RejectionReasonCode::RiskQtyTooSmall.as_str().to_string()),
                reason: Some(format!(
                    "Calculated qty too small after normalization (raw {:.8}, step {:.8}, minQty {:.8})",
                    raw_qty, rules.step_size, rules.min_qty
                )),
            });
        }
        if qty < rules.min_qty {
            return Ok(RiskDecision {
                approved: false,
                normalized_qty: 0.0,
                reason_code: Some(RejectionReasonCode::RiskQtyBelowMin.as_str().to_string()),
                reason: Some(format!(
                    "Qty below minQty (qty {:.8} < min {:.8}, step {:.8})",
                    qty, rules.min_qty, rules.step_size
                )),
            });
        }
        if rules.max_qty > 0.0 && qty > rules.max_qty {
            return Ok(RiskDecision {
                approved: false,
                normalized_qty: 0.0,
                reason_code: Some(RejectionReasonCode::RiskQtyAboveMax.as_str().to_string()),
                reason: Some(format!(
                    "Qty above maxQty (qty {:.8} > max {:.8})",
                    qty, rules.max_qty
                )),
            });
        }

        if intent.market == MarketKind::Spot {
            let (base_asset, quote_asset) = split_symbol_assets(&intent.symbol);
            match intent.side {
                OrderSide::Buy => {
                    let quote_asset_name = if quote_asset.is_empty() {
                        "USDT"
                    } else {
                        quote_asset.as_str()
                    };
                    let quote_free = balances.get(quote_asset_name).copied().unwrap_or(0.0);
                    let order_value = qty * intent.last_price;
                    if quote_free < order_value {
                        return Ok(RiskDecision {
                            approved: false,
                            normalized_qty: 0.0,
                            reason_code: Some(
                                RejectionReasonCode::RiskInsufficientQuoteBalance
                                    .as_str()
                                    .to_string(),
                            ),
                            reason: Some(format!(
                                "Insufficient {}: need {:.2}, have {:.2}",
                                quote_asset_name, order_value, quote_free
                            )),
                        });
                    }
                }
                OrderSide::Sell => {
                    let base_free = balances.get(base_asset.as_str()).copied().unwrap_or(0.0);
                    if base_free < qty {
                        return Ok(RiskDecision {
                            approved: false,
                            normalized_qty: 0.0,
                            reason_code: Some(
                                RejectionReasonCode::RiskInsufficientBaseBalance
                                    .as_str()
                                    .to_string(),
                            ),
                            reason: Some(format!(
                                "Insufficient {}: need {:.5}, have {:.5}",
                                base_asset, qty, base_free
                            )),
                        });
                    }
                }
            }
        }

        Ok(RiskDecision {
            approved: true,
            normalized_qty: qty,
            reason_code: None,
            reason: None,
        })
    }
}

fn floor_to_step(value: f64, step: f64) -> f64 {
    if !value.is_finite() || !step.is_finite() || step <= 0.0 {
        return 0.0;
    }
    let units = (value / step).floor();
    let floored = units * step;
    if floored < 0.0 { 0.0 } else { floored }
}

fn ceil_to_step(value: f64, step: f64) -> f64 {
    if !value.is_finite() || !step.is_finite() || step <= 0.0 {
        return 0.0;
    }
    let units = (value / step).ceil();
    let ceiled = units * step;
    if ceiled < 0.0 { 0.0 } else { ceiled }
}

fn split_symbol_assets(symbol: &str) -> (String, String) {
    const QUOTE_SUFFIXES: [&str; 10] = [
        "USDT", "USDC", "FDUSD", "BUSD", "TUSD", "TRY", "EUR", "BTC", "ETH", "BNB",
    ];
    for q in QUOTE_SUFFIXES {
        if let Some(base) = symbol.strip_suffix(q) {
            if !base.is_empty() {
                return (base.to_string(), q.to_string());
            }
        }
    }
    (symbol.to_string(), String::new())
}
