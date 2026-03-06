use crate::domain::identifiers::BatchId;
use crate::domain::exposure::Exposure;
use crate::domain::instrument::Instrument;
use crate::domain::market::Market;
use crate::error::exchange_error::ExchangeError;
use crate::error::execution_error::ExecutionError;
use crate::exchange::facade::ExchangeFacade;
use crate::exchange::types::CloseOrderRequest;
use crate::execution::close_all::CloseAllBatchResult;
use crate::execution::close_symbol::{CloseSubmitResult, CloseSymbolResult};
use crate::execution::command::{CommandSource, ExecutionCommand};
use crate::execution::futures::planner::FuturesExecutionPlanner;
use crate::execution::planner::ExecutionPlan;
use crate::execution::price_source::PriceSource;
use crate::execution::spot::planner::SpotExecutionPlanner;
use crate::execution::target_translation::exposure_to_notional;
use crate::portfolio::store::PortfolioStateStore;
use crate::storage::event_log::EventLog;
use crate::storage::models::EventRecord;

#[derive(Debug, Default)]
pub struct ExecutionService {
    pub last_command: Option<ExecutionCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionOutcome {
    TargetExposureSubmitted {
        instrument: Instrument,
    },
    CloseSymbol(CloseSymbolResult),
    CloseAll(CloseAllBatchResult),
}

impl ExecutionService {
    fn record(&mut self, command: ExecutionCommand) {
        self.last_command = Some(command);
    }

    pub fn execute<E: ExchangeFacade<Error = ExchangeError>>(
        &mut self,
        exchange: &E,
        store: &PortfolioStateStore,
        price_source: &impl PriceSource,
        command: ExecutionCommand,
    ) -> Result<ExecutionOutcome, ExecutionError> {
        self.record(command.clone());
        match command {
            ExecutionCommand::SetTargetExposure {
                instrument,
                target,
                source: _source,
            } => {
                self.submit_target_exposure(exchange, store, price_source, &instrument, target)?;
                Ok(ExecutionOutcome::TargetExposureSubmitted { instrument })
            }
            ExecutionCommand::CloseSymbol {
                instrument,
                source: _source,
            } => Ok(ExecutionOutcome::CloseSymbol(
                self.close_symbol(exchange, store, &instrument)?,
            )),
            ExecutionCommand::CloseAll { source } => {
                let batch_id = match source {
                    CommandSource::User => BatchId(1),
                    CommandSource::System => BatchId(2),
                };
                Ok(ExecutionOutcome::CloseAll(
                    self.close_all(exchange, store, batch_id),
                ))
            }
        }
    }

    pub fn execute_and_log<E: ExchangeFacade<Error = ExchangeError>>(
        &mut self,
        exchange: &E,
        store: &PortfolioStateStore,
        price_source: &impl PriceSource,
        event_log: &mut EventLog,
        command: ExecutionCommand,
    ) -> Result<ExecutionOutcome, ExecutionError> {
        let outcome = self.execute(exchange, store, price_source, command.clone())?;
        event_log.append(self.build_event_record(command, &outcome));
        Ok(outcome)
    }

    fn build_event_record(
        &self,
        command: ExecutionCommand,
        outcome: &ExecutionOutcome,
    ) -> EventRecord {
        match (command, outcome) {
            (
                ExecutionCommand::SetTargetExposure {
                    instrument,
                    target,
                    source,
                },
                ExecutionOutcome::TargetExposureSubmitted { .. },
            ) => EventRecord {
                kind: "execution.target_exposure.submitted".to_string(),
                payload: format!(
                    "instrument={} target={:.6} source={:?}",
                    instrument.0,
                    target.value(),
                    source
                ),
            },
            (
                ExecutionCommand::CloseSymbol { instrument, source },
                ExecutionOutcome::CloseSymbol(result),
            ) => EventRecord {
                kind: "execution.close_symbol.completed".to_string(),
                payload: format!(
                    "instrument={} source={:?} result={:?}",
                    instrument.0, source, result.result
                ),
            },
            (ExecutionCommand::CloseAll { source }, ExecutionOutcome::CloseAll(result)) => {
                EventRecord {
                    kind: "execution.close_all.completed".to_string(),
                    payload: format!(
                        "batch_id={} source={:?} symbols={}",
                        result.batch_id.0,
                        source,
                        result.results.len()
                    ),
                }
            }
            _ => EventRecord {
                kind: "execution.unknown".to_string(),
                payload: "command/outcome mismatch".to_string(),
            },
        }
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

    pub fn plan_target_exposure(
        &self,
        store: &PortfolioStateStore,
        price_source: &impl PriceSource,
        instrument: &Instrument,
        target: Exposure,
    ) -> Result<ExecutionPlan, ExecutionError> {
        let Some(position) = store.snapshot.positions.get(instrument) else {
            return Err(ExecutionError::NoOpenPosition);
        };
        let current_price = price_source
            .current_price(instrument)
            .ok_or(ExecutionError::MissingPriceContext)?;
        let equity_usdt: f64 = store.snapshot.balances.iter().map(|b| b.total()).sum();
        let target_notional = exposure_to_notional(target, equity_usdt);

        match position.market {
            Market::Spot => SpotExecutionPlanner
                .plan_target_exposure(position, current_price, target_notional.target_usdt),
            Market::Futures => FuturesExecutionPlanner
                .plan_target_exposure(position, current_price, target_notional.target_usdt),
        }
    }

    pub fn submit_target_exposure<E: ExchangeFacade<Error = ExchangeError>>(
        &mut self,
        exchange: &E,
        store: &PortfolioStateStore,
        price_source: &impl PriceSource,
        instrument: &Instrument,
        target: Exposure,
    ) -> Result<(), ExecutionError> {
        let plan = self.plan_target_exposure(store, price_source, instrument, target)?;
        let market = store
            .snapshot
            .positions
            .get(instrument)
            .map(|position| position.market)
            .ok_or(ExecutionError::NoOpenPosition)?;

        exchange.submit_order(CloseOrderRequest {
            instrument: plan.instrument,
            market,
            side: plan.side,
            qty: plan.qty,
            reduce_only: plan.reduce_only,
        })?;
        Ok(())
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
