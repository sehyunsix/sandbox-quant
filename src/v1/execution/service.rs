use crate::v1::domain::identifiers::BatchId;
use crate::v1::domain::instrument::Instrument;
use crate::v1::domain::market::Market;
use crate::v1::error::exchange_error::ExchangeError;
use crate::v1::error::execution_error::ExecutionError;
use crate::v1::exchange::facade::ExchangeFacade;
use crate::v1::exchange::types::CloseOrderRequest;
use crate::v1::execution::close_all::CloseAllBatchResult;
use crate::v1::execution::close_symbol::{CloseSubmitResult, CloseSymbolResult};
use crate::v1::execution::command::ExecutionCommand;
use crate::v1::execution::futures::planner::FuturesExecutionPlanner;
use crate::v1::execution::planner::ExecutionPlan;
use crate::v1::execution::spot::planner::SpotExecutionPlanner;
use crate::v1::portfolio::store::PortfolioStateStore;

#[derive(Debug, Default)]
pub struct ExecutionService {
    pub last_command: Option<ExecutionCommand>,
}

impl ExecutionService {
    pub fn accept(&mut self, command: ExecutionCommand) {
        self.last_command = Some(command);
    }

    fn plan_close(
        &self,
        store: &PortfolioStateStore,
        instrument: &Instrument,
    ) -> Result<ExecutionPlan, ExecutionError> {
        let Some(position) = store.snapshot.positions.get(instrument) else {
            return Err(ExecutionError::NoOpenPosition);
        };

        match position.market {
            Market::Spot => SpotExecutionPlanner.plan_close(position),
            Market::Futures => FuturesExecutionPlanner.plan_close(position),
        }
    }

    /// Submits a close order for the current authoritative position snapshot.
    ///
    /// Example:
    /// - current signed qty = `-0.3`
    /// - generated close side = `Buy`
    /// - generated close qty = `0.3`
    pub fn close_symbol<E: ExchangeFacade<Error = ExchangeError>>(
        &mut self,
        exchange: &E,
        store: &PortfolioStateStore,
        instrument: &Instrument,
    ) -> Result<CloseSymbolResult, ExecutionError> {
        let plan = match self.plan_close(store, instrument) {
            Ok(plan) => plan,
            Err(ExecutionError::NoOpenPosition) => {
                return Ok(CloseSymbolResult {
                    instrument: instrument.clone(),
                    result: CloseSubmitResult::SkippedNoPosition,
                });
            }
            Err(error) => return Err(error),
        };

        if plan.qty <= f64::EPSILON {
            return Ok(CloseSymbolResult {
                instrument: instrument.clone(),
                result: CloseSubmitResult::SkippedNoPosition,
            });
        }

        exchange.submit_close_order(CloseOrderRequest {
            instrument: plan.instrument.clone(),
            market: store
                .snapshot
                .positions
                .get(instrument)
                .map(|position| position.market)
                .ok_or(ExecutionError::NoOpenPosition)?,
            side: plan.side,
            qty: plan.qty,
            reduce_only: plan.reduce_only,
        })?;

        Ok(CloseSymbolResult {
            instrument: instrument.clone(),
            result: CloseSubmitResult::Submitted,
        })
    }

    pub fn close_all<E: ExchangeFacade<Error = ExchangeError>>(
        &mut self,
        exchange: &E,
        store: &PortfolioStateStore,
        batch_id: BatchId,
    ) -> CloseAllBatchResult {
        let mut results = Vec::new();
        for instrument in store.snapshot.positions.keys() {
            let result = match self.close_symbol(exchange, store, instrument) {
                Ok(result) => result,
                Err(_) => CloseSymbolResult {
                    instrument: instrument.clone(),
                    result: CloseSubmitResult::Rejected,
                },
            };
            results.push(result);
        }
        CloseAllBatchResult { batch_id, results }
    }
}
