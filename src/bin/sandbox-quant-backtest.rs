use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::backtest_app::runner::{run_backtest_for_path, BacktestConfig};
use sandbox_quant::backtest_app::snapshot::maybe_prepare_snapshot_from_postgres;
use sandbox_quant::backtest_app::terminal::BacktestTerminal;
use sandbox_quant::command::backtest::{parse_backtest_command, BacktestCommand};
use sandbox_quant::dataset::query::{
    load_backtest_report, load_backtest_run_summaries, persist_backtest_report,
};
use sandbox_quant::dataset::schema::init_schema_for_path;
use sandbox_quant::record::coordination::RecorderCoordination;
use sandbox_quant::storage::postgres_market_data::{
    export_backtest_report_to_postgres, postgres_url_from_env,
};
use sandbox_quant::terminal::loop_shell::run_terminal;
use sandbox_quant::ui::backtest_output::{render_backtest_run, render_backtest_run_list};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        let mut terminal = BacktestTerminal::new(BinanceMode::Demo, "var");
        return run_terminal(&mut terminal);
    }
    match args.first().map(String::as_str) {
        Some("run") | Some("list") | Some("report") => run_backtest_command(&args)?,
        Some("export") if args.get(1).map(String::as_str) == Some("postgres") => {
            run_backtest_export_postgres_command(&args[2..])?
        }
        Some("--mode") | Some("--base-dir") => {
            let (mode, base_dir) = parse_terminal_args(&args)?;
            let mut terminal = BacktestTerminal::new(mode, base_dir);
            run_terminal(&mut terminal)?;
        }
        _ => {
            eprintln!(
                "usage: sandbox-quant-backtest run <template> <instrument> --from <YYYY-MM-DD> --to <YYYY-MM-DD> [--mode <demo|real>] [--base-dir <path>]\n       sandbox-quant-backtest list [--mode <demo|real>] [--base-dir <path>]\n       sandbox-quant-backtest report latest|show <run_id> [--mode <demo|real>] [--base-dir <path>]\n       sandbox-quant-backtest export postgres latest|show <run_id> [--mode <demo|real>] [--base-dir <path>] [--postgres-url <url>]"
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

fn run_backtest_command(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (mode, base_dir, command_args) = split_global_args(args)?;
    let command = parse_backtest_command(&command_args)?;
    let db_path = RecorderCoordination::new(base_dir.clone()).db_path(mode);
    match command {
        BacktestCommand::Run {
            template,
            instrument,
            from,
            to,
        } => {
            if let Some(message) =
                maybe_prepare_snapshot_from_postgres(mode, &base_dir, &instrument, from, to)?
            {
                eprintln!("{message}");
            }
            init_schema_for_path(&db_path)?;
            let report = run_backtest_for_path(
                &db_path,
                mode,
                template,
                &instrument,
                from,
                to,
                BacktestConfig::default(),
            )?;
            let run_id = persist_backtest_report(&db_path, &report)?;
            let mut report = report;
            report.run_id = Some(run_id);
            println!("{}", render_backtest_run(&report));
        }
        BacktestCommand::List => {
            let runs = load_backtest_run_summaries(&db_path, 20)?;
            println!("{}", render_backtest_run_list(&runs));
        }
        BacktestCommand::ReportLatest => {
            if let Some(report) = load_backtest_report(&db_path, None)? {
                println!("{}", render_backtest_run(&report));
            } else {
                println!("backtest report\nstate=missing");
            }
        }
        BacktestCommand::ReportShow { run_id } => {
            if let Some(report) = load_backtest_report(&db_path, Some(run_id))? {
                println!("{}", render_backtest_run(&report));
            } else {
                println!("backtest report\nrun_id={run_id}\nstate=missing");
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BacktestExportSelection {
    Latest,
    Show(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BacktestExportPostgresConfig {
    mode: BinanceMode,
    base_dir: String,
    postgres_url: Option<String>,
    selection: BacktestExportSelection,
}

fn run_backtest_export_postgres_command(
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_export_postgres_args(args)?;
    let db_path = RecorderCoordination::new(config.base_dir.clone()).db_path(config.mode);
    let report = match config.selection {
        BacktestExportSelection::Latest => load_backtest_report(&db_path, None)?,
        BacktestExportSelection::Show(run_id) => load_backtest_report(&db_path, Some(run_id))?,
    };
    let Some(report) = report else {
        println!("backtest export\nstate=missing\ntarget=postgres");
        return Ok(());
    };
    let postgres_url = match config.postgres_url {
        Some(url) => url,
        None => postgres_url_from_env()?,
    };
    let export = export_backtest_report_to_postgres(&postgres_url, &report)?;
    println!(
        "{}",
        [
            "backtest export".to_string(),
            "target=postgres".to_string(),
            format!("source_run_id={}", export.source_run_id),
            format!("export_run_id={}", export.export_run_id),
            format!("trade_rows={}", export.trade_rows),
            format!("equity_point_rows={}", export.equity_point_rows),
        ]
        .join("\n")
    );
    Ok(())
}

fn split_global_args(
    args: &[String],
) -> Result<(BinanceMode, String, Vec<String>), Box<dyn std::error::Error>> {
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut command_args = Vec::new();
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
            other => {
                command_args.push(other.to_string());
                index += 1;
            }
        }
    }
    Ok((mode, base_dir, command_args))
}

fn parse_export_postgres_args(
    args: &[String],
) -> Result<BacktestExportPostgresConfig, Box<dyn std::error::Error>> {
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut postgres_url = None;
    let mut selection = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "latest" => {
                selection = Some(BacktestExportSelection::Latest);
                index += 1;
            }
            "show" => {
                let raw = args.get(index + 1).ok_or("missing run id for show")?;
                let run_id = raw
                    .parse::<i64>()
                    .map_err(|_| format!("invalid run id: {raw}"))?;
                selection = Some(BacktestExportSelection::Show(run_id));
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
            "--postgres-url" => {
                let value = args
                    .get(index + 1)
                    .ok_or("missing value for --postgres-url")?;
                postgres_url = Some(value.clone());
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }
    let selection =
        selection.ok_or("usage: sandbox-quant-backtest export postgres latest|show <run_id>")?;
    Ok(BacktestExportPostgresConfig {
        mode,
        base_dir,
        postgres_url,
        selection,
    })
}
