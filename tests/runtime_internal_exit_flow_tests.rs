use sandbox_quant::order_manager::OrderUpdate;
use sandbox_quant::runtime::internal_exit_flow::{advance_close_all_job, classify_close_update};
use std::collections::HashMap;

fn soft_skip(code: &str) -> bool {
    matches!(code, "risk.qty_too_small")
}

#[test]
fn increments_completed_and_keeps_failed_for_soft_skip() {
    let mut jobs = HashMap::new();
    jobs.insert(7, (2, 0, 0));

    let out = advance_close_all_job(
        &mut jobs,
        7,
        Some("close failed"),
        Some("risk.qty_too_small"),
        soft_skip,
    )
    .expect("progress should be emitted");

    assert_eq!(out.total, 2);
    assert_eq!(out.completed, 1);
    assert_eq!(out.failed, 0);
    assert!(!out.finished);
}

#[test]
fn increments_failed_for_hard_failure_and_finishes_job() {
    let mut jobs = HashMap::new();
    jobs.insert(9, (1, 0, 0));

    let out = advance_close_all_job(
        &mut jobs,
        9,
        Some("broker down"),
        Some("broker.submit_failed"),
        soft_skip,
    )
    .expect("progress should be emitted");

    assert_eq!(out.total, 1);
    assert_eq!(out.completed, 1);
    assert_eq!(out.failed, 1);
    assert!(out.finished);
    assert!(!jobs.contains_key(&9));
}

#[test]
fn returns_none_for_unknown_job() {
    let mut jobs = HashMap::new();
    let out = advance_close_all_job(
        &mut jobs,
        404,
        Some("x"),
        Some("broker.submit_failed"),
        soft_skip,
    );
    assert!(out.is_none());
}

#[test]
fn classify_close_update_extracts_rejection_metadata() {
    let update = OrderUpdate::Rejected {
        intent_id: "i".to_string(),
        client_order_id: "c".to_string(),
        reason_code: "risk.qty_too_small".to_string(),
        reason: "qty too small".to_string(),
    };
    let out = classify_close_update(&update);
    assert_eq!(out.close_reject_code.as_deref(), Some("risk.qty_too_small"));
    assert_eq!(out.close_failed_reason.as_deref(), Some("qty too small"));
}
