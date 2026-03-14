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
        Some("--mode") | Some("--base-dir") => {
            let (mode, base_dir) = parse_terminal_args(&args)?;
            let mut terminal = BacktestTerminal::new(mode, base_dir);
            run_terminal(&mut terminal)?;
        }
        _ => {
            eprintln!(
                "usage: sandbox-quant-backtest run <template> <instrument> --from <YYYY-MM-DD> --to <YYYY-MM-DD> [--mode <demo|real>] [--base-dir <path>]\n       sandbox-quant-backtest list [--mode <demo|real>] [--base-dir <path>]\n       sandbox-quant-backtest report latest|show <run_id> [--mode <demo|real>] [--base-dir <path>]"
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
