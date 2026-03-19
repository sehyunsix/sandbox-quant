use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::backtest_app::export::{export_report_to_postgres, maybe_export_report_to_postgres};
use sandbox_quant::backtest_app::runner::{
    run_backtest_for_path, run_backtest_for_postgres_url, BacktestConfig,
};
use sandbox_quant::backtest_app::snapshot::maybe_prepare_snapshot_from_postgres;
use sandbox_quant::backtest_app::terminal::BacktestTerminal;
use sandbox_quant::command::backtest::{parse_backtest_command, BacktestCommand, BacktestSweepWindow};
use sandbox_quant::dataset::query::{
    load_backtest_report, load_backtest_run_summaries, persist_backtest_report,
};
use sandbox_quant::dataset::schema::init_schema_for_path;
use sandbox_quant::observability::logging::init_logging;
use sandbox_quant::record::coordination::RecorderCoordination;
use sandbox_quant::storage::postgres_market_data::postgres_url_from_env;
use sandbox_quant::terminal::loop_shell::run_terminal;
use sandbox_quant::ui::backtest_output::{render_backtest_run, render_backtest_run_list};
use tracing::{error, info, warn};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let init_mode = args
        .windows(2)
        .find(|window| window[0] == "--mode")
        .map(|window| window[1].as_str())
        .unwrap_or("demo");
    init_logging("backtest", Some(init_mode))?;
    info!(service = "backtest", mode = init_mode, args = ?args, "process started");

    let result: Result<(), Box<dyn std::error::Error>> = if args.is_empty() {
        let mut terminal = BacktestTerminal::new(BinanceMode::Demo, "var");
        run_terminal(&mut terminal)
    } else {
        match args.first().map(String::as_str) {
            Some("run") | Some("sweep") | Some("list") | Some("report") => {
                run_backtest_command(&args)
            }
            Some("--mode") | Some("--base-dir") => {
                let (mode, base_dir) = parse_terminal_args(&args)?;
                let mut terminal = BacktestTerminal::new(mode, base_dir);
                run_terminal(&mut terminal)
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "usage: sandbox-quant-backtest run <template> <instrument> --from <YYYY-MM-DD> --to <YYYY-MM-DD> [--mode <demo|real>] [--base-dir <path>]\n       sandbox-quant-backtest sweep --templates <csv> --instruments <csv> --windows <from:to,...> [--output-dir <path>] [--mode <demo|real>] [--base-dir <path>]\n       sandbox-quant-backtest list [--mode <demo|real>] [--base-dir <path>]\n       sandbox-quant-backtest report latest|show <run_id> [--mode <demo|real>] [--base-dir <path>]",
            )
            .into()),
        }
    };
    if let Err(error) = result {
        error!(service = "backtest", mode = init_mode, error = %error, "process failed");
        return Err(error);
    }
    info!(service = "backtest", mode = init_mode, "process completed");
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
            if backtest_source_is_postgres() {
                let postgres_url = postgres_url_from_env()?;
                let report = run_backtest_for_postgres_url(
                    &postgres_url,
                    mode,
                    template,
                    &instrument,
                    from,
                    to,
                    BacktestConfig::default(),
                )?;
                let export_run_id = maybe_export_report_to_postgres(&report)?
                    .ok_or("PostgreSQL direct mode expected PostgreSQL export to succeed")?;
                info!(
                    service = "backtest",
                    mode = mode.as_str(),
                    export_run_id,
                    "postgres export completed"
                );
                eprintln!("postgres export completed: export_run_id={export_run_id}");
                println!("{}", render_backtest_run(&report));
                return Ok(());
            }
            if let Some(message) =
                maybe_prepare_snapshot_from_postgres(mode, &base_dir, &instrument, from, to)?
            {
                warn!(service = "backtest", mode = mode.as_str(), instrument = instrument, from = %from, to = %to, message = %message, "snapshot preparation message");
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
            if let Some(export_run_id) = maybe_export_report_to_postgres(&report)? {
                info!(
                    service = "backtest",
                    mode = mode.as_str(),
                    export_run_id,
                    "postgres export completed"
                );
                eprintln!("postgres export completed: export_run_id={export_run_id}");
            }
            println!("{}", render_backtest_run(&report));
        }
        BacktestCommand::List => {
            let runs = load_backtest_run_summaries(&db_path, 20)?;
            println!("{}", render_backtest_run_list(&runs));
        }
        BacktestCommand::Sweep {
            templates,
            instruments,
            windows,
            output_dir,
        } => {
            let postgres_url = postgres_url_from_env()?;
            let sweep = run_backtest_sweep(
                &postgres_url,
                mode,
                &base_dir,
                templates,
                instruments,
                windows,
                &output_dir,
            )?;
            println!("backtest sweep");
            println!("summary_path={}", sweep.summary_path.display());
            println!("raw_log_path={}", sweep.raw_log_path.display());
            println!("runs={}", sweep.rows.len());
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

#[derive(Debug)]
struct SweepRow {
    template: String,
    instrument: String,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    export_run_id: Option<i64>,
    state: String,
    note: String,
}

#[derive(Debug)]
struct SweepReport {
    summary_path: std::path::PathBuf,
    raw_log_path: std::path::PathBuf,
    rows: Vec<SweepRow>,
}

fn run_backtest_sweep(
    postgres_url: &str,
    mode: BinanceMode,
    base_dir: &str,
    templates: Vec<sandbox_quant::strategy::model::StrategyTemplate>,
    instruments: Vec<String>,
    windows: Vec<BacktestSweepWindow>,
    output_dir: &str,
) -> Result<SweepReport, Box<dyn std::error::Error>> {
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let run_dir = std::path::PathBuf::from(output_dir).join(&timestamp);
    std::fs::create_dir_all(&run_dir)?;
    let summary_path = run_dir.join("summary.md");
    let raw_log_path = run_dir.join("runs.log");
    let mut summary = String::new();
    summary.push_str("# Backtest Sweep\n\n");
    summary.push_str(&format!("- timestamp: `{timestamp}`\n"));
    summary.push_str(&format!("- mode: `{}`\n", mode.as_str()));
    summary.push_str(&format!("- base_dir: `{base_dir}`\n"));
    summary.push_str("- postgres_url: `postgres://***`\n\n");
    summary.push_str("| template | instrument | from | to | export_run_id | state | note |\n");
    summary.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");

    let mut raw_log = String::new();
    let mut rows = Vec::new();

    for template in templates {
        for instrument in &instruments {
            for window in &windows {
                let (report, export_run_id) = execute_backtest_run_with_export(
                    postgres_url,
                    mode,
                    base_dir,
                    template,
                    instrument,
                    window.from,
                    window.to,
                )?;
                let rendered = render_backtest_run(&report);
                let state = rendered
                    .lines()
                    .find_map(|line| line.strip_prefix("state="))
                    .unwrap_or("unknown")
                    .to_string();
                raw_log.push_str(&format!(
                    "=== {} | {} | {} -> {} ===\n{}\nexport_run_id={}\n\n",
                    template.slug(),
                    instrument,
                    window.from,
                    window.to,
                    rendered,
                    export_run_id
                ));
                let row = SweepRow {
                    template: template.slug().to_string(),
                    instrument: instrument.clone(),
                    from: window.from,
                    to: window.to,
                    export_run_id: Some(export_run_id),
                    state: state.clone(),
                    note: "ok".to_string(),
                };
                summary.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} | {} |\n",
                    row.template,
                    row.instrument,
                    row.from,
                    row.to,
                    row.export_run_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "n/a".to_string()),
                    row.state,
                    row.note
                ));
                rows.push(row);
            }
        }
    }

    summary.push_str(
        "\nGrafana reads the exported runs from `backtest_runs`, `backtest_trades`, and `backtest_equity_points`.\n",
    );
    summary.push_str(
        "Use the `sandbox-quant backtest pnl` dashboard to inspect the exported `export_run_id` values.\n",
    );

    std::fs::write(&summary_path, summary)?;
    std::fs::write(&raw_log_path, raw_log)?;

    Ok(SweepReport {
        summary_path,
        raw_log_path,
        rows,
    })
}

fn execute_backtest_run_with_export(
    postgres_url: &str,
    mode: BinanceMode,
    base_dir: &str,
    template: sandbox_quant::strategy::model::StrategyTemplate,
    instrument: &str,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<(sandbox_quant::backtest_app::runner::BacktestReport, i64), Box<dyn std::error::Error>>
{
    if backtest_source_is_postgres() {
        let report = run_backtest_for_postgres_url(
            postgres_url,
            mode,
            template,
            instrument,
            from,
            to,
            BacktestConfig::default(),
        )?;
        let export_run_id = export_report_to_postgres(&report, postgres_url)?;
        return Ok((report, export_run_id));
    }

    if let Some(message) = maybe_prepare_snapshot_from_postgres(mode, base_dir, instrument, from, to)?
    {
        eprintln!("{message}");
    }
    let db_path = RecorderCoordination::new(base_dir.to_string()).db_path(mode);
    init_schema_for_path(&db_path)?;
    let report = run_backtest_for_path(
        &db_path,
        mode,
        template,
        instrument,
        from,
        to,
        BacktestConfig::default(),
    )?;
    let run_id = persist_backtest_report(&db_path, &report)?;
    let mut report = report;
    report.run_id = Some(run_id);
    let export_run_id = export_report_to_postgres(&report, postgres_url)?;
    Ok((report, export_run_id))
}

fn backtest_source_is_postgres() -> bool {
    std::env::var("SANDBOX_QUANT_BACKTEST_SOURCE")
        .ok()
        .map(|value| value.trim().eq_ignore_ascii_case("postgres"))
        .unwrap_or(false)
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
