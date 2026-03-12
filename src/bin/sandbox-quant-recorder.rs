use std::thread;
use std::time::Duration;

use chrono::Utc;
use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::app::cli::normalize_instrument_symbol;
use sandbox_quant::record::manager::{
    format_mode, RecordManager, RecordRuntimeStatus, RecordStatusFile,
};
use sandbox_quant::recorder_app::terminal::RecorderTerminal;
use sandbox_quant::storage::market_data_store::MarketDataRecorder;
use sandbox_quant::terminal::loop_shell::run_terminal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        let mut terminal = RecorderTerminal::new(BinanceMode::Demo, "var");
        return run_terminal(&mut terminal);
    }
    match args.first().map(String::as_str) {
        Some("start") => start_recorder(&args[1..])?,
        Some("status") => status_recorder(&args[1..])?,
        Some("stop") => stop_recorder(&args[1..])?,
        Some("run") => run_recorder(&args[1..])?,
        Some("--mode") | Some("--base-dir") => {
            let (mode, base_dir, _) = parse_control_args(&args)?;
            let mut terminal = RecorderTerminal::new(mode, base_dir);
            run_terminal(&mut terminal)?;
        }
        _ => {
            eprintln!(
                "usage: sandbox-quant-recorder <start|status|stop|run> [symbols...] [--mode <demo|real>] [--base-dir <path>]"
            );
            std::process::exit(2);
        }
    }
    Ok(())
}

fn start_recorder(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (mode, base_dir, symbols) = parse_control_args(args)?;
    let manager = RecordManager::new(base_dir);
    let status = manager.start(mode, symbols, Vec::new())?;
    println!("{}", render_status("record started", &status));
    Ok(())
}

fn status_recorder(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (mode, base_dir, _) = parse_control_args(args)?;
    let manager = RecordManager::new(base_dir);
    let status = manager.status(mode)?;
    println!("{}", render_status("record status", &status));
    Ok(())
}

fn stop_recorder(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (mode, base_dir, _) = parse_control_args(args)?;
    let manager = RecordManager::new(base_dir);
    let status = manager.stop(mode)?;
    println!("{}", render_status("record stopped", &status));
    Ok(())
}

fn run_recorder(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (mode, base_dir) = parse_args(args)?;
    let manager = RecordManager::new(base_dir);
    let Some(config) = manager.load_config_file(mode)? else {
        return Ok(());
    };
    if !config.desired_running {
        return Ok(());
    }

    let mut recorder = MarketDataRecorder::new(manager.base_dir());
    let status = recorder.start(
        mode,
        config.manual_symbols.clone(),
        config.strategy_symbols.clone(),
    )?;
    write_status(&manager, mode, &status)?;

    loop {
        thread::sleep(Duration::from_secs(1));
        let Some(config) = manager.load_config_file(mode)? else {
            break;
        };
        if !config.desired_running {
            break;
        }
        if !recorder.worker_alive(mode) {
            eprintln!(
                "recorder worker is not alive; restarting streams for mode={}",
                format_mode(mode)
            );
            recorder.update_manual_symbols(mode, config.manual_symbols.clone())?;
            recorder.update_strategy_symbols(mode, config.strategy_symbols.clone())?;
        }
        recorder.update_manual_symbols(mode, config.manual_symbols.clone())?;
        recorder.update_strategy_symbols(mode, config.strategy_symbols.clone())?;
        let status = recorder.status(mode);
        write_status(&manager, mode, &status)?;
    }

    let status = recorder
        .stop(mode)
        .unwrap_or_else(|_| recorder.status(mode));
    let mut stopped = status.clone();
    stopped.state = sandbox_quant::storage::market_data_store::RecorderState::Stopped;
    write_status(&manager, mode, &stopped)?;
    Ok(())
}

fn write_status(
    manager: &RecordManager,
    mode: BinanceMode,
    status: &sandbox_quant::storage::market_data_store::RecorderStatus,
) -> Result<(), Box<dyn std::error::Error>> {
    manager.write_status_file(
        mode,
        &RecordStatusFile {
            mode: format_mode(mode).to_string(),
            pid: std::process::id(),
            state: status.state.as_str().to_string(),
            binary_version: env!("CARGO_PKG_VERSION").to_string(),
            db_path: status.db_path.display().to_string(),
            started_at: status.started_at.map(|value| value.to_rfc3339()),
            updated_at: Utc::now().to_rfc3339(),
            manual_symbols: status.manual_symbols.clone(),
            strategy_symbols: status.strategy_symbols.clone(),
            watched_symbols: status.watched_symbols.clone(),
            worker_alive: status.worker_alive,
        },
    )?;
    Ok(())
}

fn render_status(header: &str, status: &RecordRuntimeStatus) -> String {
    [
        header.to_string(),
        format!("mode={}", format_mode(status.mode)),
        format!("state={}", status.state),
        format!("desired_running={}", status.desired_running),
        format!("process_alive={}", status.process_alive),
        format!("worker_alive={}", status.worker_alive),
        format!("status_stale={}", status.status_stale),
        format!(
            "heartbeat_age_sec={}",
            status
                .heartbeat_age_sec
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "pid={}",
            status
                .pid
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!("binary_version={}", status.binary_version),
        format!("db_path={}", status.db_path.display()),
        format!(
            "started_at={}",
            status
                .started_at
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!("updated_at={}", status.updated_at.to_rfc3339()),
        format!("manual_symbols={}", status.manual_symbols.len()),
        format!("strategy_symbols={}", status.strategy_symbols.len()),
        format!("watched_symbols={}", status.watched_symbols.len()),
        format!("liquidation_events={}", status.metrics.liquidation_events),
        format!("book_ticker_events={}", status.metrics.book_ticker_events),
        format!("agg_trade_events={}", status.metrics.agg_trade_events),
        format!(
            "derived_kline_1s_bars={}",
            status.metrics.derived_kline_1s_bars
        ),
    ]
    .join("\n")
}

fn parse_args(args: &[String]) -> Result<(BinanceMode, String), Box<dyn std::error::Error>> {
    let mut mode = None;
    let mut base_dir = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--mode" => {
                let value = args.get(index + 1).ok_or("missing value for --mode")?;
                mode = Some(match value.as_str() {
                    "demo" => BinanceMode::Demo,
                    "real" => BinanceMode::Real,
                    _ => return Err(format!("unsupported mode: {value}").into()),
                });
                index += 2;
            }
            "--base-dir" => {
                let value = args.get(index + 1).ok_or("missing value for --base-dir")?;
                base_dir = Some(value.clone());
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }
    Ok((
        mode.ok_or("missing --mode")?,
        base_dir.unwrap_or_else(|| "var".to_string()),
    ))
}

fn parse_control_args(
    args: &[String],
) -> Result<(BinanceMode, String, Vec<String>), Box<dyn std::error::Error>> {
    let mut mode = None;
    let mut base_dir = None;
    let mut symbols = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--mode" => {
                let value = args.get(index + 1).ok_or("missing value for --mode")?;
                mode = Some(match value.as_str() {
                    "demo" => BinanceMode::Demo,
                    "real" => BinanceMode::Real,
                    _ => return Err(format!("unsupported mode: {value}").into()),
                });
                index += 2;
            }
            "--base-dir" => {
                let value = args.get(index + 1).ok_or("missing value for --base-dir")?;
                base_dir = Some(value.clone());
                index += 2;
            }
            raw => {
                symbols.push(normalize_instrument_symbol(raw));
                index += 1;
            }
        }
    }
    Ok((
        mode.unwrap_or(BinanceMode::Demo),
        base_dir.unwrap_or_else(|| "var".to_string()),
        symbols,
    ))
}
