use std::collections::HashMap;

use crate::ev::price_model::YNormal;

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
}

impl EwmaYModel {
    pub fn new(cfg: EwmaYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        if price <= f64::EPSILON {
            return;
        }
        let st = self
            .by_instrument
            .entry(instrument.to_string())
            .or_default();
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                let a_mu = self.cfg.alpha_mean.clamp(0.0, 1.0);
                let a_var = self.cfg.alpha_var.clamp(0.0, 1.0);
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

    pub fn estimate(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
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
}
