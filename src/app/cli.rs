use crate::app::commands::AppCommand;
use crate::domain::exposure::Exposure;
use crate::domain::instrument::Instrument;
use crate::execution::command::{CommandSource, ExecutionCommand};

pub fn parse_app_command(args: &[String]) -> Result<AppCommand, String> {
    match args.first().map(String::as_str).unwrap_or("refresh") {
        "refresh" => Ok(AppCommand::RefreshAuthoritativeState),
        "close-all" => Ok(AppCommand::Execution(ExecutionCommand::CloseAll {
            source: CommandSource::User,
        })),
        "close-symbol" => {
            let instrument = args
                .get(1)
                .ok_or("usage: close-symbol <instrument>")?
                .clone();
            Ok(AppCommand::Execution(ExecutionCommand::CloseSymbol {
                instrument: Instrument::new(instrument),
                source: CommandSource::User,
            }))
        }
        "set-target-exposure" => {
            let instrument = args
                .get(1)
                .ok_or("usage: set-target-exposure <instrument> <target>")?
                .clone();
            let raw_target = args
                .get(2)
                .ok_or("usage: set-target-exposure <instrument> <target>")?;
            let target = raw_target
                .parse::<f64>()
                .map_err(|_| format!("invalid target exposure: {raw_target}"))?;
            let exposure = Exposure::new(target).ok_or(format!(
                "target exposure out of range: {target}. expected -1.0..=1.0"
            ))?;
            Ok(AppCommand::Execution(
                ExecutionCommand::SetTargetExposure {
                    instrument: Instrument::new(instrument),
                    target: exposure,
                    source: CommandSource::User,
                },
            ))
        }
        other => Err(format!(
            "unsupported command: {other}. supported commands: refresh, close-all, close-symbol, set-target-exposure"
        )),
    }
}
