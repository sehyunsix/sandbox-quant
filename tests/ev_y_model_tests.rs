use sandbox_quant::ev::{EwmaYModel, EwmaYModelConfig};
use sandbox_quant::model::signal::Signal;

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

#[test]
fn ewma_y_model_conditions_on_source_and_signal_side() {
    let mut m = EwmaYModel::new(EwmaYModelConfig {
        alpha_mean: 0.5,
        alpha_var: 0.5,
        min_sigma: 0.0001,
    });
    m.observe_price("BTCUSDT", 100.0);
    m.observe_price("BTCUSDT", 100.0);

    m.observe_signal_price("BTCUSDT", "c01", &Signal::Buy, 100.0);
    m.observe_signal_price("BTCUSDT", "c01", &Signal::Buy, 103.0);

    m.observe_signal_price("BTCUSDT", "c01", &Signal::Sell, 103.0);
    m.observe_signal_price("BTCUSDT", "c01", &Signal::Sell, 100.0);

    let y_buy = m.estimate_for_signal("BTCUSDT", "c01", &Signal::Buy, 0.0, 0.01);
    let y_sell = m.estimate_for_signal("BTCUSDT", "c01", &Signal::Sell, 0.0, 0.01);
    assert!(y_buy.mu > y_sell.mu);

    let y_other_tag = m.estimate_for_signal("BTCUSDT", "c99", &Signal::Buy, 0.0, 0.01);
    assert!(y_other_tag.mu.abs() <= y_buy.mu.abs());
}
