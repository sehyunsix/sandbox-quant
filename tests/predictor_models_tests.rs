use sandbox_quant::ev::EwmaYModelConfig;
use sandbox_quant::model::signal::Signal;
use sandbox_quant::predictor::{
    backfill_predictor_metrics_from_closes, build_predictor_models, default_predictor_horizons,
    default_predictor_specs, OnlinePredictorMetrics, PREDICTOR_R2_MIN_SAMPLES,
};

#[test]
fn default_predictor_specs_include_ar1_variants() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let ids: Vec<String> = specs.iter().map(|(id, _)| id.clone()).collect();
    assert!(ids.contains(&"ewma-v1".to_string()));
    assert!(ids.contains(&"ar1-v1".to_string()));
    assert!(ids.contains(&"ar1-fast-v1".to_string()));
    assert!(ids.contains(&"holt-v1".to_string()));
    assert!(ids.contains(&"kalman-v1".to_string()));
    assert!(ids.contains(&"lin-ind-v1".to_string()));
    assert!(ids.contains(&"tsmom-rls-v1".to_string()));
    assert!(ids.contains(&"ou-revert-v1".to_string()));
    assert!(ids.contains(&"ou-revert-fast-v1".to_string()));
    assert!(ids.contains(&"volmom-v1".to_string()));
    assert!(ids.contains(&"volmom-fast-v1".to_string()));
    assert!(ids.contains(&"varratio-v1".to_string()));
    assert!(ids.contains(&"varratio-fast-v1".to_string()));
    assert!(ids.contains(&"microrev-v1".to_string()));
    assert!(ids.contains(&"microrev-fast-v1".to_string()));
    assert!(ids.contains(&"selfcalib-v1".to_string()));
    assert!(ids.contains(&"selfcalib-fast-v1".to_string()));
    assert!(ids.contains(&"feat-rls-v1".to_string()));
    assert!(ids.contains(&"feat-rls-fast-v1".to_string()));
    assert!(ids.contains(&"xasset-macro-rls-v1".to_string()));
}

#[test]
fn build_predictor_models_constructs_requested_models() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let models = build_predictor_models(&specs);
    assert!(models.contains_key("ewma-v1"));
    assert!(models.contains_key("ar1-v1"));
    assert!(models.contains_key("holt-v1"));
    assert!(models.contains_key("kalman-v1"));
    assert!(models.contains_key("lin-ind-v1"));
    assert!(models.contains_key("tsmom-rls-v1"));
    assert!(models.contains_key("ou-revert-v1"));
    assert!(models.contains_key("ou-revert-fast-v1"));
    assert!(models.contains_key("volmom-v1"));
    assert!(models.contains_key("volmom-fast-v1"));
    assert!(models.contains_key("varratio-v1"));
    assert!(models.contains_key("varratio-fast-v1"));
    assert!(models.contains_key("microrev-v1"));
    assert!(models.contains_key("microrev-fast-v1"));
    assert!(models.contains_key("selfcalib-v1"));
    assert!(models.contains_key("selfcalib-fast-v1"));
    assert!(models.contains_key("xasset-macro-rls-v1"));
}

#[test]
fn ar1_predictor_estimate_changes_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models.get_mut("ar1-v1").expect("missing ar1-v1");

    for p in [100.0, 100.3, 100.5, 100.2, 100.6, 100.7, 100.4] {
        m.observe_price("BTCUSDT", p);
        m.observe_signal_price("BTCUSDT", "cfg", &Signal::Buy, p);
    }

    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
}

#[test]
fn metrics_backfill_produces_non_empty_samples() {
    let closes = vec![100.0, 101.0, 100.8, 101.2, 101.1, 101.4];
    let metrics = backfill_predictor_metrics_from_closes(&closes, 0.1, 120);
    assert!(metrics.sample_count() > 0);
    assert!(metrics.mae().is_some());
}

#[test]
fn online_metrics_r2_is_defined_after_multiple_points() {
    let mut m = OnlinePredictorMetrics::with_window(PREDICTOR_R2_MIN_SAMPLES + 16);
    for i in 0..PREDICTOR_R2_MIN_SAMPLES {
        let x = i as f64;
        let y = (x.sin()) * 0.01;
        let yhat = (x.sin()) * 0.009;
        m.observe(y, yhat);
    }
    assert!(m.r2().is_some());
}

#[test]
fn default_horizon_is_minute_only() {
    let hs = default_predictor_horizons();
    assert_eq!(hs.len(), 1);
    assert_eq!(hs[0].0, "1m");
    assert_eq!(hs[0].1, 60_000);
}

#[test]
fn holt_predictor_estimate_changes_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models.get_mut("holt-v1").expect("missing holt-v1");

    for p in [100.0, 100.2, 100.5, 100.9, 101.1, 101.3] {
        m.observe_price("BTCUSDT", p);
    }
    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
}

#[test]
fn kalman_predictor_estimate_changes_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models.get_mut("kalman-v1").expect("missing kalman-v1");

    for p in [100.0, 99.8, 100.1, 100.0, 100.2, 100.15] {
        m.observe_price("BTCUSDT", p);
    }
    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
}

#[test]
fn linear_indicator_predictor_estimate_changes_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models.get_mut("lin-ind-v1").expect("missing lin-ind-v1");

    for p in [100.0, 100.2, 100.1, 100.4, 100.35, 100.7, 100.6, 100.9] {
        m.observe_price("BTCUSDT", p);
    }
    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
}

#[test]
fn tsmom_rls_predictor_estimate_changes_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models
        .get_mut("tsmom-rls-v1")
        .expect("missing tsmom-rls-v1");

    for p in [
        100.0, 100.3, 100.5, 100.2, 100.7, 100.8, 101.0, 100.9, 101.1, 101.4,
    ] {
        m.observe_price("BTCUSDT", p);
    }
    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
}

#[test]
fn feature_rls_predictor_estimate_changes_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models.get_mut("feat-rls-v1").expect("missing feat-rls-v1");

    for p in [
        100.0, 100.4, 100.1, 100.7, 100.2, 100.9, 100.5, 101.1, 100.8, 101.2, 100.7, 101.3,
    ] {
        m.observe_price("BTCUSDT", p);
    }
    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
}

#[test]
fn feature_rls_fast_predictor_estimate_changes_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models
        .get_mut("feat-rls-fast-v1")
        .expect("missing feat-rls-fast-v1");

    for p in [
        100.0, 99.9, 100.2, 99.8, 100.3, 99.7, 100.4, 99.6, 100.5, 99.5, 100.6, 99.4,
    ] {
        m.observe_price("BTCUSDT", p);
    }
    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
}

#[test]
fn feature_rls_prediction_is_clipped_to_reasonable_band() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models.get_mut("feat-rls-v1").expect("missing feat-rls-v1");

    // Feed a volatile zig-zag so residual sigma is meaningful.
    for p in [
        100.0, 104.0, 96.0, 103.0, 95.0, 102.0, 94.0, 101.0, 93.0, 100.0, 92.0, 99.0, 91.0,
    ] {
        m.observe_price("BTCUSDT", p);
    }
    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    // pred_clip=3.0 in default specs; allow tiny epsilon for floating-point noise.
    assert!(y.mu.abs() <= 3.0 * y.sigma + 1e-9);
}

#[test]
fn cross_asset_macro_rls_uses_factor_ticks_and_estimates() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models
        .get_mut("xasset-macro-rls-v1")
        .expect("missing xasset-macro-rls-v1");

    // Feed macro factors first
    for p in [5000.0, 5002.0, 5005.0, 5004.0, 5007.0] {
        m.observe_price("SPX500", p);
    }
    for p in [2300.0, 2298.0, 2302.0, 2304.0, 2301.0] {
        m.observe_price("XAUUSD", p);
    }
    for p in [80.0, 80.3, 79.9, 80.1, 80.6] {
        m.observe_price("WTI", p);
    }

    // Feed target crypto prices
    for p in [100.0, 100.2, 99.9, 100.3, 100.1, 100.5, 100.4, 100.7] {
        m.observe_price("BTCUSDT", p);
    }

    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
}

#[test]
fn ou_revert_predictor_estimate_changes_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models
        .get_mut("ou-revert-v1")
        .expect("missing ou-revert-v1");

    for p in [100.0, 100.3, 100.5, 100.2, 100.6, 100.7, 100.4] {
        m.observe_price("BTCUSDT", p);
        m.observe_signal_price("BTCUSDT", "cfg", &Signal::Buy, p);
    }

    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
    assert!(y.mu.abs() < 0.1, "mu should be small: {}", y.mu);
}

#[test]
fn ou_revert_predicts_reversion_after_uptrend() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models
        .get_mut("ou-revert-v1")
        .expect("missing ou-revert-v1");

    for i in 0..20 {
        let p = 100.0 + (i as f64) * 0.5;
        m.observe_price("BTCUSDT", p);
    }

    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(
        y.mu < 0.005,
        "After uptrend should predict reversion, got: {}",
        y.mu
    );
}

#[test]
fn ou_revert_fast_variant_builds_and_estimates() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models
        .get_mut("ou-revert-fast-v1")
        .expect("missing ou-revert-fast-v1");

    for p in [100.0, 100.5, 101.0, 100.8, 100.2, 99.5] {
        m.observe_price("BTCUSDT", p);
    }
    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
}

#[test]
fn volmom_predictor_estimate_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models.get_mut("volmom-v1").expect("missing volmom-v1");

    for p in [100.0, 100.3, 100.5, 100.8, 101.0, 101.3, 101.5] {
        m.observe_price("BTCUSDT", p);
        m.observe_signal_price("BTCUSDT", "test", &Signal::Buy, p);
    }

    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
    // After consistent uptrend, vol-scaled momentum should predict near-zero or positive
    // (heavy shrinkage with n/(n+100) can make it slightly negative at low sample counts)
    assert!(
        y.mu > -0.001,
        "After uptrend, volmom should be near-zero or positive, got: {}",
        y.mu
    );
}

#[test]
fn volmom_fast_variant_builds_and_estimates() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models
        .get_mut("volmom-fast-v1")
        .expect("missing volmom-fast-v1");

    for p in [100.0, 99.5, 99.0, 98.5, 98.0] {
        m.observe_price("BTCUSDT", p);
    }
    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
    // After consistent downtrend, should predict near-zero or negative
    assert!(
        y.mu < 0.001,
        "After downtrend, volmom should be near-zero or negative, got: {}",
        y.mu
    );
}

#[test]
fn varratio_predictor_estimate_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models.get_mut("varratio-v1").expect("missing varratio-v1");

    // Oscillating prices (mean-reverting regime)
    for p in [100.0, 101.0, 99.5, 101.5, 99.0, 102.0, 98.5, 101.0] {
        m.observe_price("BTCUSDT", p);
    }

    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
    assert!(y.mu.abs() < 0.1, "mu should be reasonable: {}", y.mu);
}

#[test]
fn varratio_fast_variant_builds_and_estimates() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models
        .get_mut("varratio-fast-v1")
        .expect("missing varratio-fast-v1");

    for p in [100.0, 100.2, 100.5, 100.9, 101.4, 102.0, 102.7] {
        m.observe_price("BTCUSDT", p);
    }
    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
}

#[test]
fn microrev_predictor_estimate_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models.get_mut("microrev-v1").expect("missing microrev-v1");

    // Oscillating prices to create negative autocovariance (bid-ask bounce)
    for p in [100.0, 100.5, 99.8, 100.3, 99.9, 100.4, 99.7, 100.2] {
        m.observe_price("BTCUSDT", p);
    }

    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
    // Predictions should be very small due to heavy shrinkage n/(n+200)
    assert!(
        y.mu.abs() < 0.01,
        "microrev predictions should be tiny, got: {}",
        y.mu
    );
}

#[test]
fn microrev_predicts_zero_during_trending() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models.get_mut("microrev-v1").expect("missing microrev-v1");

    // Pure uptrend → positive autocovariance → should predict zero
    for i in 0..10 {
        let p = 100.0 + (i as f64) * 0.3;
        m.observe_price("BTCUSDT", p);
    }

    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    // During trending, gamma1 > 0, prediction should be zero
    assert!(
        y.mu.abs() < 1e-6,
        "microrev should predict ~zero during trend, got: {}",
        y.mu
    );
}

#[test]
fn selfcalib_predictor_estimate_after_observations() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models
        .get_mut("selfcalib-v1")
        .expect("missing selfcalib-v1");

    for p in [
        100.0, 100.3, 100.5, 100.2, 100.6, 100.7, 100.4, 100.8, 100.5, 100.9,
    ] {
        m.observe_price("BTCUSDT", p);
    }

    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
    // Self-calibration should keep predictions small
    assert!(
        y.mu.abs() < 0.01,
        "selfcalib predictions should be small, got: {}",
        y.mu
    );
}

#[test]
fn selfcalib_fast_variant_builds_and_estimates() {
    let specs = default_predictor_specs(EwmaYModelConfig::default());
    let mut models = build_predictor_models(&specs);
    let m = models
        .get_mut("selfcalib-fast-v1")
        .expect("missing selfcalib-fast-v1");

    for p in [100.0, 100.5, 101.0, 100.5, 100.0, 99.5, 100.0] {
        m.observe_price("BTCUSDT", p);
    }
    let y = m.estimate_base("BTCUSDT", 0.0, 0.01);
    assert!(y.sigma > 0.0);
    assert!(y.mu.is_finite());
}
