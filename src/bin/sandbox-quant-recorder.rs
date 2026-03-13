use std::thread;
use std::time::Duration;

use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::app::cli::normalize_instrument_symbol;
use sandbox_quant::record::coordination::RecorderCoordination;
use sandbox_quant::recorder_app::runtime::{MarketDataRecorder, RecorderState};
use sandbox_quant::recorder_app::terminal::RecorderTerminal;
use sandbox_quant::terminal::loop_shell::run_terminal;
use sandbox_quant::ui::recorder_output::render_live_recorder_status;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty()
        || matches!(
            args.first().map(String::as_str),
            Some("--mode") | Some("--base-dir")
        )
    {
        let (mode, base_dir) = parse_terminal_args(&args)?;
        let mut terminal = RecorderTerminal::new(mode, base_dir);
        return run_terminal(&mut terminal);
    }

    match args.first().map(String::as_str) {
        Some("run") => run_recorder(&args[1..])?,
        _ => {
            eprintln!(
                "usage: sandbox-quant-recorder [--mode <demo|real>] [--base-dir <path>]\n       sandbox-quant-recorder run [symbols...] [--mode <demo|real>] [--base-dir <path>]"
            );
            std::process::exit(2);
        }
    }

    Ok(())
}

fn run_recorder(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (mode, base_dir, symbols) = parse_run_args(args)?;
    let coordination = RecorderCoordination::new(base_dir.clone());
    let strategy_symbols = coordination.strategy_symbols(mode)?;
    let mut recorder = MarketDataRecorder::new(base_dir.clone());
    let status = recorder.start(mode, symbols, strategy_symbols)?;
    println!("{}", render_live_recorder_status("record running", &status));

    loop {
        thread::sleep(Duration::from_secs(1));
        let status = recorder.status(mode);
        if status.state != RecorderState::Running {
            break;
        }
        if !status.worker_alive {
            eprintln!(
                "recorder worker is not alive; restarting streams for mode={}",
                mode.as_str()
            );
            recorder.update_manual_symbols(mode, status.manual_symbols.clone())?;
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

fn parse_run_args(
    args: &[String],
) -> Result<(BinanceMode, String, Vec<String>), Box<dyn std::error::Error>> {
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut symbols = Vec::new();
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
            raw => {
                symbols.push(normalize_instrument_symbol(raw));
                index += 1;
            }
        }
    }
    Ok((mode, base_dir, symbols))
}
