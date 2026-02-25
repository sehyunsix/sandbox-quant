#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionSide {
    Long,
    Short,
}

#[derive(Debug, Clone, Copy)]
pub struct YNormal {
    pub mu: f64,
    pub sigma: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct SpotEvInputs {
    pub p0: f64,
    pub qty: f64,
    pub side: PositionSide,
    pub fee: f64,
    pub slippage: f64,
    pub borrow: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct FuturesEvInputs {
    pub p0: f64,
    pub qty: f64,
    pub multiplier: f64,
    pub side: PositionSide,
    pub fee: f64,
    pub slippage: f64,
    pub funding: f64,
    pub liq_risk: f64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EvStats {
    pub ev: f64,
    pub ev_std: f64,
    pub p_win: f64,
}

pub fn spot_ev_from_y_normal(y: YNormal, i: SpotEvInputs) -> EvStats {
    let p0 = i.p0.max(0.0);
    let qty = i.qty.abs();
    if p0 <= f64::EPSILON || qty <= f64::EPSILON {
        return EvStats::default();
    }
    let sigma = y.sigma.max(0.0);
    let cost = i.fee + i.slippage + i.borrow;
    let signed = side_sign(i.side);
    let (e_pt, var_pt) = lognormal_moments(p0, y.mu, sigma);
    let pnl_mean = signed * qty * (e_pt - p0) - cost;
    let pnl_std = (qty * var_pt.sqrt()).abs();
    let p_win = p_win_lognormal(y.mu, sigma, p0, qty, signed, cost);
    EvStats {
        ev: pnl_mean,
        ev_std: pnl_std,
        p_win,
    }
}

pub fn futures_ev_from_y_normal(y: YNormal, i: FuturesEvInputs) -> EvStats {
    let p0 = i.p0.max(0.0);
    let qty = i.qty.abs();
    let m = i.multiplier.abs();
    if p0 <= f64::EPSILON || qty <= f64::EPSILON || m <= f64::EPSILON {
        return EvStats::default();
    }
    let sigma = y.sigma.max(0.0);
    let cost = i.fee + i.slippage + i.funding + i.liq_risk;
    let signed = side_sign(i.side);
    let scale = m * qty;
    let (e_pt, var_pt) = lognormal_moments(p0, y.mu, sigma);
    let pnl_mean = signed * scale * (e_pt - p0) - cost;
    let pnl_std = (scale * var_pt.sqrt()).abs();
    let p_win = p_win_lognormal(y.mu, sigma, p0, scale, signed, cost);
    EvStats {
        ev: pnl_mean,
        ev_std: pnl_std,
        p_win,
    }
}

fn side_sign(side: PositionSide) -> f64 {
    match side {
        PositionSide::Long => 1.0,
        PositionSide::Short => -1.0,
    }
}

fn lognormal_moments(p0: f64, mu: f64, sigma: f64) -> (f64, f64) {
    let sigma2 = sigma * sigma;
    let e_pt = p0 * (mu + 0.5 * sigma2).exp();
    let var_pt = e_pt * e_pt * (sigma2.exp() - 1.0).max(0.0);
    (e_pt, var_pt)
}

fn p_win_lognormal(mu: f64, sigma: f64, p0: f64, scale: f64, signed: f64, cost: f64) -> f64 {
    if scale <= f64::EPSILON || p0 <= f64::EPSILON {
        return 0.0;
    }
    let thresh = if signed > 0.0 {
        p0 + cost / scale
    } else {
        p0 - cost / scale
    };
    if thresh <= 0.0 {
        return if signed > 0.0 { 1.0 } else { 0.0 };
    }
    if sigma <= f64::EPSILON {
        let pt = p0 * mu.exp();
        return if signed > 0.0 {
            (pt > thresh) as i32 as f64
        } else {
            (pt < thresh) as i32 as f64
        };
    }
    let z = ((thresh / p0).ln() - mu) / sigma;
    if signed > 0.0 {
        1.0 - normal_cdf(z)
    } else {
        normal_cdf(z)
    }
}

fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf_approx(x / 2f64.sqrt()))
}

// Abramowitz-Stegun style approximation; sufficient for gating/probability display.
fn erf_approx(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let y = 1.0 - (((((a5 * t + a4) * t + a3) * t + a2) * t + a1) * t * (-x * x).exp());
    sign * y
}
