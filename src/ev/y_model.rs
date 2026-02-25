use std::collections::HashMap;

use crate::ev::price_model::YNormal;
use crate::model::signal::Signal;

#[derive(Debug, Clone, Copy)]
pub struct EwmaYModelConfig {
    pub alpha_mean: f64,
    pub alpha_var: f64,
    pub min_sigma: f64,
}

impl Default for EwmaYModelConfig {
    fn default() -> Self {
        Self {
            alpha_mean: 0.08,
            alpha_var: 0.08,
            min_sigma: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct EwmaState {
    last_price: Option<f64>,
    mu: f64,
    var: f64,
    samples: u64,
}

#[derive(Debug, Default)]
pub struct EwmaYModel {
    cfg: EwmaYModelConfig,
    by_instrument: HashMap<String, EwmaState>,
    by_scope_side: HashMap<String, EwmaState>,
}

impl EwmaYModel {
    pub fn new(cfg: EwmaYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(&self, instrument: &str, fallback_mu: f64, fallback_sigma: f64) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        let sigma = st.var.max(0.0).sqrt().max(self.cfg.min_sigma);
        let mu = if st.samples == 0 { fallback_mu } else { st.mu };
        YNormal { mu, sigma }
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        // Shrink conditioned estimate toward base to avoid noisy overreaction.
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let base_var = base.sigma * base.sigma;
        let scoped_var = scoped.var.max(0.0);
        let mu = w * scoped.mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_var + (1.0 - w) * base_var)
            .max(0.0)
            .sqrt()
            .max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    pub fn estimate(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        self.estimate_base(instrument, fallback_mu, fallback_sigma)
    }

    fn update_state(st: &mut EwmaState, price: f64, cfg: EwmaYModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                let a_mu = cfg.alpha_mean.clamp(0.0, 1.0);
                let a_var = cfg.alpha_var.clamp(0.0, 1.0);
                st.mu = if st.samples == 0 {
                    r
                } else {
                    (1.0 - a_mu) * st.mu + a_mu * r
                };
                let centered = r - st.mu;
                let sample_var = centered * centered;
                st.var = if st.samples == 0 {
                    sample_var
                } else {
                    (1.0 - a_var) * st.var + a_var * sample_var
                };
                st.samples = st.samples.saturating_add(1);
            }
        }
        st.last_price = Some(price);
    }
}

fn scoped_side_key(instrument: &str, source_tag: &str, signal: &Signal) -> String {
    let side = match signal {
        Signal::Buy => "buy",
        Signal::Sell => "sell",
        Signal::Hold => "hold",
    };
    format!(
        "{}::{}::{}",
        instrument.trim().to_ascii_uppercase(),
        source_tag.trim().to_ascii_lowercase(),
        side
    )
}
