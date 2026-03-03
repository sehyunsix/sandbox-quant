#[derive(Debug, Clone, Copy, Default)]
pub struct PredictorEvalVolState {
    pub last_price: Option<f64>,
    pub var: f64,
    pub ready: bool,
}

pub fn observe_predictor_eval_volatility(st: &mut PredictorEvalVolState, price: f64, alpha_var: f64) {
    if price <= f64::EPSILON {
        return;
    }
    if let Some(prev) = st.last_price {
        if prev > f64::EPSILON {
            let r = (price / prev).ln();
            if !st.ready {
                st.var = r * r;
                st.ready = true;
            } else {
                let a = alpha_var.clamp(0.0, 1.0);
                st.var = (1.0 - a) * st.var + a * (r * r);
            }
        }
    }
    st.last_price = Some(price);
}

pub fn predictor_eval_scale(st: &PredictorEvalVolState, min_sigma: f64) -> f64 {
    if st.ready {
        st.var.max(0.0).sqrt().max(min_sigma.max(1e-8))
    } else {
        min_sigma.max(1e-8)
    }
}
