use sandbox_quant::lifecycle::{ExitOrchestrator, ExitTrigger};

#[test]
fn trigger_maps_to_reason_code() {
    assert_eq!(
        ExitOrchestrator::decide(ExitTrigger::StopLossProtection),
        "exit.stop_loss_protection"
    );
    assert_eq!(
        ExitOrchestrator::decide(ExitTrigger::MaxHoldingTime),
        "exit.max_holding_time"
    );
    assert_eq!(
        ExitOrchestrator::decide(ExitTrigger::EmergencyClose),
        "exit.emergency_close"
    );
}
