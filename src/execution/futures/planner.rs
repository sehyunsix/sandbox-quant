use crate::domain::position::{PositionSnapshot, Side};
use crate::error::execution_error::ExecutionError;
use crate::execution::planner::ExecutionPlan;

#[derive(Debug, Default)]
pub struct FuturesExecutionPlanner;

impl FuturesExecutionPlanner {
    pub fn plan_close(
        &self,
        position: &PositionSnapshot,
    ) -> Result<ExecutionPlan, ExecutionError> {
        if position.is_flat() {
            return Err(ExecutionError::NoOpenPosition);
        }

        let side = match position.side() {
            Some(Side::Buy) => Side::Sell,
            Some(Side::Sell) => Side::Buy,
            None => return Err(ExecutionError::NoOpenPosition),
        };
        let qty = position.abs_qty();
        if qty <= f64::EPSILON {
            return Err(ExecutionError::CloseQtyTooSmall);
        }

        Ok(ExecutionPlan {
            instrument: position.instrument.clone(),
            side,
            qty,
            reduce_only: true,
        })
    }
}
