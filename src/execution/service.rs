use crate::domain::exposure::Exposure;
use crate::domain::identifiers::BatchId;
use crate::domain::instrument::Instrument;
use crate::domain::market::Market;
use crate::domain::order_type::OrderType;
use crate::domain::position::PositionSnapshot;
use crate::error::exchange_error::ExchangeError;
use crate::error::execution_error::ExecutionError;
use crate::exchange::facade::ExchangeFacade;
use crate::exchange::symbol_rules::SymbolRules;
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

#[derive(Debug, Clone, PartialEq)]
struct NormalizedOrderQty {
    qty: f64,
    qty_text: String,
}

#[derive(Debug, Default)]
pub struct ExecutionService {
    pub last_command: Option<ExecutionCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionOutcome {
    TargetExposureSubmitted { instrument: Instrument },
    TargetExposureAlreadyAtTarget { instrument: Instrument },
    OptionOrderSubmitted { instrument: Instrument },
    CloseSymbol(CloseSymbolResult),
    CloseAll(CloseAllBatchResult),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetExposureSubmitResult {
    Submitted,
    AlreadyAtTarget,
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
                order_type,
                source: _source,
            } => match self.submit_target_exposure(
                exchange,
                store,
                price_source,
                &instrument,
                target,
                order_type,
            )? {
                TargetExposureSubmitResult::Submitted => {
                    Ok(ExecutionOutcome::TargetExposureSubmitted { instrument })
                }
                TargetExposureSubmitResult::AlreadyAtTarget => {
                    Ok(ExecutionOutcome::TargetExposureAlreadyAtTarget { instrument })
                }
            },
            ExecutionCommand::SubmitOptionOrder {
                instrument,
                side,
                qty,
                order_type,
                source: _source,
            } => {
                self.submit_option_order(exchange, &instrument, side, qty, order_type)?;
                Ok(ExecutionOutcome::OptionOrderSubmitted { instrument })
            }
            ExecutionCommand::CloseSymbol {
                instrument,
                source: _source,
            } => Ok(ExecutionOutcome::CloseSymbol(self.close_symbol(
                exchange,
                store,
                &instrument,
            )?)),
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
            Market::Options => Err(ExecutionError::SubmitFailed(
                ExchangeError::UnsupportedMarketOperation,
            )),
        }
    }

    pub fn plan_target_exposure<E: ExchangeFacade<Error = ExchangeError>>(
        &self,
        exchange: &E,
        store: &PortfolioStateStore,
        price_source: &impl PriceSource,
        instrument: &Instrument,
        target: Exposure,
        _order_type: OrderType,
    ) -> Result<ExecutionPlan, ExecutionError> {
        let (resolved_instrument, market, current_qty) =
            self.resolve_target_context(exchange, store, instrument)?;
        let current_price = price_source
            .current_price(&resolved_instrument)
            .or_else(|| exchange.load_last_price(&resolved_instrument, market).ok())
            .ok_or(ExecutionError::MissingPriceContext)?;
        let equity_usdt: f64 = store.snapshot.balances.iter().map(|b| b.total()).sum();
        let target_notional = exposure_to_notional(target, equity_usdt);
        let synthetic_position = PositionSnapshot {
            instrument: resolved_instrument.clone(),
            market,
            signed_qty: current_qty,
            entry_price: None,
        };

        match market {
            Market::Spot => SpotExecutionPlanner.plan_target_exposure(
                &synthetic_position,
                current_price,
                target_notional.target_usdt,
            ),
            Market::Futures => FuturesExecutionPlanner.plan_target_exposure(
                &synthetic_position,
                current_price,
                target_notional.target_usdt,
            ),
            Market::Options => Err(ExecutionError::SubmitFailed(
                ExchangeError::UnsupportedMarketOperation,
            )),
        }
    }

    pub fn submit_target_exposure<E: ExchangeFacade<Error = ExchangeError>>(
        &mut self,
        exchange: &E,
        store: &PortfolioStateStore,
        price_source: &impl PriceSource,
        instrument: &Instrument,
        target: Exposure,
        order_type: OrderType,
    ) -> Result<TargetExposureSubmitResult, ExecutionError> {
        let (resolved_instrument, market, current_qty) =
            self.resolve_target_context(exchange, store, instrument)?;
        let current_price = price_source
            .current_price(&resolved_instrument)
            .or_else(|| exchange.load_last_price(&resolved_instrument, market).ok())
            .ok_or(ExecutionError::MissingPriceContext)?;
        let equity_usdt: f64 = store.snapshot.balances.iter().map(|b| b.total()).sum();
        let target_notional = exposure_to_notional(target, equity_usdt);
        let synthetic_position = PositionSnapshot {
            instrument: resolved_instrument.clone(),
            market,
            signed_qty: current_qty,
            entry_price: None,
        };
        let plan = match market {
            Market::Spot => SpotExecutionPlanner.plan_target_exposure(
                &synthetic_position,
                current_price,
                target_notional.target_usdt,
            ),
            Market::Futures => FuturesExecutionPlanner.plan_target_exposure(
                &synthetic_position,
                current_price,
                target_notional.target_usdt,
            ),
            Market::Options => {
                return Err(ExecutionError::SubmitFailed(
                    ExchangeError::UnsupportedMarketOperation,
                ))
            }
        }?;
        let qty = match self.normalize_order_qty(
            exchange,
            &plan.instrument,
            market,
            plan.qty,
            target.value(),
            equity_usdt,
            current_price,
            target_notional.target_usdt,
        ) {
            Ok(qty) => qty,
            Err(ExecutionError::OrderQtyTooSmall {
                raw_qty,
                normalized_qty,
                ..
            }) if current_qty.abs() > f64::EPSILON
                && raw_qty > f64::EPSILON
                && normalized_qty <= f64::EPSILON =>
            {
                return Ok(TargetExposureSubmitResult::AlreadyAtTarget);
            }
            Err(error) => return Err(error),
        };

        exchange.submit_order(CloseOrderRequest {
            instrument: plan.instrument,
            market,
            side: plan.side,
            qty: qty.qty,
            qty_text: qty.qty_text,
            order_type,
            reduce_only: plan.reduce_only,
        })?;
        Ok(TargetExposureSubmitResult::Submitted)
    }

    fn resolve_target_context<E: ExchangeFacade<Error = ExchangeError>>(
        &self,
        exchange: &E,
        store: &PortfolioStateStore,
        instrument: &Instrument,
    ) -> Result<(Instrument, Market, f64), ExecutionError> {
        if let Some(position) = store.snapshot.positions.get(instrument) {
            return Ok((instrument.clone(), position.market, position.signed_qty));
        }

        if exchange
            .load_symbol_rules(instrument, Market::Futures)
            .is_ok()
        {
            return Ok((instrument.clone(), Market::Futures, 0.0));
        }

        if exchange.load_symbol_rules(instrument, Market::Spot).is_ok() {
            return Ok((instrument.clone(), Market::Spot, 0.0));
        }

        Err(ExecutionError::UnknownInstrument(instrument.0.clone()))
    }

    pub fn submit_option_order<E: ExchangeFacade<Error = ExchangeError>>(
        &mut self,
        exchange: &E,
        instrument: &Instrument,
        side: crate::domain::position::Side,
        qty: f64,
        order_type: OrderType,
    ) -> Result<(), ExecutionError> {
        let normalized_qty =
            self.normalize_direct_order_qty(exchange, instrument, Market::Options, qty)?;
        exchange.submit_order(CloseOrderRequest {
            instrument: instrument.clone(),
            market: Market::Options,
            side,
            qty: normalized_qty.qty,
            qty_text: normalized_qty.qty_text,
            order_type,
            reduce_only: false,
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
        let market = store
            .snapshot
            .positions
            .get(instrument)
            .map(|position| position.market)
            .ok_or(ExecutionError::NoOpenPosition)?;
        let qty = self.normalize_order_qty(
            exchange,
            &plan.instrument,
            market,
            plan.qty,
            0.0,
            0.0,
            0.0,
            0.0,
        )?;
        exchange.submit_close_order(CloseOrderRequest {
            instrument: plan.instrument.clone(),
            market,
            side: plan.side,
            qty: qty.qty,
            qty_text: qty.qty_text,
            order_type: OrderType::Market,
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

    fn normalize_order_qty<E: ExchangeFacade<Error = ExchangeError>>(
        &self,
        exchange: &E,
        instrument: &Instrument,
        market: Market,
        raw_qty: f64,
        target_exposure: f64,
        equity_usdt: f64,
        current_price: f64,
        target_notional_usdt: f64,
    ) -> Result<NormalizedOrderQty, ExecutionError> {
        let rules = exchange.load_symbol_rules(instrument, market)?;
        let normalized_qty = floor_to_step(raw_qty, rules.step_size);
        let validated_qty = validate_normalized_qty(
            instrument,
            market,
            raw_qty,
            normalized_qty,
            rules,
            target_exposure,
            equity_usdt,
            current_price,
            target_notional_usdt,
        )?;

        Ok(NormalizedOrderQty {
            qty: validated_qty,
            qty_text: format_qty_to_step(validated_qty, rules.step_size),
        })
    }

    fn normalize_direct_order_qty<E: ExchangeFacade<Error = ExchangeError>>(
        &self,
        exchange: &E,
        instrument: &Instrument,
        market: Market,
        raw_qty: f64,
    ) -> Result<NormalizedOrderQty, ExecutionError> {
        let rules = exchange.load_symbol_rules(instrument, market)?;
        let normalized_qty = floor_to_step(raw_qty, rules.step_size);
        let validated_qty = validate_normalized_qty(
            instrument,
            market,
            raw_qty,
            normalized_qty,
            rules,
            0.0,
            0.0,
            0.0,
            0.0,
        )?;

        Ok(NormalizedOrderQty {
            qty: validated_qty,
            qty_text: format_qty_to_step(validated_qty, rules.step_size),
        })
    }
}

fn floor_to_step(raw_qty: f64, step_size: f64) -> f64 {
    if raw_qty <= f64::EPSILON || step_size <= f64::EPSILON {
        return 0.0;
    }
    (raw_qty / step_size).floor() * step_size
}

fn format_qty_to_step(qty: f64, step_size: f64) -> String {
    let precision = step_precision(step_size);
    format!("{qty:.precision$}")
}

fn step_precision(step_size: f64) -> usize {
    if step_size <= f64::EPSILON {
        return 0;
    }

    let mut normalized = step_size.abs();
    let mut precision = 0usize;
    while precision < 12 && (normalized.round() - normalized).abs() > 1e-9 {
        normalized *= 10.0;
        precision += 1;
    }
    precision
}

fn validate_normalized_qty(
    instrument: &Instrument,
    market: Market,
    raw_qty: f64,
    normalized_qty: f64,
    rules: SymbolRules,
    target_exposure: f64,
    equity_usdt: f64,
    current_price: f64,
    target_notional_usdt: f64,
) -> Result<f64, ExecutionError> {
    if normalized_qty <= f64::EPSILON || normalized_qty < rules.min_qty {
        return Err(ExecutionError::OrderQtyTooSmall {
            instrument: instrument.0.clone(),
            market: format!("{market:?}"),
            target_exposure,
            equity_usdt,
            current_price,
            target_notional_usdt,
            raw_qty,
            normalized_qty,
            min_qty: rules.min_qty,
            step_size: rules.step_size,
        });
    }

    Ok(normalized_qty)
}
