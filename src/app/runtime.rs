use crate::app::commands::AppCommand;
use crate::app::bootstrap::AppBootstrap;
use crate::execution::command::ExecutionCommand;
use crate::storage::event_log::log;
use serde_json::json;

#[derive(Debug, Default)]
pub struct AppRuntime {
    pub last_command: Option<AppCommand>,
}

impl AppRuntime {
    pub fn record_command(&mut self, command: AppCommand) {
        self.last_command = Some(command);
    }

    pub fn run<E: crate::exchange::facade::ExchangeFacade<Error = crate::error::exchange_error::ExchangeError>>(
        &mut self,
        app: &mut AppBootstrap<E>,
        command: AppCommand,
    ) -> Result<(), crate::error::app_error::AppError> {
        self.record_command(command.clone());

        match command {
            AppCommand::Execution(command) => {
                if let ExecutionCommand::SetTargetExposure { instrument, .. } = &command {
                    if let Some(position) = app.portfolio_store.snapshot.positions.get(instrument) {
                        let price = app.market_data.refresh_price(
                            &app.exchange,
                            &mut app.price_store,
                            instrument.clone(),
                            position.market,
                        )?;
                        log(
                            &mut app.event_log,
                            "app.market_data.price_refreshed",
                            json!({
                                "instrument": instrument.0,
                                "market": format!("{:?}", position.market),
                                "price": price,
                            }),
                        );
                    }
                }
                let outcome = app.execution.execute(
                    &app.exchange,
                    &app.portfolio_store,
                    &app.price_store,
                    command.clone(),
                )?;
                log(
                    &mut app.event_log,
                    "app.execution.completed",
                    execution_payload(&command, &outcome),
                );
            }
            AppCommand::RefreshAuthoritativeState => {
                let report = app
                    .portfolio_sync
                    .refresh_authoritative(&app.exchange, &mut app.portfolio_store)?;
                log(
                    &mut app.event_log,
                    "app.portfolio.refreshed",
                    json!({
                        "positions": report.positions,
                        "open_order_groups": report.open_order_groups,
                        "balances": report.balances,
                    }),
                );
            }
        }

        Ok(())
    }
}

fn execution_payload(
    command: &ExecutionCommand,
    outcome: &crate::execution::service::ExecutionOutcome,
) -> serde_json::Value {
    match (command, outcome) {
        (
            ExecutionCommand::SetTargetExposure {
                instrument, target, ..
            },
            crate::execution::service::ExecutionOutcome::TargetExposureSubmitted { .. },
        ) => json!({
            "command_kind": "set_target_exposure",
            "instrument": instrument.0,
            "target": target.value(),
            "outcome_kind": "submitted",
        }),
        (
            ExecutionCommand::CloseSymbol { instrument, .. },
            crate::execution::service::ExecutionOutcome::CloseSymbol(result),
        ) => json!({
            "command_kind": "close_symbol",
            "instrument": instrument.0,
            "outcome_kind": format!("{:?}", result.result),
        }),
        (
            ExecutionCommand::CloseAll { .. },
            crate::execution::service::ExecutionOutcome::CloseAll(result),
        ) => {
            let submitted = result
                .results
                .iter()
                .filter(|item| matches!(item.result, crate::execution::close_symbol::CloseSubmitResult::Submitted))
                .count();
            let skipped = result
                .results
                .iter()
                .filter(|item| matches!(item.result, crate::execution::close_symbol::CloseSubmitResult::SkippedNoPosition))
                .count();
            let rejected = result
                .results
                .iter()
                .filter(|item| matches!(item.result, crate::execution::close_symbol::CloseSubmitResult::Rejected))
                .count();
            json!({
                "command_kind": "close_all",
                "batch_id": result.batch_id.0,
                "submitted": submitted,
                "skipped": skipped,
                "rejected": rejected,
                "outcome_kind": "batch_completed",
            })
        }
        _ => json!({
            "command_kind": "unknown",
            "outcome_kind": "unknown",
        }),
    }
}
