use crate::v1::domain::position::{PositionSnapshot, Side};
use crate::v1::error::execution_error::ExecutionError;
use crate::v1::execution::planner::ExecutionPlan;

#[derive(Debug, Default)]
pub struct SpotExecutionPlanner;

impl SpotExecutionPlanner {
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
            reduce_only: false,
        })
    }
}
