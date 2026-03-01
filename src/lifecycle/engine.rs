use std::collections::HashMap;

use crate::ev::EntryExpectancySnapshot;

#[derive(Debug, Clone)]
pub struct PositionLifecycleState {
    pub position_id: String,
    pub source_tag: String,
    pub instrument: String,
    pub opened_at_ms: u64,
    pub entry_price: f64,
    pub qty: f64,
    pub mfe_usdt: f64,
    pub mae_usdt: f64,
    pub expected_holding_ms: u64,
    pub stop_loss_order_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitTrigger {
    StopLossProtection,
    MaxHoldingTime,
    RiskDegrade,
    SignalReversal,
    EmergencyClose,
}

#[derive(Default)]
pub struct PositionLifecycleEngine {
    states: HashMap<String, PositionLifecycleState>,
}

impl PositionLifecycleEngine {
    pub fn on_entry_filled(
        &mut self,
        instrument: &str,
        source_tag: &str,
        entry_price: f64,
        qty: f64,
        expectancy: &EntryExpectancySnapshot,
        now_ms: u64,
    ) -> String {
        let position_id = format!("pos-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        let state = PositionLifecycleState {
            position_id: position_id.clone(),
            source_tag: source_tag.to_ascii_lowercase(),
            instrument: instrument.to_string(),
            opened_at_ms: now_ms,
            entry_price,
            qty,
            mfe_usdt: 0.0,
            mae_usdt: 0.0,
            expected_holding_ms: expectancy.expected_holding_ms.max(1),
            stop_loss_order_id: None,
        };
        self.states.insert(instrument.to_string(), state);
        position_id
    }

    pub fn on_tick(
        &mut self,
        instrument: &str,
        mark_price: f64,
        now_ms: u64,
    ) -> Option<ExitTrigger> {
        let state = self.states.get_mut(instrument)?;
        let unrealized = (mark_price - state.entry_price) * state.qty;
        if unrealized > state.mfe_usdt {
            state.mfe_usdt = unrealized;
        }
        if unrealized < state.mae_usdt {
            state.mae_usdt = unrealized;
        }
        let held_ms = now_ms.saturating_sub(state.opened_at_ms);
        if held_ms >= state.expected_holding_ms {
            return Some(ExitTrigger::MaxHoldingTime);
        }
        None
    }

    pub fn set_stop_loss_order_id(&mut self, instrument: &str, order_id: Option<String>) {
        if let Some(state) = self.states.get_mut(instrument) {
            state.stop_loss_order_id = order_id;
        }
    }

    pub fn has_valid_stop_loss(&self, instrument: &str) -> bool {
        self.states
            .get(instrument)
            .and_then(|s| s.stop_loss_order_id.as_ref())
            .is_some()
    }

    pub fn on_position_closed(&mut self, instrument: &str) -> Option<PositionLifecycleState> {
        self.states.remove(instrument)
    }
}
