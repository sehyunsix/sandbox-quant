use chrono::NaiveDate;

use crate::app::bootstrap::BinanceMode;
use crate::app::cli::normalize_instrument_symbol;
use crate::strategy::model::StrategyTemplate;
use crate::terminal::completion::ShellCompletion;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BacktestCommand {
    Run {
        template: StrategyTemplate,
        instrument: String,
        from: NaiveDate,
        to: NaiveDate,
    },
    List,
    ReportLatest,
    ReportShow {
        run_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BacktestShellInput {
    Empty,
    Help,
    Exit,
    Mode(BinanceMode),
    Command(BacktestCommand),
}

pub fn backtest_help_text() -> &'static str {
    "/run <template> <instrument> --from <YYYY-MM-DD> --to <YYYY-MM-DD>\n/list\n/report latest\n/report show <run_id>\n/mode <real|demo>\n/help\n/exit"
}

pub fn parse_backtest_shell_input(line: &str) -> Result<BacktestShellInput, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(BacktestShellInput::Empty);
    }

    let without_prefix = trimmed.strip_prefix('/').unwrap_or(trimmed);
    match without_prefix {
        "help" => return Ok(BacktestShellInput::Help),
        "exit" | "quit" => return Ok(BacktestShellInput::Exit),
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
        return Ok(BacktestShellInput::Mode(mode));
    }

    parse_backtest_command(&args).map(BacktestShellInput::Command)
}

pub fn parse_backtest_command(args: &[String]) -> Result<BacktestCommand, String> {
    match args.first().map(String::as_str) {
        Some("run") => {
            let template = match args.get(1).map(String::as_str) {
                Some("liquidation-breakdown-short") => StrategyTemplate::LiquidationBreakdownShort,
                Some(other) => return Err(format!("unsupported template: {other}")),
                None => {
                    return Err(
                        "usage: run <template> <instrument> --from <YYYY-MM-DD> --to <YYYY-MM-DD>"
                            .to_string(),
                    )
                }
            };
            let instrument = normalize_instrument_symbol(args.get(2).ok_or(
                "usage: run <template> <instrument> --from <YYYY-MM-DD> --to <YYYY-MM-DD>",
            )?);
            let (from, to) = parse_dates(&args[3..])?;
            Ok(BacktestCommand::Run {
                template,
                instrument,
                from,
                to,
            })
        }
        Some("list") => {
            if args.len() == 1 {
                Ok(BacktestCommand::List)
            } else {
                Err("usage: list".to_string())
            }
        }
        Some("report") => match args.get(1).map(String::as_str) {
            Some("latest") if args.len() == 2 => Ok(BacktestCommand::ReportLatest),
            Some("show") if args.len() == 3 => {
                let run_id = args[2]
                    .parse::<i64>()
                    .map_err(|_| format!("invalid run id: {}", args[2]))?;
                Ok(BacktestCommand::ReportShow { run_id })
            }
            _ => Err("usage: report latest | report show <run_id>".to_string()),
        },
        Some(other) => Err(format!("unsupported command: {other}")),
        None => Err("missing backtest command".to_string()),
    }
}

pub fn complete_backtest_input(line: &str) -> Vec<ShellCompletion> {
    let trimmed = line.trim_start();
    let without_prefix = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let trailing_space = without_prefix.ends_with(' ');
    let parts: Vec<&str> = without_prefix.split_whitespace().collect();

    if parts.is_empty() {
        return vec![
            completion("/run", "run a backtest over a date range"),
            completion("/list", "list stored backtest runs"),
            completion("/report", "show stored backtest reports"),
            completion("/mode", "switch dataset mode"),
            completion("/help", "show help"),
            completion("/exit", "exit"),
        ];
    }
    if parts.len() == 1 && !trailing_space {
        return ["/run", "/list", "/report", "/mode", "/help", "/exit"]
            .into_iter()
            .filter(|item| item.trim_start_matches('/').starts_with(parts[0]))
            .map(|item| completion(item, ""))
            .collect();
    }

    match parts.first().copied() {
        Some("mode") => ["real", "demo"]
            .into_iter()
            .filter(|item| item.starts_with(parts.last().copied().unwrap_or_default()))
            .map(|item| completion(&format!("/mode {item}"), "switch backtest mode"))
            .collect(),
        Some("run") if parts.len() <= 2 => StrategyTemplate::all()
            .into_iter()
            .map(|template| {
                completion(
                    &format!("/run {}", template.slug()),
                    "choose a backtest template",
                )
            })
            .collect(),
        Some("report") if parts.len() <= 2 => vec![
            completion("/report latest", "show latest stored run"),
            completion("/report show ", "show a stored run by id"),
        ],
        _ => Vec::new(),
    }
}

fn parse_dates(args: &[String]) -> Result<(NaiveDate, NaiveDate), String> {
    let mut from = None;
    let mut to = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--from" => {
                let value = args.get(index + 1).ok_or("missing value for --from")?;
                from = Some(
                    NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .map_err(|_| format!("invalid date: {value}"))?,
                );
                index += 2;
            }
            "--to" => {
                let value = args.get(index + 1).ok_or("missing value for --to")?;
                to = Some(
                    NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .map_err(|_| format!("invalid date: {value}"))?,
                );
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}")),
        }
    }
    Ok((from.ok_or("missing --from")?, to.ok_or("missing --to")?))
}

fn completion(value: &str, description: &str) -> ShellCompletion {
    ShellCompletion {
        value: value.to_string(),
        description: description.to_string(),
    }
}
