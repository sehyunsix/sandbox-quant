use crate::lifecycle::engine::ExitTrigger;

pub struct ExitOrchestrator;

impl ExitOrchestrator {
    pub fn decide(trigger: ExitTrigger) -> &'static str {
        match trigger {
            ExitTrigger::StopLossProtection => "exit.stop_loss_protection",
            ExitTrigger::MaxHoldingTime => "exit.max_holding_time",
            ExitTrigger::RiskDegrade => "exit.risk_degrade",
            ExitTrigger::SignalReversal => "exit.signal_reversal",
            ExitTrigger::EmergencyClose => "exit.emergency_close",
        }
    }
}
