use crate::app::commands::AppCommand;
use crate::app::bootstrap::AppBootstrap;
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
                let outcome = app.execution.execute(
                    &app.exchange,
                    &app.portfolio_store,
                    &app.price_store,
                    command.clone(),
                )?;
                log(
                    &mut app.event_log,
                    "app.execution.completed",
                    json!({
                        "command": format!("{command:?}"),
                        "outcome": format!("{outcome:?}"),
                    }),
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
