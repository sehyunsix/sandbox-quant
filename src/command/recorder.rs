use crate::app::bootstrap::BinanceMode;
use crate::app::cli::normalize_instrument_symbol;
use crate::terminal::completion::ShellCompletion;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecorderCommand {
    Start { symbols: Vec<String> },
    Status,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecorderShellInput {
    Empty,
    Help,
    Exit,
    Mode(BinanceMode),
    Command(RecorderCommand),
}

pub fn recorder_help_text() -> &'static str {
    "/start [symbols...]\n/status\n/stop\n/mode <real|demo>\n/help\n/exit"
}

pub fn parse_recorder_shell_input(line: &str) -> Result<RecorderShellInput, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(RecorderShellInput::Empty);
    }

    let without_prefix = trimmed.strip_prefix('/').unwrap_or(trimmed);
    match without_prefix {
        "help" => return Ok(RecorderShellInput::Help),
        "exit" | "quit" => return Ok(RecorderShellInput::Exit),
        _ => {}
    }

    let args: Vec<String> = without_prefix
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if args.first().map(String::as_str) == Some("mode") {
        let raw_mode = args.get(1).ok_or("usage: /mode <real|demo>")?;
        let mode = match raw_mode.as_str() {
            "real" => BinanceMode::Real,
            "demo" => BinanceMode::Demo,
            _ => return Err(format!("unsupported mode: {raw_mode}")),
        };
        return Ok(RecorderShellInput::Mode(mode));
    }

    parse_recorder_command(&args).map(RecorderShellInput::Command)
}

pub fn parse_recorder_command(args: &[String]) -> Result<RecorderCommand, String> {
    match args.first().map(String::as_str) {
        Some("start") => Ok(RecorderCommand::Start {
            symbols: args[1..]
                .iter()
                .map(|raw| normalize_instrument_symbol(raw))
                .collect(),
        }),
        Some("status") => {
            if args.len() > 1 {
                Err("usage: /status".to_string())
            } else {
                Ok(RecorderCommand::Status)
            }
        }
        Some("stop") => {
            if args.len() > 1 {
                Err("usage: /stop".to_string())
            } else {
                Ok(RecorderCommand::Stop)
            }
        }
        Some(other) => Err(format!("unsupported command: {other}")),
        None => Err("missing recorder command".to_string()),
    }
}

pub fn complete_recorder_input(line: &str) -> Vec<ShellCompletion> {
    let trimmed = line.trim_start();
    let without_prefix = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let trailing_space = without_prefix.ends_with(' ');
    let parts: Vec<&str> = without_prefix.split_whitespace().collect();

    if parts.is_empty() {
        return vec![
            completion("/start", "start recorder with optional symbols"),
            completion("/status", "show recorder status"),
            completion("/stop", "stop recorder"),
            completion("/mode", "switch mode"),
            completion("/help", "show help"),
            completion("/exit", "exit"),
        ];
    }

    if parts.len() == 1 && !trailing_space {
        return ["/start", "/status", "/stop", "/mode", "/help", "/exit"]
            .into_iter()
            .filter(|item| item.trim_start_matches('/').starts_with(parts[0]))
            .map(|item| completion(item, ""))
            .collect();
    }

    match parts.first().copied() {
        Some("mode") => ["real", "demo"]
            .into_iter()
            .filter(|item| item.starts_with(parts.last().copied().unwrap_or_default()))
            .map(|item| completion(&format!("/mode {item}"), "switch recorder mode"))
            .collect(),
        _ => Vec::new(),
    }
}

fn completion(value: &str, description: &str) -> ShellCompletion {
    ShellCompletion {
        value: value.to_string(),
        description: description.to_string(),
    }
}
