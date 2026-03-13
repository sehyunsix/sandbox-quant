use chrono::NaiveDate;
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::app::cli::normalize_instrument_symbol;
use sandbox_quant::backtest_app::terminal::BacktestTerminal;
use sandbox_quant::dataset::query::backtest_summary_for_path;
use sandbox_quant::record::coordination::RecorderCoordination;
use sandbox_quant::strategy::model::StrategyTemplate;
use sandbox_quant::terminal::loop_shell::run_terminal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        let mut terminal = BacktestTerminal::new(BinanceMode::Demo, "var");
        return run_terminal(&mut terminal);
    }
    match args.first().map(String::as_str) {
        Some("run") => run_backtest(&args[1..])?,
        Some("--mode") | Some("--base-dir") => {
            let (mode, base_dir) = parse_terminal_args(&args)?;
            let mut terminal = BacktestTerminal::new(mode, base_dir);
            run_terminal(&mut terminal)?;
        }
        _ => {
            eprintln!(
                "usage: sandbox-quant-backtest run <template> <instrument> --from <YYYY-MM-DD> --to <YYYY-MM-DD> [--mode <demo|real>] [--base-dir <path>]"
            );
            std::process::exit(2);
        }
    }
    Ok(())
}

fn parse_terminal_args(
    args: &[String],
) -> Result<(BinanceMode, String), Box<dyn std::error::Error>> {
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--mode" => {
                let value = args.get(index + 1).ok_or("missing value for --mode")?;
                mode = match value.as_str() {
                    "demo" => BinanceMode::Demo,
                    "real" => BinanceMode::Real,
                    _ => return Err(format!("unsupported mode: {value}").into()),
                };
                index += 2;
            }
            "--base-dir" => {
                let value = args.get(index + 1).ok_or("missing value for --base-dir")?;
                base_dir = value.clone();
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }
    Ok((mode, base_dir))
}

fn run_backtest(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (template, instrument, from, to, mode, base_dir) = parse_args(args)?;
    let db_path = RecorderCoordination::new(base_dir.clone()).db_path(mode);
    let summary = backtest_summary_for_path(&db_path, mode, &instrument, from, to)?;

    println!(
        "{}",
        [
            "backtest run".to_string(),
            format!("mode={}", mode.as_str()),
            format!("template={}", template.slug()),
            format!("instrument={}", instrument),
            format!("from={}", from),
            format!("to={}", to),
            format!("db_path={}", db_path.display()),
            format!("liquidation_events={}", summary.liquidation_events),
            format!("book_ticker_events={}", summary.book_ticker_events),
            format!("agg_trade_events={}", summary.agg_trade_events),
            format!("derived_kline_1s_bars={}", summary.derived_kline_1s_bars),
            "outcome=dataset-ready".to_string(),
        ]
        .join("\n")
    );

    Ok(())
}

fn parse_args(
    args: &[String],
) -> Result<
    (
        StrategyTemplate,
        String,
        NaiveDate,
        NaiveDate,
        BinanceMode,
        String,
    ),
    Box<dyn std::error::Error>,
> {
    let template = match args.first().map(String::as_str) {
        Some("liquidation-breakdown-short") => StrategyTemplate::LiquidationBreakdownShort,
        Some(other) => return Err(format!("unsupported template: {other}").into()),
        None => return Err("missing template".into()),
    };
    let instrument = normalize_instrument_symbol(args.get(1).ok_or("missing instrument")?);
    let mut from = None;
    let mut to = None;
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut index = 2usize;
    while index < args.len() {
        match args[index].as_str() {
            "--from" => {
                let value = args.get(index + 1).ok_or("missing value for --from")?;
                from = Some(NaiveDate::parse_from_str(value, "%Y-%m-%d")?);
                index += 2;
            }
            "--to" => {
                let value = args.get(index + 1).ok_or("missing value for --to")?;
                to = Some(NaiveDate::parse_from_str(value, "%Y-%m-%d")?);
                index += 2;
            }
            "--mode" => {
                let value = args.get(index + 1).ok_or("missing value for --mode")?;
                mode = match value.as_str() {
                    "demo" => BinanceMode::Demo,
                    "real" => BinanceMode::Real,
                    _ => return Err(format!("unsupported mode: {value}").into()),
                };
                index += 2;
            }
            "--base-dir" => {
                let value = args.get(index + 1).ok_or("missing value for --base-dir")?;
                base_dir = value.clone();
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }
    let from = from.ok_or("missing --from")?;
    let to = to.ok_or("missing --to")?;
    Ok((template, instrument, from, to, mode, base_dir))
}
