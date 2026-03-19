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
    Sweep {
        templates: Vec<StrategyTemplate>,
        instruments: Vec<String>,
        windows: Vec<BacktestSweepWindow>,
        output_dir: String,
    },
    List,
    ReportLatest,
    ReportShow {
        run_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktestSweepWindow {
    pub from: NaiveDate,
    pub to: NaiveDate,
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
    "/run <template> <instrument> --from <YYYY-MM-DD> --to <YYYY-MM-DD>\n/sweep --templates <csv> --instruments <csv> --windows <from:to,...> [--output-dir <path>]\n/list\n/report latest\n/report show <run_id>\n/mode <real|demo>\n/help\n/exit"
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
                Some("price-sma-cross-long") => StrategyTemplate::PriceSmaCrossLong,
                Some("price-sma-cross-short") => StrategyTemplate::PriceSmaCrossShort,
                Some("price-sma-cross-long-fast") => StrategyTemplate::PriceSmaCrossLongFast,
                Some("price-sma-cross-short-fast") => StrategyTemplate::PriceSmaCrossShortFast,
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
        Some("sweep") => parse_backtest_sweep_command(&args[1..]),
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
        Some("sweep") => vec![completion(
            "/sweep --templates price-sma-cross-long,price-sma-cross-short --instruments BTCUSDT,ETHUSDT --windows 2026-03-01:2026-03-15",
            "run a backtest sweep and export to PostgreSQL",
        )],
        Some("report") if parts.len() <= 2 => vec![
            completion("/report latest", "show latest stored run"),
            completion("/report show ", "show a stored run by id"),
        ],
        _ => Vec::new(),
    }
}

fn parse_backtest_sweep_command(args: &[String]) -> Result<BacktestCommand, String> {
    let mut templates = None;
    let mut instruments = None;
    let mut windows = None;
    let mut output_dir = "var/backtest-sweeps".to_string();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--templates" => {
                let value = args.get(index + 1).ok_or("missing value for --templates")?;
                templates = Some(parse_templates_csv(value)?);
                index += 2;
            }
            "--instruments" => {
                let value = args
                    .get(index + 1)
                    .ok_or("missing value for --instruments")?;
                instruments = Some(parse_instruments_csv(value));
                index += 2;
            }
            "--windows" => {
                let value = args.get(index + 1).ok_or("missing value for --windows")?;
                windows = Some(parse_windows_csv(value)?);
                index += 2;
            }
            "--output-dir" => {
                let value = args.get(index + 1).ok_or("missing value for --output-dir")?;
                output_dir = value.clone();
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}")),
        }
    }

    let templates = templates.ok_or(
        "usage: sweep --templates <csv> --instruments <csv> --windows <from:to,...> [--output-dir <path>]",
    )?;
    let instruments = instruments.ok_or(
        "usage: sweep --templates <csv> --instruments <csv> --windows <from:to,...> [--output-dir <path>]",
    )?;
    let windows = windows.ok_or(
        "usage: sweep --templates <csv> --instruments <csv> --windows <from:to,...> [--output-dir <path>]",
    )?;

    Ok(BacktestCommand::Sweep {
        templates,
        instruments,
        windows,
        output_dir,
    })
}

fn parse_templates_csv(raw: &str) -> Result<Vec<StrategyTemplate>, String> {
    let mut templates = Vec::new();
    for value in raw.split(',').map(str::trim).filter(|value| !value.is_empty()) {
        let template = match value {
            "liquidation-breakdown-short" => StrategyTemplate::LiquidationBreakdownShort,
            "price-sma-cross-long" => StrategyTemplate::PriceSmaCrossLong,
            "price-sma-cross-short" => StrategyTemplate::PriceSmaCrossShort,
            "price-sma-cross-long-fast" => StrategyTemplate::PriceSmaCrossLongFast,
            "price-sma-cross-short-fast" => StrategyTemplate::PriceSmaCrossShortFast,
            other => return Err(format!("unsupported template: {other}")),
        };
        if !templates.contains(&template) {
            templates.push(template);
        }
    }
    if templates.is_empty() {
        return Err("no templates provided".to_string());
    }
    Ok(templates)
}

fn parse_instruments_csv(raw: &str) -> Vec<String> {
    let mut instruments = raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_instrument_symbol)
        .collect::<Vec<_>>();
    instruments.sort();
    instruments.dedup();
    instruments
}

fn parse_windows_csv(raw: &str) -> Result<Vec<BacktestSweepWindow>, String> {
    let mut windows = Vec::new();
    for value in raw.split(',').map(str::trim).filter(|value| !value.is_empty()) {
        let Some((from_raw, to_raw)) = value.split_once(':') else {
            return Err(format!("invalid window: {value}. expected <from:to>"));
        };
        let from = NaiveDate::parse_from_str(from_raw, "%Y-%m-%d")
            .map_err(|_| format!("invalid date: {from_raw}"))?;
        let to = NaiveDate::parse_from_str(to_raw, "%Y-%m-%d")
            .map_err(|_| format!("invalid date: {to_raw}"))?;
        if from > to {
            return Err(format!(
                "invalid date range: from ({from}) must be on or before to ({to})"
            ));
        }
        windows.push(BacktestSweepWindow { from, to });
    }
    if windows.is_empty() {
        return Err("no windows provided".to_string());
    }
    Ok(windows)
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
    let from = from.ok_or("missing --from")?;
    let to = to.ok_or("missing --to")?;
    if from > to {
        return Err(format!(
            "invalid date range: from ({from}) must be on or before to ({to})"
        ));
    }
    Ok((from, to))
}

fn completion(value: &str, description: &str) -> ShellCompletion {
    ShellCompletion {
        value: value.to_string(),
        description: description.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_backtest_command_rejects_reversed_date_range() {
        let args = vec![
            "run".to_string(),
            "liquidation-breakdown-short".to_string(),
            "btcusdt".to_string(),
            "--from".to_string(),
            "2026-03-14".to_string(),
            "--to".to_string(),
            "2026-03-13".to_string(),
        ];

        let error = parse_backtest_command(&args).expect_err("expected invalid date range");

        assert_eq!(
            error,
            "invalid date range: from (2026-03-14) must be on or before to (2026-03-13)"
        );
    }

    #[test]
    fn parse_backtest_command_normalizes_instrument_and_accepts_valid_dates() {
        let args = vec![
            "run".to_string(),
            "liquidation-breakdown-short".to_string(),
            "btcusdt".to_string(),
            "--from".to_string(),
            "2026-03-13".to_string(),
            "--to".to_string(),
            "2026-03-14".to_string(),
        ];

        let command = parse_backtest_command(&args).expect("valid run command");

        assert_eq!(
            command,
            BacktestCommand::Run {
                template: StrategyTemplate::LiquidationBreakdownShort,
                instrument: "BTCUSDT".to_string(),
                from: NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
                to: NaiveDate::from_ymd_opt(2026, 3, 14).expect("date"),
            }
        );
    }

    #[test]
    fn parse_backtest_command_accepts_price_sma_template() {
        let args = vec![
            "run".to_string(),
            "price-sma-cross-long".to_string(),
            "ethusdt".to_string(),
            "--from".to_string(),
            "2026-03-13".to_string(),
            "--to".to_string(),
            "2026-03-14".to_string(),
        ];

        let command = parse_backtest_command(&args).expect("valid run command");

        assert_eq!(
            command,
            BacktestCommand::Run {
                template: StrategyTemplate::PriceSmaCrossLong,
                instrument: "ETHUSDT".to_string(),
                from: NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
                to: NaiveDate::from_ymd_opt(2026, 3, 14).expect("date"),
            }
        );
    }

    #[test]
    fn parse_backtest_command_accepts_price_sma_short_template() {
        let args = vec![
            "run".to_string(),
            "price-sma-cross-short".to_string(),
            "xrpusdt".to_string(),
            "--from".to_string(),
            "2026-03-13".to_string(),
            "--to".to_string(),
            "2026-03-14".to_string(),
        ];

        let command = parse_backtest_command(&args).expect("valid run command");

        assert_eq!(
            command,
            BacktestCommand::Run {
                template: StrategyTemplate::PriceSmaCrossShort,
                instrument: "XRPUSDT".to_string(),
                from: NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
                to: NaiveDate::from_ymd_opt(2026, 3, 14).expect("date"),
            }
        );
    }

    #[test]
    fn parse_backtest_command_accepts_price_sma_long_fast_template() {
        let args = vec![
            "run".to_string(),
            "price-sma-cross-long-fast".to_string(),
            "solusdt".to_string(),
            "--from".to_string(),
            "2026-03-13".to_string(),
            "--to".to_string(),
            "2026-03-14".to_string(),
        ];

        let command = parse_backtest_command(&args).expect("valid run command");

        assert_eq!(
            command,
            BacktestCommand::Run {
                template: StrategyTemplate::PriceSmaCrossLongFast,
                instrument: "SOLUSDT".to_string(),
                from: NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
                to: NaiveDate::from_ymd_opt(2026, 3, 14).expect("date"),
            }
        );
    }

    #[test]
    fn parse_backtest_command_accepts_price_sma_short_fast_template() {
        let args = vec![
            "run".to_string(),
            "price-sma-cross-short-fast".to_string(),
            "bnbusdt".to_string(),
            "--from".to_string(),
            "2026-03-13".to_string(),
            "--to".to_string(),
            "2026-03-14".to_string(),
        ];

        let command = parse_backtest_command(&args).expect("valid run command");

        assert_eq!(
            command,
            BacktestCommand::Run {
                template: StrategyTemplate::PriceSmaCrossShortFast,
                instrument: "BNBUSDT".to_string(),
                from: NaiveDate::from_ymd_opt(2026, 3, 13).expect("date"),
                to: NaiveDate::from_ymd_opt(2026, 3, 14).expect("date"),
            }
        );
    }

    #[test]
    fn parse_backtest_command_accepts_sweep() {
        let args = vec![
            "sweep".to_string(),
            "--templates".to_string(),
            "price-sma-cross-long,price-sma-cross-short-fast".to_string(),
            "--instruments".to_string(),
            "btcusdt,ethusdt".to_string(),
            "--windows".to_string(),
            "2026-03-01:2026-03-15,2026-03-16:2026-03-18".to_string(),
            "--output-dir".to_string(),
            "var/backtest-sweeps".to_string(),
        ];

        assert_eq!(
            parse_backtest_command(&args).unwrap(),
            BacktestCommand::Sweep {
                templates: vec![
                    StrategyTemplate::PriceSmaCrossLong,
                    StrategyTemplate::PriceSmaCrossShortFast,
                ],
                instruments: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
                windows: vec![
                    BacktestSweepWindow {
                        from: NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
                        to: NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
                    },
                    BacktestSweepWindow {
                        from: NaiveDate::from_ymd_opt(2026, 3, 16).unwrap(),
                        to: NaiveDate::from_ymd_opt(2026, 3, 18).unwrap(),
                    },
                ],
                output_dir: "var/backtest-sweeps".to_string(),
            }
        );
    }
}
