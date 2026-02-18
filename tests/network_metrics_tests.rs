use sandbox_quant::ui::network_metrics::{
    classify_health, count_since, percentile, ratio_pct, NetworkHealth,
};

#[test]
fn count_since_respects_window() {
    let now_ms = 100_000;
    let events = vec![39_000, 40_000, 70_000, 99_900];
    assert_eq!(count_since(&events, now_ms, 60_000), 3);
    assert_eq!(count_since(&events, now_ms, 30_000), 2);
    assert_eq!(count_since(&events, now_ms, 1_000), 1);
}

#[test]
fn ratio_pct_handles_zero_denom() {
    assert!((ratio_pct(0, 0) - 0.0).abs() < f64::EPSILON);
    assert!((ratio_pct(5, 10) - 50.0).abs() < f64::EPSILON);
}

#[test]
fn percentile_returns_expected_values() {
    let samples = vec![100, 200, 400, 800, 1600];
    assert_eq!(percentile(&samples, 50), Some(400));
    assert_eq!(percentile(&samples, 95), Some(1600));
    assert_eq!(percentile(&samples, 99), Some(1600));
    assert_eq!(percentile(&[], 95), None);
}

#[test]
fn classify_health_uses_rfc_thresholds() {
    assert_eq!(
        classify_health(false, 0.0, 0.0, Some(100), Some(100)),
        NetworkHealth::Crit
    );
    assert_eq!(
        classify_health(true, 2.0, 0.0, Some(100), Some(100)),
        NetworkHealth::Warn
    );
    assert_eq!(
        classify_health(true, 0.2, 6.0, Some(100), Some(100)),
        NetworkHealth::Crit
    );
    assert_eq!(
        classify_health(true, 0.2, 0.0, Some(2_500), Some(100)),
        NetworkHealth::Warn
    );
    assert_eq!(
        classify_health(true, 0.2, 0.0, Some(100), Some(9_000)),
        NetworkHealth::Crit
    );
    assert_eq!(
        classify_health(true, 0.2, 0.0, Some(100), Some(200)),
        NetworkHealth::Ok
    );
}
