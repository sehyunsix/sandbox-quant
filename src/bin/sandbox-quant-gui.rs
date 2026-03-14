use std::fs;
use std::path::{Path, PathBuf};

use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::charting::adapters::sandbox::{
    equity_scene_from_report, market_scene_from_snapshot_with_timeframe, MarketTimeframe,
};
use sandbox_quant::charting::plotters::PlottersRenderer;
use sandbox_quant::charting::render::ChartRenderer;
use sandbox_quant::charting::scene::{RenderRequest, RenderedFrame};
use sandbox_quant::gui::app::{GuiLaunchConfig, SandboxQuantGuiApp};
use sandbox_quant::visualization::service::VisualizationService;
use sandbox_quant::visualization::types::DashboardQuery;

#[derive(Debug, Clone, PartialEq)]
struct GuiCliConfig {
    launch: GuiLaunchConfig,
    headless_debug_export_dir: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_launch_config(&std::env::args().skip(1).collect::<Vec<_>>())?;
    if let Some(export_dir) = &config.headless_debug_export_dir {
        export_headless_debug(&config.launch, export_dir)?;
        return Ok(());
    }
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1540.0, 940.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Sandbox Quant GUI",
        native_options,
        Box::new(move |_cc| Ok(Box::new(SandboxQuantGuiApp::new(config.launch)))),
    )?;
    Ok(())
}

fn parse_launch_config(args: &[String]) -> Result<GuiCliConfig, Box<dyn std::error::Error>> {
    let mut mode = BinanceMode::Demo;
    let mut base_dir = "var".to_string();
    let mut symbol = String::new();
    let mut from = chrono::Utc::now().date_naive() - chrono::Days::new(1);
    let mut to = chrono::Utc::now().date_naive();
    let mut market_timeframe = MarketTimeframe::Tick1s;
    let mut headless_debug_export_dir = None;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--mode" => {
                let value = args.get(index + 1).ok_or("missing value for --mode")?;
                mode = match value.as_str() {
                    "demo" => BinanceMode::Demo,
                    "real" => BinanceMode::Real,
                    other => return Err(format!("unsupported mode: {other}").into()),
                };
                index += 2;
            }
            "--base-dir" => {
                base_dir = args
                    .get(index + 1)
                    .ok_or("missing value for --base-dir")?
                    .clone();
                index += 2;
            }
            "--symbol" => {
                symbol = args
                    .get(index + 1)
                    .ok_or("missing value for --symbol")?
                    .trim()
                    .to_ascii_uppercase();
                index += 2;
            }
            "--from" => {
                from = chrono::NaiveDate::parse_from_str(
                    args.get(index + 1).ok_or("missing value for --from")?,
                    "%Y-%m-%d",
                )?;
                index += 2;
            }
            "--to" => {
                to = chrono::NaiveDate::parse_from_str(
                    args.get(index + 1).ok_or("missing value for --to")?,
                    "%Y-%m-%d",
                )?;
                index += 2;
            }
            "--chart-timeframe" => {
                market_timeframe = match args
                    .get(index + 1)
                    .ok_or("missing value for --chart-timeframe")?
                    .as_str()
                {
                    "1s" => MarketTimeframe::Tick1s,
                    "1m" => MarketTimeframe::Minute1m,
                    "3m" => MarketTimeframe::Minute3m,
                    "5m" => MarketTimeframe::Minute5m,
                    "15m" => MarketTimeframe::Minute15m,
                    "30m" => MarketTimeframe::Minute30m,
                    "1h" => MarketTimeframe::Hour1h,
                    "4h" => MarketTimeframe::Hour4h,
                    "1w" => MarketTimeframe::Week1w,
                    "1d" => MarketTimeframe::Day1d,
                    "1mo" => MarketTimeframe::Month1mo,
                    other => return Err(format!("unsupported chart timeframe: {other}").into()),
                };
                index += 2;
            }
            "--headless-debug-export-dir" => {
                headless_debug_export_dir = Some(PathBuf::from(
                    args.get(index + 1)
                        .ok_or("missing value for --headless-debug-export-dir")?,
                ));
                index += 2;
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }

    Ok(GuiCliConfig {
        launch: GuiLaunchConfig {
            mode,
            base_dir,
            symbol,
            from,
            to,
            market_timeframe,
        },
        headless_debug_export_dir,
    })
}

fn export_headless_debug(
    launch: &GuiLaunchConfig,
    export_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(export_dir)?;
    let service = VisualizationService;
    let snapshot = service.load_dashboard(DashboardQuery {
        mode: launch.mode,
        base_dir: PathBuf::from(launch.base_dir.clone()),
        symbol: launch.symbol.clone(),
        from: launch.from,
        to: launch.to,
        selected_run_id: None,
        run_limit: 24,
    })?;
    let renderer = PlottersRenderer;
    let market_scene =
        market_scene_from_snapshot_with_timeframe(&snapshot, launch.market_timeframe);
    let request = RenderRequest {
        width_px: 1540,
        height_px: 940,
        pixel_ratio: 1.0,
        oversample: 1,
    };
    let market_frame = renderer.render(&market_scene, &request)?;
    write_ppm(export_dir.join("market.ppm"), &market_frame)?;
    fs::write(
        export_dir.join("market-scene.txt"),
        format!("{market_scene:#?}"),
    )?;
    fs::write(
        export_dir.join("snapshot.txt"),
        snapshot_debug_summary(&snapshot),
    )?;

    if let Some(report) = &snapshot.selected_report {
        let equity_scene = equity_scene_from_report(report);
        let equity_frame = renderer.render(
            &equity_scene,
            &RenderRequest {
                width_px: 1540,
                height_px: 480,
                pixel_ratio: 1.0,
                oversample: 1,
            },
        )?;
        write_ppm(export_dir.join("equity.ppm"), &equity_frame)?;
        fs::write(
            export_dir.join("equity-scene.txt"),
            format!("{equity_scene:#?}"),
        )?;
    } else {
        fs::write(
            export_dir.join("equity-scene.txt"),
            "No selected_report available for equity scene export.\n",
        )?;
    }

    println!(
        "headless debug export completed\nexport_dir={}\nsymbol={}\nfrom={}\nto={}",
        export_dir.display(),
        snapshot.symbol,
        snapshot.from,
        snapshot.to
    );
    Ok(())
}

fn write_ppm(path: PathBuf, frame: &RenderedFrame) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = format!("P6\n{} {}\n255\n", frame.width_px, frame.height_px).into_bytes();
    bytes.extend_from_slice(&frame.rgb);
    fs::write(path, bytes)?;
    Ok(())
}

fn snapshot_debug_summary(
    snapshot: &sandbox_quant::visualization::types::DashboardSnapshot,
) -> String {
    let kline_range = snapshot
        .market_series
        .klines
        .first()
        .zip(snapshot.market_series.klines.last())
        .map(|(first, last)| format!("{}..{}", first.open_time_ms, last.close_time_ms))
        .unwrap_or_else(|| "-".to_string());
    let ticker_range = snapshot
        .market_series
        .book_tickers
        .first()
        .zip(snapshot.market_series.book_tickers.last())
        .map(|(first, last)| format!("{}..{}", first.event_time_ms, last.event_time_ms))
        .unwrap_or_else(|| "-".to_string());
    format!(
        concat!(
            "db_path={}\n",
            "symbol={}\n",
            "from={}\n",
            "to={}\n",
            "available_symbols={}\n",
            "recent_runs={}\n",
            "book_tickers={}\n",
            "liquidations={}\n",
            "klines={}\n",
            "kline_range={}\n",
            "ticker_range={}\n",
            "selected_run_id={:?}\n"
        ),
        snapshot.db_path.display(),
        snapshot.symbol,
        snapshot.from,
        snapshot.to,
        snapshot.available_symbols.len(),
        snapshot.recent_runs.len(),
        snapshot.market_series.book_tickers.len(),
        snapshot.market_series.liquidations.len(),
        snapshot.market_series.klines.len(),
        kline_range,
        ticker_range,
        snapshot.selected_run_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_launch_config_accepts_headless_debug_export_dir() {
        let args = vec![
            "--mode".to_string(),
            "demo".to_string(),
            "--base-dir".to_string(),
            "var".to_string(),
            "--symbol".to_string(),
            "btcusdt".to_string(),
            "--from".to_string(),
            "2026-03-13".to_string(),
            "--to".to_string(),
            "2026-03-13".to_string(),
            "--headless-debug-export-dir".to_string(),
            "/tmp/sq-debug".to_string(),
        ];

        let parsed = parse_launch_config(&args).expect("parse config");

        assert_eq!(parsed.launch.symbol, "BTCUSDT");
        assert_eq!(parsed.launch.market_timeframe, MarketTimeframe::Tick1s);
        assert_eq!(
            parsed.headless_debug_export_dir,
            Some(PathBuf::from("/tmp/sq-debug"))
        );
    }
}
