use crate::app::commands::AppCommand;
use crate::domain::exposure::Exposure;
use crate::domain::instrument::Instrument;
use crate::execution::command::{CommandSource, ExecutionCommand};

#[derive(Debug, Clone, PartialEq)]
pub enum ShellInput {
    Empty,
    Help,
    Exit,
    Command(AppCommand),
}

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

pub fn parse_shell_input(line: &str) -> Result<ShellInput, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(ShellInput::Empty);
    }

    let without_prefix = trimmed.strip_prefix('/').unwrap_or(trimmed);
    match without_prefix {
        "help" => return Ok(ShellInput::Help),
        "exit" | "quit" => return Ok(ShellInput::Exit),
        _ => {}
    }

    let args: Vec<String> = without_prefix
        .split_whitespace()
        .map(str::to_string)
        .collect();
    parse_app_command(&args).map(ShellInput::Command)
}

pub fn shell_help_text() -> &'static str {
    "/refresh\n/close-all\n/close-symbol <instrument>\n/set-target-exposure <instrument> <target>\n/help\n/exit"
}
