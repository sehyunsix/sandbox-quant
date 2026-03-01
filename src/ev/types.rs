#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct ProbabilitySnapshot {
    pub p_win: f64,
    pub p_tail_loss: f64,
    pub p_timeout_exit: f64,
    pub n_eff: f64,
    pub confidence: ConfidenceLevel,
    pub prob_model_version: String,
}

#[derive(Debug, Clone)]
pub struct EntryExpectancySnapshot {
    pub expected_return_usdt: f64,
    pub expected_holding_ms: u64,
    pub worst_case_loss_usdt: f64,
    pub fee_slippage_penalty_usdt: f64,
    pub probability: ProbabilitySnapshot,
    pub ev_model_version: String,
    pub computed_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct TradeStatsSample {
    pub age_days: f64,
    pub pnl_usdt: f64,
    pub holding_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct TradeStatsWindow {
    pub samples: Vec<TradeStatsSample>,
}

impl TradeStatsWindow {
    pub fn n_eff(&self, recency_lambda: f64) -> f64 {
        self.samples
            .iter()
            .map(|s| recency_weight(s.age_days, recency_lambda))
            .sum()
    }

    pub fn weighted_win_loss(&self, recency_lambda: f64) -> (f64, f64) {
        let mut wins = 0.0;
        let mut losses = 0.0;
        for s in &self.samples {
            let w = recency_weight(s.age_days, recency_lambda);
            if s.pnl_usdt > 0.0 {
                wins += w;
            } else if s.pnl_usdt < 0.0 {
                losses += w;
            }
        }
        (wins, losses)
    }

    pub fn weighted_tail_events(
        &self,
        recency_lambda: f64,
        loss_threshold_usdt: f64,
    ) -> (f64, f64) {
        let mut tail_events = 0.0;
        let mut loss_events = 0.0;
        for s in &self.samples {
            if s.pnl_usdt >= 0.0 {
                continue;
            }
            let w = recency_weight(s.age_days, recency_lambda);
            loss_events += w;
            if s.pnl_usdt <= -loss_threshold_usdt {
                tail_events += w;
            }
        }
        (tail_events, loss_events)
    }

    pub fn weighted_avg_win_loss(&self, recency_lambda: f64) -> (f64, f64) {
        let mut win_sum = 0.0;
        let mut win_w = 0.0;
        let mut loss_sum = 0.0;
        let mut loss_w = 0.0;
        for s in &self.samples {
            let w = recency_weight(s.age_days, recency_lambda);
            if s.pnl_usdt > 0.0 {
                win_sum += s.pnl_usdt * w;
                win_w += w;
            } else if s.pnl_usdt < 0.0 {
                loss_sum += s.pnl_usdt.abs() * w;
                loss_w += w;
            }
        }
        let avg_win = if win_w > f64::EPSILON {
            win_sum / win_w
        } else {
            0.0
        };
        let avg_loss = if loss_w > f64::EPSILON {
            loss_sum / loss_w
        } else {
            0.0
        };
        (avg_win, avg_loss)
    }

    pub fn median_holding_ms(&self) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        let mut values: Vec<u64> = self.samples.iter().map(|s| s.holding_ms).collect();
        values.sort_unstable();
        values[values.len() / 2]
    }

    pub fn q05_loss_abs_usdt(&self) -> f64 {
        let mut losses: Vec<f64> = self
            .samples
            .iter()
            .filter_map(|s| (s.pnl_usdt < 0.0).then_some(s.pnl_usdt.abs()))
            .collect();
        if losses.is_empty() {
            return 0.0;
        }
        losses.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((losses.len() as f64) * 0.05).floor() as usize;
        losses[idx.min(losses.len() - 1)]
    }
}

pub fn recency_weight(age_days: f64, lambda: f64) -> f64 {
    (-lambda.max(0.0) * age_days.max(0.0)).exp()
}
