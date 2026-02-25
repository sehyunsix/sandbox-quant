use sandbox_quant::ev::{EwmaYModel, EwmaYModelConfig};

#[test]
fn ewma_y_model_updates_mu_and_sigma_from_prices() {
    let mut m = EwmaYModel::new(EwmaYModelConfig {
        alpha_mean: 0.5,
        alpha_var: 0.5,
        min_sigma: 0.0001,
    });
    m.observe_price("BTCUSDT", 100.0);
    m.observe_price("BTCUSDT", 101.0);
    m.observe_price("BTCUSDT", 102.0);
    let y = m.estimate("BTCUSDT", 0.0, 0.01);
    assert!(y.mu > 0.0);
    assert!(y.sigma >= 0.0001);
}

#[test]
fn ewma_y_model_uses_fallback_before_samples() {
    let m = EwmaYModel::new(EwmaYModelConfig::default());
    let y = m.estimate("ETHUSDT", 0.002, 0.015);
    assert!((y.mu - 0.002).abs() < f64::EPSILON);
    assert!((y.sigma - 0.015).abs() < f64::EPSILON);
}
