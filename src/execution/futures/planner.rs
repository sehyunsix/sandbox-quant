use crate::domain::position::{PositionSnapshot, Side};
use crate::error::execution_error::ExecutionError;
use crate::execution::planner::ExecutionPlan;

#[derive(Debug, Default)]
pub struct FuturesExecutionPlanner;

impl FuturesExecutionPlanner {
    pub fn plan_close(&self, position: &PositionSnapshot) -> Result<ExecutionPlan, ExecutionError> {
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

    pub fn plan_target_exposure(
        &self,
        position: &PositionSnapshot,
        current_price: f64,
        target_notional_usdt: f64,
    ) -> Result<ExecutionPlan, ExecutionError> {
        if current_price <= f64::EPSILON {
            return Err(ExecutionError::MissingPriceContext);
        }

        let current_qty = position.signed_qty;
        let target_qty = target_notional_usdt / current_price;
        let delta_qty = target_qty - current_qty;
        let side = if delta_qty >= 0.0 {
            Side::Buy
        } else {
            Side::Sell
        };

        Ok(ExecutionPlan {
            instrument: position.instrument.clone(),
            side,
            qty: delta_qty.abs(),
            reduce_only: false,
        })
    }
}
