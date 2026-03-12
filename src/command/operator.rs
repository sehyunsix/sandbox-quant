use crate::app::bootstrap::BinanceMode;
use crate::app::commands::{AppCommand, PortfolioView};
use crate::domain::exposure::Exposure;
use crate::domain::instrument::Instrument;
use crate::domain::order_type::OrderType;
use crate::domain::position::Side;
use crate::execution::command::{CommandSource, ExecutionCommand};
use crate::strategy::command::{StrategyCommand, StrategyStartConfig};
use crate::strategy::model::StrategyTemplate;
use crate::terminal::completion::ShellCompletion;

#[derive(Debug, Clone, PartialEq)]
pub enum ShellInput {
    Empty,
    Help,
    Exit,
    Mode(BinanceMode),
    Command(AppCommand),
}

pub fn parse_app_command(args: &[String]) -> Result<AppCommand, String> {
    match args.first().map(String::as_str).unwrap_or("refresh") {
        "refresh" | "portfolio" => Ok(parse_portfolio_command(args)),
        "positions" => Ok(AppCommand::Portfolio(PortfolioView::Positions)),
        "balances" => Ok(AppCommand::Portfolio(PortfolioView::Balances)),
        "orders" => Ok(AppCommand::Portfolio(PortfolioView::Orders)),
        "close-all" => Ok(AppCommand::Execution(ExecutionCommand::CloseAll {
            source: CommandSource::User,
        })),
        "close-symbol" => {
            let instrument = args
                .get(1)
                .ok_or("usage: close-symbol <instrument>")?
                .clone();
            Ok(AppCommand::Execution(ExecutionCommand::CloseSymbol {
                instrument: Instrument::new(normalize_instrument_symbol(&instrument)),
                source: CommandSource::User,
            }))
        }
        "set-target-exposure" => {
            let instrument = args
                .get(1)
                .ok_or("usage: set-target-exposure <instrument> <target> [market|limit <price>]")?
                .clone();
            let raw_target = args
                .get(2)
                .ok_or("usage: set-target-exposure <instrument> <target> [market|limit <price>]")?;
            let target = raw_target
                .parse::<f64>()
                .map_err(|_| format!("invalid target exposure: {raw_target}"))?;
            let exposure = Exposure::new(target).ok_or(format!(
                "target exposure out of range: {target}. expected -1.0..=1.0"
            ))?;
            let order_type = match args.get(3).map(String::as_str) {
                None | Some("market") => OrderType::Market,
                Some("limit") => {
                    let raw_price = args
                        .get(4)
                        .ok_or("usage: set-target-exposure <instrument> <target> limit <price>")?;
                    let price = raw_price
                        .parse::<f64>()
                        .map_err(|_| format!("invalid limit price: {raw_price}"))?;
                    if price <= f64::EPSILON {
                        return Err(format!("invalid limit price: {raw_price}"));
                    }
                    OrderType::Limit { price }
                }
                Some(other) => {
                    return Err(format!(
                        "unsupported order type: {other}. expected market or limit"
                    ))
                }
            };
            Ok(AppCommand::Execution(
                ExecutionCommand::SetTargetExposure {
                    instrument: Instrument::new(normalize_instrument_symbol(&instrument)),
                    target: exposure,
                    order_type,
                    source: CommandSource::User,
                },
            ))
        }
        "option-order" => {
            let symbol = args
                .get(1)
                .ok_or("usage: option-order <symbol> <buy|sell> <qty> <limit_price>")?
                .clone();
            let side = match args.get(2).map(String::as_str) {
                Some("buy") => Side::Buy,
                Some("sell") => Side::Sell,
                Some(other) => {
                    return Err(format!(
                        "unsupported option side: {other}. expected buy or sell"
                    ))
                }
                None => return Err("usage: option-order <symbol> <buy|sell> <qty> <limit_price>".to_string()),
            };
            let raw_qty = args
                .get(3)
                .ok_or("usage: option-order <symbol> <buy|sell> <qty> <limit_price>")?;
            let qty = raw_qty
                .parse::<f64>()
                .map_err(|_| format!("invalid option quantity: {raw_qty}"))?;
            if qty <= f64::EPSILON {
                return Err(format!("invalid option quantity: {raw_qty}"));
            }
            let raw_price = args
                .get(4)
                .ok_or("usage: option-order <symbol> <buy|sell> <qty> <limit_price>")?;
            let price = raw_price
                .parse::<f64>()
                .map_err(|_| format!("invalid limit price: {raw_price}"))?;
            if price <= f64::EPSILON {
                return Err(format!("invalid limit price: {raw_price}"));
            }
            Ok(AppCommand::Execution(ExecutionCommand::SubmitOptionOrder {
                instrument: Instrument::new(normalize_option_symbol(&symbol)),
                side,
                qty,
                order_type: OrderType::Limit { price },
                source: CommandSource::User,
            }))
        }
        "strategy" => parse_strategy_command(args),
        other => Err(format!(
            "unsupported command: {other}. supported commands: portfolio, positions, balances, orders, close-all, close-symbol, set-target-exposure, option-order, strategy"
        )),
    }
}

fn parse_strategy_command(args: &[String]) -> Result<AppCommand, String> {
    match args.get(1).map(String::as_str) {
        Some("templates") => Ok(AppCommand::Strategy(StrategyCommand::Templates)),
        Some("list") => Ok(AppCommand::Strategy(StrategyCommand::List)),
        Some("history") => Ok(AppCommand::Strategy(StrategyCommand::History)),
        Some("show") => {
            let watch_id = parse_watch_id(args.get(2), "usage: strategy show <watch_id>")?;
            Ok(AppCommand::Strategy(StrategyCommand::Show { watch_id }))
        }
        Some("stop") => {
            let watch_id = parse_watch_id(args.get(2), "usage: strategy stop <watch_id>")?;
            Ok(AppCommand::Strategy(StrategyCommand::Stop { watch_id }))
        }
        Some("start") => {
            let template = parse_strategy_template(
                args.get(2),
                "usage: strategy start <template> <instrument> --risk-pct <value> --win-rate <value> --r <value> --max-entry-slippage <value>",
            )?;
            let instrument = args
                .get(3)
                .ok_or("usage: strategy start <template> <instrument> --risk-pct <value> --win-rate <value> --r <value> --max-entry-slippage <value>")?;
            let config = parse_strategy_start_flags(&args[4..])?;
            Ok(AppCommand::Strategy(StrategyCommand::Start {
                template,
                instrument: Instrument::new(normalize_instrument_symbol(instrument)),
                config,
            }))
        }
        _ => Err("usage: strategy <templates|start|list|show|stop|history>".to_string()),
    }
}

fn parse_watch_id(raw: Option<&String>, usage: &str) -> Result<u64, String> {
    let raw = raw.ok_or_else(|| usage.to_string())?;
    raw.parse::<u64>()
        .map_err(|_| format!("invalid watch id: {raw}"))
}

fn parse_strategy_template(raw: Option<&String>, usage: &str) -> Result<StrategyTemplate, String> {
    match raw.map(String::as_str) {
        Some("liquidation-breakdown-short") => Ok(StrategyTemplate::LiquidationBreakdownShort),
        Some(other) => Err(format!(
            "unsupported strategy template: {other}. expected liquidation-breakdown-short"
        )),
        None => Err(usage.to_string()),
    }
}

fn parse_strategy_start_flags(args: &[String]) -> Result<StrategyStartConfig, String> {
    let mut risk_pct = 0.005;
    let mut win_rate = 0.8;
    let mut r_multiple = 1.5;
    let mut max_entry_slippage_pct = 0.001;
    let mut index = 0usize;

    while index < args.len() {
        let flag = args
            .get(index)
            .ok_or("missing strategy flag".to_string())?
            .as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        let parsed = value
            .parse::<f64>()
            .map_err(|_| format!("invalid value for {flag}: {value}"))?;
        match flag {
            "--risk-pct" => risk_pct = parsed,
            "--win-rate" => win_rate = parsed,
            "--r" => r_multiple = parsed,
            "--max-entry-slippage" => max_entry_slippage_pct = parsed,
            _ => return Err(format!("unsupported strategy flag: {flag}")),
        }
        index += 2;
    }

    let config = StrategyStartConfig {
        risk_pct,
        win_rate,
        r_multiple,
        max_entry_slippage_pct,
    };

    if !(0.0 < config.risk_pct && config.risk_pct <= 1.0) {
        return Err(format!(
            "invalid strategy risk_pct: {}. expected 0 < risk_pct <= 1",
            config.risk_pct
        ));
    }
    if !(0.0..=1.0).contains(&config.win_rate) {
        return Err(format!(
            "invalid strategy win_rate: {}. expected 0 <= win_rate <= 1",
            config.win_rate
        ));
    }
    if config.r_multiple <= f64::EPSILON {
        return Err(format!(
            "invalid strategy r_multiple: {}. expected r > 0",
            config.r_multiple
        ));
    }
    if config.max_entry_slippage_pct <= f64::EPSILON {
        return Err(format!(
            "invalid strategy max_entry_slippage_pct: {}. expected slippage > 0",
            config.max_entry_slippage_pct
        ));
    }

    Ok(config)
}

fn parse_portfolio_command(args: &[String]) -> AppCommand {
    match args.get(1).map(String::as_str) {
        None => AppCommand::Portfolio(PortfolioView::Overview),
        Some("positions") => AppCommand::Portfolio(PortfolioView::Positions),
        Some("balances") => AppCommand::Portfolio(PortfolioView::Balances),
        Some("orders") => AppCommand::Portfolio(PortfolioView::Orders),
        Some("refresh") => AppCommand::Portfolio(PortfolioView::Overview),
        Some(_) => AppCommand::Portfolio(PortfolioView::Overview),
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
    if args.first().map(String::as_str) == Some("mode") {
        let raw_mode = args.get(1).ok_or("usage: /mode <real|demo>")?;
        let mode = match raw_mode.as_str() {
            "real" => BinanceMode::Real,
            "demo" => BinanceMode::Demo,
            _ => {
                return Err(format!(
                    "unsupported mode: {raw_mode}. expected real or demo"
                ))
            }
        };
        return Ok(ShellInput::Mode(mode));
    }
    parse_app_command(&args).map(ShellInput::Command)
}

pub fn shell_help_text() -> &'static str {
    "/portfolio [positions|balances|orders]\n/positions\n/balances\n/orders\n/close-all\n/close-symbol <instrument>\n/set-target-exposure <instrument> <target> [market|limit <price>]\n/option-order <symbol> <buy|sell> <qty> <limit_price>\n/strategy <templates|start|list|show|stop|history>\n/mode <real|demo>\n/help\n/exit"
}

pub fn complete_shell_input(line: &str, instruments: &[String]) -> Vec<String> {
    complete_shell_input_with_description(line, instruments)
        .into_iter()
        .map(|item| item.value)
        .collect()
}

pub fn complete_shell_input_with_description(
    line: &str,
    instruments: &[String],
) -> Vec<ShellCompletion> {
    complete_shell_input_with_market_data(line, instruments, &[])
}

pub fn complete_shell_input_with_market_data(
    line: &str,
    instruments: &[String],
    priced_instruments: &[(String, f64)],
) -> Vec<ShellCompletion> {
    let trimmed = line.trim_start();
    let without_prefix = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let trailing_space = without_prefix.ends_with(' ');
    let parts: Vec<&str> = without_prefix.split_whitespace().collect();

    if parts.is_empty() {
        return shell_commands()
            .into_iter()
            .map(|command| ShellCompletion {
                value: format!("/{}", command.name),
                description: command.description.to_string(),
            })
            .collect();
    }

    if parts.len() == 1 && !trailing_space {
        return shell_commands()
            .into_iter()
            .filter(|command| command.name.starts_with(parts[0]))
            .map(|command| ShellCompletion {
                value: format!("/{}", command.name),
                description: command.description.to_string(),
            })
            .collect();
    }

    let command = parts[0];
    let current = if trailing_space {
        ""
    } else {
        parts.last().copied().unwrap_or_default()
    };
    let current_upper = current.trim().to_ascii_uppercase();

    match command {
        "mode" => ["real", "demo"]
            .into_iter()
            .filter(|mode| mode.starts_with(current))
            .map(|mode| ShellCompletion {
                value: format!("/mode {mode}"),
                description: match mode {
                    "real" => "switch to real Binance endpoints",
                    "demo" => "switch to Binance demo endpoints",
                    _ => "",
                }
                .to_string(),
            })
            .collect(),
        "portfolio" => ["positions", "balances", "orders"]
            .into_iter()
            .filter(|section| section.starts_with(current))
            .map(|section| ShellCompletion {
                value: format!("/portfolio {section}"),
                description: match section {
                    "positions" => "show non-flat positions after refresh",
                    "balances" => "show visible balances after refresh",
                    "orders" => "show open orders after refresh",
                    _ => "",
                }
                .to_string(),
            })
            .collect(),
        "close-symbol" => {
            let normalized_prefix = fallback_base_symbol(current)
                .as_deref()
                .map(normalize_instrument_symbol);
            let mut known_matches: Vec<String> = instruments
                .iter()
                .filter(|instrument| {
                    current_upper.is_empty()
                        || instrument.starts_with(&current_upper)
                        || normalized_prefix
                            .as_ref()
                            .is_some_and(|prefix| instrument.starts_with(prefix))
                })
                .cloned()
                .collect();

            if known_matches.is_empty() {
                known_matches.extend(fallback_instrument_suggestions(current));
            }

            known_matches
                .into_iter()
                .map(|instrument| ShellCompletion {
                    value: format!("/{command} {instrument}"),
                    description: match command {
                        "close-symbol" => "submit a close order for this instrument",
                        "set-target-exposure" => "plan and submit toward target exposure",
                        _ => "",
                    }
                    .to_string(),
                })
                .fold(Vec::<ShellCompletion>::new(), |mut acc, item| {
                    if !acc.iter().any(|existing| existing.value == item.value) {
                        acc.push(item);
                    }
                    acc
                })
        }
        "set-target-exposure" => complete_target_exposure_input(
            parts.as_slice(),
            trailing_space,
            current,
            &current_upper,
            instruments,
            priced_instruments,
        ),
        "option-order" => complete_option_order_input(
            parts.as_slice(),
            trailing_space,
            current,
            &current_upper,
            instruments,
        ),
        "strategy" => {
            complete_strategy_input(parts.as_slice(), trailing_space, current, instruments)
        }
        _ => Vec::new(),
    }
}

fn complete_strategy_input(
    parts: &[&str],
    trailing_space: bool,
    current: &str,
    instruments: &[String],
) -> Vec<ShellCompletion> {
    let arg_index = if trailing_space {
        parts.len()
    } else {
        parts.len().saturating_sub(1)
    };

    match arg_index {
        1 => ["templates", "start", "list", "show", "stop", "history"]
            .into_iter()
            .filter(|item| item.starts_with(current))
            .map(|item| ShellCompletion {
                value: format!("/strategy {item}"),
                description: match item {
                    "templates" => "show available strategy templates",
                    "start" => "arm a strategy watch",
                    "list" => "show active strategy watches",
                    "show" => "show one strategy watch",
                    "stop" => "stop one active strategy watch",
                    "history" => "show finished strategy watches",
                    _ => "",
                }
                .to_string(),
            })
            .collect(),
        2 if parts.first().copied() == Some("strategy")
            && parts.get(1).copied() == Some("start") =>
        {
            StrategyTemplate::all()
                .into_iter()
                .filter(|template| template.slug().starts_with(current))
                .map(|template| ShellCompletion {
                    value: format!("/strategy start {}", template.slug()),
                    description: "event-driven one-shot short strategy".to_string(),
                })
                .collect()
        }
        3 if parts.first().copied() == Some("strategy")
            && parts.get(1).copied() == Some("start") =>
        {
            complete_strategy_start_instrument(parts, current, instruments)
        }
        4 if parts.first().copied() == Some("strategy")
            && parts.get(1).copied() == Some("start") =>
        {
            vec![ShellCompletion {
                value: format!(
                    "/strategy start {} {} --risk-pct 0.005 --win-rate 0.8 --r 1.5 --max-entry-slippage 0.001",
                    parts.get(2).copied().unwrap_or("liquidation-breakdown-short"),
                    normalize_instrument_symbol(parts.get(3).copied().unwrap_or("BTC")),
                ),
                description: "start a liquidation breakdown short watch".to_string(),
            }]
        }
        _ => Vec::new(),
    }
}

fn complete_strategy_start_instrument(
    parts: &[&str],
    current: &str,
    instruments: &[String],
) -> Vec<ShellCompletion> {
    let current_upper = current.trim().to_ascii_uppercase();
    let normalized_prefix = fallback_base_symbol(current)
        .as_deref()
        .map(normalize_instrument_symbol);
    let mut known_matches: Vec<String> = instruments
        .iter()
        .filter(|instrument| {
            current_upper.is_empty()
                || instrument.starts_with(&current_upper)
                || normalized_prefix
                    .as_ref()
                    .is_some_and(|prefix| instrument.starts_with(prefix))
        })
        .cloned()
        .collect();

    if known_matches.is_empty() {
        known_matches.extend(fallback_instrument_suggestions(current));
    }

    known_matches
        .into_iter()
        .map(|instrument| ShellCompletion {
            value: format!(
                "/strategy start {} {}",
                parts
                    .get(2)
                    .copied()
                    .unwrap_or("liquidation-breakdown-short"),
                instrument
            ),
            description: "choose a futures instrument".to_string(),
        })
        .collect()
}

pub fn normalize_instrument_symbol(raw: &str) -> String {
    let upper = raw.trim().to_ascii_uppercase();
    if looks_like_option_symbol(&upper) {
        return upper;
    }
    let known_quotes = ["USDT", "USDC", "BUSD", "FDUSD"];
    if known_quotes.iter().any(|quote| upper.ends_with(quote)) {
        upper
    } else {
        format!("{upper}USDT")
    }
}

pub fn normalize_option_symbol(raw: &str) -> String {
    raw.trim().to_ascii_uppercase()
}

fn fallback_instrument_suggestions(prefix: &str) -> impl Iterator<Item = String> {
    let Some(base) = fallback_base_symbol(prefix) else {
        return Vec::new().into_iter();
    };
    let mut suggestions = Vec::new();
    suggestions.push(normalize_instrument_symbol(&base));
    suggestions.push(format!("{base}USDC"));
    suggestions.into_iter()
}

fn fallback_base_symbol(prefix: &str) -> Option<String> {
    let base = prefix.trim().to_ascii_uppercase();
    let known_quotes = ["USDT", "USDC", "BUSD", "FDUSD"];
    if base.is_empty()
        || base.len() > 12
        || !base.chars().all(|ch| ch.is_ascii_alphanumeric())
        || known_quotes.iter().any(|quote| base.contains(quote))
    {
        None
    } else {
        Some(base)
    }
}

fn complete_target_exposure_input(
    parts: &[&str],
    trailing_space: bool,
    current: &str,
    current_upper: &str,
    instruments: &[String],
    priced_instruments: &[(String, f64)],
) -> Vec<ShellCompletion> {
    let arg_index = if trailing_space {
        parts.len()
    } else {
        parts.len().saturating_sub(1)
    };

    match arg_index {
        1 => {
            complete_instrument_argument("set-target-exposure", current, current_upper, instruments)
        }
        2 => target_exposure_suggestions(parts.get(1).copied().unwrap_or(""), current),
        3 => order_type_suggestions(
            parts.get(1).copied().unwrap_or(""),
            parts.get(2).copied().unwrap_or(""),
            current,
        ),
        4 => limit_price_suggestions(
            parts.get(1).copied().unwrap_or(""),
            parts.get(2).copied().unwrap_or(""),
            parts.get(3).copied().unwrap_or(""),
            current,
            priced_instruments,
        ),
        _ => Vec::new(),
    }
}

fn complete_instrument_argument(
    command: &str,
    current: &str,
    current_upper: &str,
    instruments: &[String],
) -> Vec<ShellCompletion> {
    let normalized_prefix = fallback_base_symbol(current)
        .as_deref()
        .map(normalize_instrument_symbol);
    let mut known_matches: Vec<String> = instruments
        .iter()
        .filter(|instrument| {
            current_upper.is_empty()
                || instrument.starts_with(current_upper)
                || normalized_prefix
                    .as_ref()
                    .is_some_and(|prefix| instrument.starts_with(prefix))
        })
        .cloned()
        .collect();

    if known_matches.is_empty() {
        known_matches.extend(fallback_instrument_suggestions(current));
    }

    known_matches
        .into_iter()
        .map(|instrument| ShellCompletion {
            value: format!("/{command} {instrument}"),
            description: "plan and submit toward target exposure".to_string(),
        })
        .fold(Vec::<ShellCompletion>::new(), |mut acc, item| {
            if !acc.iter().any(|existing| existing.value == item.value) {
                acc.push(item);
            }
            acc
        })
}

fn complete_option_order_input(
    parts: &[&str],
    trailing_space: bool,
    current: &str,
    current_upper: &str,
    instruments: &[String],
) -> Vec<ShellCompletion> {
    let arg_index = if trailing_space {
        parts.len()
    } else {
        parts.len().saturating_sub(1)
    };

    match arg_index {
        1 => complete_option_symbol_argument(current, current_upper, instruments),
        2 => ["buy", "sell"]
            .into_iter()
            .filter(|side| side.starts_with(current))
            .map(|side| ShellCompletion {
                value: format!(
                    "/option-order {} {side}",
                    parts.get(1).copied().unwrap_or("BTC-260327-200000-C")
                ),
                description: "choose option order side".to_string(),
            })
            .collect(),
        3 => ["0.01", "0.10", "1.00"]
            .into_iter()
            .filter(|qty| qty.starts_with(current))
            .map(|qty| ShellCompletion {
                value: format!(
                    "/option-order {} {} {qty}",
                    parts.get(1).copied().unwrap_or("BTC-260327-200000-C"),
                    parts.get(2).copied().unwrap_or("buy"),
                ),
                description: "order quantity".to_string(),
            })
            .collect(),
        4 => ["5", "50", "500"]
            .into_iter()
            .filter(|price| price.starts_with(current))
            .map(|price| ShellCompletion {
                value: format!(
                    "/option-order {} {} {} {price}",
                    parts.get(1).copied().unwrap_or("BTC-260327-200000-C"),
                    parts.get(2).copied().unwrap_or("buy"),
                    parts.get(3).copied().unwrap_or("0.01"),
                ),
                description: "limit price".to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn complete_option_symbol_argument(
    current: &str,
    current_upper: &str,
    instruments: &[String],
) -> Vec<ShellCompletion> {
    let option_symbols = instruments
        .iter()
        .filter(|instrument| looks_like_option_symbol(instrument))
        .cloned()
        .collect::<Vec<_>>();

    if current_upper.is_empty() {
        return option_underlying_prefixes(&option_symbols)
            .into_iter()
            .map(|prefix| ShellCompletion {
                value: format!("/option-order {prefix}"),
                description: "type expiry/strike to narrow option contracts".to_string(),
            })
            .collect();
    }

    let direct_matches = option_symbols
        .iter()
        .filter(|instrument| instrument.starts_with(current_upper))
        .cloned()
        .collect::<Vec<_>>();
    if !direct_matches.is_empty() {
        return direct_matches
            .into_iter()
            .map(|instrument| ShellCompletion {
                value: format!("/option-order {instrument}"),
                description: "submit a Binance options limit order".to_string(),
            })
            .collect();
    }

    option_underlying_prefixes(&option_symbols)
        .into_iter()
        .filter(|prefix| prefix.starts_with(&format!("{}-", current.trim().to_ascii_uppercase())))
        .map(|prefix| ShellCompletion {
            value: format!("/option-order {prefix}"),
            description: "type expiry/strike to narrow option contracts".to_string(),
        })
        .collect()
}

fn option_underlying_prefixes(option_symbols: &[String]) -> Vec<String> {
    option_symbols
        .iter()
        .filter_map(|symbol| symbol.split('-').next().map(|base| format!("{base}-")))
        .fold(Vec::<String>::new(), |mut acc, item| {
            if !acc.iter().any(|existing| existing == &item) {
                acc.push(item);
            }
            acc
        })
}

fn looks_like_option_symbol(raw: &str) -> bool {
    let mut parts = raw.split('-');
    let Some(base) = parts.next() else {
        return false;
    };
    let Some(expiry) = parts.next() else {
        return false;
    };
    let Some(strike) = parts.next() else {
        return false;
    };
    let Some(kind) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && !base.is_empty()
        && expiry.len() == 6
        && expiry.chars().all(|ch| ch.is_ascii_digit())
        && strike.chars().all(|ch| ch.is_ascii_digit())
        && matches!(kind, "C" | "P")
}

fn target_exposure_suggestions(instrument: &str, current: &str) -> Vec<ShellCompletion> {
    let prefix = current.trim();
    ["0.25", "0.5", "-0.25", "-0.5", "1.0", "-1.0"]
        .into_iter()
        .filter(|target| target.starts_with(prefix))
        .map(|target| ShellCompletion {
            value: format!("/set-target-exposure {instrument} {target}"),
            description: "target signed exposure".to_string(),
        })
        .collect()
}

fn order_type_suggestions(instrument: &str, target: &str, current: &str) -> Vec<ShellCompletion> {
    ["market", "limit"]
        .into_iter()
        .filter(|order_type| order_type.starts_with(current))
        .map(|order_type| ShellCompletion {
            value: format!("/set-target-exposure {instrument} {target} {order_type}"),
            description: match order_type {
                "market" => "submit immediately at market",
                "limit" => "submit at an explicit limit price",
                _ => "",
            }
            .to_string(),
        })
        .collect()
}

fn limit_price_suggestions(
    instrument: &str,
    target: &str,
    order_type: &str,
    current: &str,
    priced_instruments: &[(String, f64)],
) -> Vec<ShellCompletion> {
    if order_type != "limit" {
        return Vec::new();
    }

    let suggestions = if current.trim().is_empty() {
        price_examples(instrument, priced_instruments)
    } else {
        vec![current.to_string()]
    };

    suggestions
        .into_iter()
        .map(|price| ShellCompletion {
            value: format!("/set-target-exposure {instrument} {target} limit {price}"),
            description: "limit price example; replace with desired price".to_string(),
        })
        .collect()
}

fn price_examples(instrument: &str, priced_instruments: &[(String, f64)]) -> Vec<String> {
    let maybe_price = priced_instruments
        .iter()
        .find(|(known, _)| known == instrument)
        .map(|(_, price)| *price);

    match maybe_price {
        Some(price) if price > f64::EPSILON => {
            let ticked = if price >= 1000.0 {
                10.0
            } else if price >= 100.0 {
                1.0
            } else if price >= 1.0 {
                0.1
            } else {
                0.01
            };
            let below = (price * 0.995 / ticked).floor() * ticked;
            let near = (price / ticked).round() * ticked;
            let above = (price * 1.005 / ticked).ceil() * ticked;
            vec![
                format!("{below:.2}"),
                format!("{near:.2}"),
                format!("{above:.2}"),
            ]
        }
        _ => vec!["1000".to_string(), "50000".to_string(), "68000".to_string()],
    }
}

struct ShellCommandSpec {
    name: &'static str,
    description: &'static str,
}

fn shell_commands() -> [ShellCommandSpec; 12] {
    [
        ShellCommandSpec {
            name: "portfolio",
            description: "refresh and show portfolio overview",
        },
        ShellCommandSpec {
            name: "positions",
            description: "refresh and show non-flat positions",
        },
        ShellCommandSpec {
            name: "balances",
            description: "refresh and show visible balances",
        },
        ShellCommandSpec {
            name: "orders",
            description: "refresh and show open orders",
        },
        ShellCommandSpec {
            name: "close-all",
            description: "submit close orders for all currently open instruments",
        },
        ShellCommandSpec {
            name: "close-symbol",
            description: "submit a close order for one instrument",
        },
        ShellCommandSpec {
            name: "set-target-exposure",
            description: "plan and submit toward a signed target exposure",
        },
        ShellCommandSpec {
            name: "option-order",
            description: "submit a Binance options limit order",
        },
        ShellCommandSpec {
            name: "strategy",
            description: "manage event-driven strategy watches",
        },
        ShellCommandSpec {
            name: "mode",
            description: "switch between real and demo Binance endpoints",
        },
        ShellCommandSpec {
            name: "help",
            description: "show available slash commands",
        },
        ShellCommandSpec {
            name: "exit",
            description: "leave the interactive shell",
        },
    ]
}
