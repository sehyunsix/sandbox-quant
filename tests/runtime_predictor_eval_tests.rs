use sandbox_quant::runtime::predictor_eval::{
    observe_predictor_eval_volatility, predictor_eval_scale, PredictorEvalVolState,
};

#[test]
fn scale_uses_min_sigma_before_state_is_ready() {
    let st = PredictorEvalVolState::default();
    assert!((predictor_eval_scale(&st, 0.02) - 0.02).abs() < 1e-12);
}

#[test]
fn observe_marks_state_ready_after_second_price() {
    let mut st = PredictorEvalVolState::default();
    observe_predictor_eval_volatility(&mut st, 100.0, 0.1);
    assert!(!st.ready);
    observe_predictor_eval_volatility(&mut st, 101.0, 0.1);
    assert!(st.ready);
}

#[test]
fn scale_tracks_realized_volatility_floor() {
    let mut st = PredictorEvalVolState::default();
    observe_predictor_eval_volatility(&mut st, 100.0, 0.2);
    observe_predictor_eval_volatility(&mut st, 110.0, 0.2);
    let out = predictor_eval_scale(&st, 1e-8);
    assert!(out > 0.0);
}
