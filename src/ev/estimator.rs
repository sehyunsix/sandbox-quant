use anyhow::Result;

use crate::ev::types::{
    ConfidenceLevel, EntryExpectancySnapshot, ProbabilitySnapshot, TradeStatsWindow,
};

#[derive(Debug, Clone)]
pub struct EvEstimatorConfig {
    pub prior_a: f64,
    pub prior_b: f64,
    pub tail_prior_a: f64,
    pub tail_prior_b: f64,
    pub recency_lambda: f64,
    pub shrink_k: f64,
    pub loss_threshold_usdt: f64,
    pub timeout_ms_default: u64,
    pub gamma_tail_penalty: f64,
    pub fee_slippage_penalty_usdt: f64,
    pub prob_model_version: String,
    pub ev_model_version: String,
}

impl Default for EvEstimatorConfig {
    fn default() -> Self {
        Self {
            prior_a: 6.0,
            prior_b: 6.0,
            tail_prior_a: 3.0,
            tail_prior_b: 7.0,
            recency_lambda: 0.08,
            shrink_k: 40.0,
            loss_threshold_usdt: 15.0,
            timeout_ms_default: 1_800_000,
            gamma_tail_penalty: 0.8,
            fee_slippage_penalty_usdt: 0.0,
            prob_model_version: "beta-binomial-v1".to_string(),
            ev_model_version: "ev-conservative-v1".to_string(),
        }
    }
}

pub trait TradeStatsReader {
    fn load_local_stats(
        &self,
        source_tag: &str,
        instrument: &str,
        lookback: usize,
    ) -> Result<TradeStatsWindow>;
    fn load_global_stats(&self, source_tag: &str, lookback: usize) -> Result<TradeStatsWindow>;
}

pub struct EvEstimator<R: TradeStatsReader> {
    cfg: EvEstimatorConfig,
    reader: R,
    lookback: usize,
}

impl<R: TradeStatsReader> EvEstimator<R> {
    pub fn new(cfg: EvEstimatorConfig, reader: R, lookback: usize) -> Self {
        Self {
            cfg,
            reader,
            lookback: lookback.max(1),
        }
    }

    pub fn estimate_entry_expectancy(
        &self,
        source_tag: &str,
        instrument: &str,
        now_ms: u64,
    ) -> Result<EntryExpectancySnapshot> {
        let local = self
            .reader
            .load_local_stats(source_tag, instrument, self.lookback)?;
        let global = self.reader.load_global_stats(source_tag, self.lookback)?;

        let p_win_local = posterior_win_prob(
            &local,
            self.cfg.recency_lambda,
            self.cfg.prior_a,
            self.cfg.prior_b,
        );
        let p_win_global = posterior_win_prob(
            &global,
            self.cfg.recency_lambda,
            self.cfg.prior_a,
            self.cfg.prior_b,
        );
        let n_eff = local.n_eff(self.cfg.recency_lambda);
        let alpha = n_eff / (n_eff + self.cfg.shrink_k.max(1e-9));
        let p_win = alpha * p_win_local + (1.0 - alpha) * p_win_global;

        let p_tail_loss = posterior_tail_prob(
            &local,
            self.cfg.recency_lambda,
            self.cfg.loss_threshold_usdt,
            self.cfg.tail_prior_a,
            self.cfg.tail_prior_b,
        );
        let p_timeout_exit = timeout_prob(&local, self.cfg.timeout_ms_default);
        let (avg_win, avg_loss) = local.weighted_avg_win_loss(self.cfg.recency_lambda);
        let q05_loss = local.q05_loss_abs_usdt();

        let ev = p_win * avg_win - (1.0 - p_win) * avg_loss - self.cfg.fee_slippage_penalty_usdt;
        let ev_conservative = ev - self.cfg.gamma_tail_penalty * p_tail_loss * q05_loss;

        let expected_holding_ms = {
            let median = local.median_holding_ms();
            if median == 0 {
                self.cfg.timeout_ms_default.max(1)
            } else {
                median
            }
        };

        let confidence = confidence_from_n_eff(n_eff);

        Ok(EntryExpectancySnapshot {
            expected_return_usdt: ev_conservative,
            expected_holding_ms,
            worst_case_loss_usdt: q05_loss,
            fee_slippage_penalty_usdt: self.cfg.fee_slippage_penalty_usdt,
            probability: ProbabilitySnapshot {
                p_win,
                p_tail_loss,
                p_timeout_exit,
                n_eff,
                confidence,
                prob_model_version: self.cfg.prob_model_version.clone(),
            },
            ev_model_version: self.cfg.ev_model_version.clone(),
            computed_at_ms: now_ms,
        })
    }
}

fn posterior_win_prob(
    window: &TradeStatsWindow,
    recency_lambda: f64,
    prior_a: f64,
    prior_b: f64,
) -> f64 {
    let (wins, losses) = window.weighted_win_loss(recency_lambda);
    (prior_a + wins) / (prior_a + prior_b + wins + losses).max(1e-9)
}

fn posterior_tail_prob(
    window: &TradeStatsWindow,
    recency_lambda: f64,
    loss_threshold_usdt: f64,
    prior_a: f64,
    prior_b: f64,
) -> f64 {
    let (tail_events, loss_events) = window.weighted_tail_events(recency_lambda, loss_threshold_usdt);
    (prior_a + tail_events) / (prior_a + prior_b + loss_events).max(1e-9)
}

fn timeout_prob(window: &TradeStatsWindow, threshold_ms: u64) -> f64 {
    let mut total = 0usize;
    let mut timeout = 0usize;
    for sample in &window.samples {
        total += 1;
        if sample.holding_ms > threshold_ms {
            timeout += 1;
        }
    }
    if total == 0 {
        return 0.5;
    }
    timeout as f64 / total as f64
}

fn confidence_from_n_eff(n_eff: f64) -> ConfidenceLevel {
    if n_eff >= 80.0 {
        ConfidenceLevel::High
    } else if n_eff >= 20.0 {
        ConfidenceLevel::Medium
    } else {
        ConfidenceLevel::Low
    }
}
