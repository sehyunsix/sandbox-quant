use crate::v1::domain::identifiers::BatchId;
use crate::v1::domain::instrument::Instrument;
use crate::v1::domain::position::Side;
use crate::v1::error::exchange_error::ExchangeError;
use crate::v1::error::execution_error::ExecutionError;
use crate::v1::exchange::facade::ExchangeFacade;
use crate::v1::exchange::types::CloseOrderRequest;
use crate::v1::execution::close_all::CloseAllBatchResult;
use crate::v1::execution::close_symbol::{CloseSubmitResult, CloseSymbolResult};
use crate::v1::execution::command::ExecutionCommand;
use crate::v1::portfolio::store::PortfolioStateStore;

#[derive(Debug, Default)]
pub struct ExecutionService {
    pub last_command: Option<ExecutionCommand>,
}

impl ExecutionService {
    pub fn accept(&mut self, command: ExecutionCommand) {
        self.last_command = Some(command);
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
        let Some(position) = store.snapshot.positions.get(instrument) else {
            return Ok(CloseSymbolResult {
                instrument: instrument.clone(),
                result: CloseSubmitResult::SkippedNoPosition,
            });
        };
        if position.is_flat() {
            return Ok(CloseSymbolResult {
                instrument: instrument.clone(),
                result: CloseSubmitResult::SkippedNoPosition,
            });
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

        exchange.submit_close_order(CloseOrderRequest {
            instrument: instrument.clone(),
            market: position.market,
            side,
            qty,
            reduce_only: position.market == crate::v1::domain::market::Market::Futures,
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
