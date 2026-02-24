use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use serde::Deserialize;

use crate::model::candle::Candle;
use crate::ui::{self, AppState, GridTab};

const DEFAULT_SCENARIO_DIR: &str = "docs/ui/scenarios";
const DEFAULT_INDEX_PATH: &str = "docs/ui/INDEX.md";
const DEFAULT_README_PATH: &str = "README.md";
const DEFAULT_SYMBOLS: [&str; 5] = ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "XRPUSDT"];

#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub title: String,
    #[serde(default = "default_width")]
    pub width: u16,
    #[serde(default = "default_height")]
    pub height: u16,
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default, alias = "step")]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Step {
    Key { value: String },
    Wait { ms: u64 },
    AssertText { value: String },
    Snapshot { path: String },
}

#[derive(Debug, Clone)]
pub struct RenderedScenario {
    pub id: String,
    pub title: String,
    pub snapshot_paths: Vec<SnapshotArtifact>,
}

#[derive(Debug, Clone)]
pub struct SnapshotArtifact {
    pub raw_path: String,
    pub image_path: Option<String>,
}

fn default_width() -> u16 {
    180
}

fn default_height() -> u16 {
    50
}

pub fn run_cli(args: &[String]) -> Result<()> {
    if args.is_empty() {
        return run_mode("full");
    }
    match args[0].as_str() {
        "smoke" => run_mode("smoke"),
        "full" => run_mode("full"),
        "scenario" => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow!("`scenario` requires an id argument"))?;
            run_single_scenario(id)
        }
        "readme-only" => {
            let rendered = collect_existing_rendered(DEFAULT_INDEX_PATH)?;
            update_readme(DEFAULT_README_PATH, &rendered)
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        other => bail!(
            "unknown subcommand `{}`. expected one of: smoke|full|scenario|readme-only",
            other
        ),
    }
}

fn run_mode(profile: &str) -> Result<()> {
    let scenarios = load_scenarios_from_dir(DEFAULT_SCENARIO_DIR)?;
    let filtered: Vec<Scenario> = if profile == "full" {
        scenarios
    } else {
        scenarios
            .into_iter()
            .filter(|s| s.profiles.iter().any(|p| p == profile))
            .collect()
    };
    if filtered.is_empty() {
        bail!("no scenarios found for profile `{}`", profile);
    }
    run_scenarios_and_write(&filtered, DEFAULT_INDEX_PATH, DEFAULT_README_PATH)?;
    Ok(())
}

fn run_single_scenario(id: &str) -> Result<()> {
    let scenarios = load_scenarios_from_dir(DEFAULT_SCENARIO_DIR)?;
    let scenario = scenarios
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| anyhow!("scenario `{}` not found", id))?;
    run_scenarios_and_write(&[scenario], DEFAULT_INDEX_PATH, DEFAULT_README_PATH)?;
    Ok(())
}

pub fn run_scenarios_and_write<P: AsRef<Path>, R: AsRef<Path>>(
    scenarios: &[Scenario],
    index_path: P,
    readme_path: R,
) -> Result<Vec<RenderedScenario>> {
    let rendered = run_scenarios(scenarios)?;
    write_index(index_path, &rendered)?;
    update_readme(readme_path, &rendered)?;
    Ok(rendered)
}

pub fn load_scenarios_from_dir<P: AsRef<Path>>(dir: P) -> Result<Vec<Scenario>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir.as_ref())
        .with_context(|| format!("failed to read {}", dir.as_ref().display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().map(|ext| ext == "toml").unwrap_or(false))
        .collect();
    paths.sort();

    let mut scenarios = Vec::with_capacity(paths.len());
    for path in paths {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read scenario {}", path.display()))?;
        let scenario: Scenario = toml::from_str(&raw)
            .with_context(|| format!("failed to parse scenario {}", path.display()))?;
        scenarios.push(scenario);
    }
    Ok(scenarios)
}

fn run_scenarios(scenarios: &[Scenario]) -> Result<Vec<RenderedScenario>> {
    scenarios.iter().map(run_scenario).collect()
}

fn run_scenario(s: &Scenario) -> Result<RenderedScenario> {
    let mut state = seed_state();
    let mut snapshots = Vec::new();

    for step in &s.steps {
        match step {
            Step::Key { value } => apply_key_action(&mut state, value)?,
            Step::Wait { ms } => {
                let _ = ms;
            }
            Step::AssertText { value } => {
                let text = render_to_text(&state, s.width, s.height)?;
                if !text.contains(value) {
                    bail!(
                        "scenario `{}` assert_text failed: missing `{}`",
                        s.id,
                        value
                    );
                }
            }
            Step::Snapshot { path } => {
                let text = render_to_text(&state, s.width, s.height)?;
                let snapshot_path = PathBuf::from(path);
                if let Some(parent) = snapshot_path.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create snapshot dir {}", parent.display())
                    })?;
                }
                fs::write(&snapshot_path, text).with_context(|| {
                    format!("failed to write snapshot {}", snapshot_path.display())
                })?;
                let image_path = write_svg_preview(&snapshot_path)?;
                snapshots.push(SnapshotArtifact {
                    raw_path: snapshot_path.to_string_lossy().to_string(),
                    image_path,
                });
            }
        }
    }

    if snapshots.is_empty() {
        let default_path = format!("docs/ui/screenshots/{}.txt", s.id);
        let text = render_to_text(&state, s.width, s.height)?;
        let default_path_buf = PathBuf::from(&default_path);
        if let Some(parent) = default_path_buf.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&default_path_buf, text)
            .with_context(|| format!("failed to write {}", default_path_buf.display()))?;
        let image_path = write_svg_preview(&default_path_buf)?;
        snapshots.push(SnapshotArtifact {
            raw_path: default_path,
            image_path,
        });
    }

    Ok(RenderedScenario {
        id: s.id.clone(),
        title: s.title.clone(),
        snapshot_paths: snapshots,
    })
}

pub fn render_to_text(state: &AppState, width: u16, height: u16) -> Result<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).context("failed to init test terminal")?;
    terminal
        .draw(|frame| ui::render(frame, state))
        .context("failed to render frame")?;
    let buf = terminal.backend().buffer();
    let area = buf.area;
    let mut out = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    Ok(out)
}

pub fn seed_state() -> AppState {
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
    state.ws_connected = true;
    state.current_equity_usdt = Some(10_000.0);
    state.initial_equity_usdt = Some(9_800.0);
    state.candles = seed_candles(now_ms, state.candle_interval_ms, 100, 67_000.0);
    state.last_price_update_ms = Some(now_ms);
    state.last_price_event_ms = Some(now_ms.saturating_sub(180));
    state.last_price_latency_ms = Some(180);
    state.last_order_history_update_ms = Some(now_ms.saturating_sub(1_100));
    state.last_order_history_event_ms = Some(now_ms.saturating_sub(1_950));
    state.last_order_history_latency_ms = Some(850);
    state.symbol_items = DEFAULT_SYMBOLS.iter().map(|v| v.to_string()).collect();
    state.strategy_item_symbols = vec![
        "BTCUSDT".to_string(),
        "ETHUSDT".to_string(),
        "SOLUSDT".to_string(),
    ];
    state.strategy_item_active = vec![true, false, true];
    state.strategy_item_total_running_ms = vec![3_600_000, 0, 7_200_000];
    state.network_reconnect_count = 1;
    state.network_tick_drop_count = 2;
    state.network_tick_latencies_ms = vec![120, 160, 170, 210, 300];
    state.network_fill_latencies_ms = vec![400, 600, 1200];
    state.network_order_sync_latencies_ms = vec![100, 130, 170];
    state.network_tick_in_timestamps_ms = vec![
        now_ms.saturating_sub(200),
        now_ms.saturating_sub(450),
        now_ms.saturating_sub(920),
        now_ms.saturating_sub(1_800),
        now_ms.saturating_sub(8_000),
    ];
    state.network_tick_drop_timestamps_ms = vec![
        now_ms.saturating_sub(600),
        now_ms.saturating_sub(9_500),
    ];
    state.network_reconnect_timestamps_ms = vec![now_ms.saturating_sub(16_000)];
    state.network_disconnect_timestamps_ms = vec![now_ms.saturating_sub(15_500)];
    state.network_last_fill_ms = Some(now_ms.saturating_sub(4_500));
    state.fast_sma = state.candles.last().map(|c| c.close * 0.9992);
    state.slow_sma = state.candles.last().map(|c| c.close * 0.9985);
    state
}

fn seed_candles(now_ms: u64, interval_ms: u64, count: usize, base_price: f64) -> Vec<Candle> {
    let count = count.max(8);
    let bucket_close = now_ms - (now_ms % interval_ms);
    let mut candles = Vec::with_capacity(count);
    for i in 0..count {
        let remaining = (count - i) as u64;
        let open_time = bucket_close.saturating_sub(remaining * interval_ms);
        let close_time = open_time.saturating_add(interval_ms);
        let drift = (i as f64) * 2.1;
        let wave = ((i as f64) * 0.24).sin() * 18.0;
        let open = base_price + drift + wave;
        let close = open + (((i % 6) as f64) - 2.0) * 1.7;
        let high = open.max(close) + 6.5;
        let low = open.min(close) - 6.0;
        candles.push(Candle {
            open,
            high,
            low,
            close,
            open_time,
            close_time,
        });
    }
    candles
}

fn apply_key_action(state: &mut AppState, key: &str) -> Result<()> {
    match key.to_ascii_lowercase().as_str() {
        "g" => {
            state.grid_open = !state.grid_open;
            if !state.grid_open {
                state.strategy_editor_open = false;
            }
        }
        "1" => {
            if state.grid_open {
                state.grid_tab = GridTab::Assets;
            }
        }
        "2" => {
            if state.grid_open {
                state.grid_tab = GridTab::Strategies;
            }
        }
        "3" => {
            if state.grid_open {
                state.grid_tab = GridTab::Risk;
            }
        }
        "4" => {
            if state.grid_open {
                state.grid_tab = GridTab::Network;
            }
        }
        "5" => {
            if state.grid_open {
                state.grid_tab = GridTab::History;
            }
        }
        "6" => {
            if state.grid_open {
                state.grid_tab = GridTab::SystemLog;
            }
        }
        "tab" => {
            if state.grid_open && state.grid_tab == GridTab::Strategies {
                state.grid_select_on_panel = !state.grid_select_on_panel;
            }
        }
        "c" => {
            if state.grid_open && state.grid_tab == GridTab::Strategies {
                state.strategy_editor_open = true;
            }
        }
        "esc" => {
            if state.strategy_editor_open {
                state.strategy_editor_open = false;
            } else if state.grid_open {
                state.grid_open = false;
            } else if state.symbol_selector_open {
                state.symbol_selector_open = false;
            } else if state.strategy_selector_open {
                state.strategy_selector_open = false;
            } else if state.account_popup_open {
                state.account_popup_open = false;
            } else if state.history_popup_open {
                state.history_popup_open = false;
            }
        }
        "t" => {
            if !state.grid_open {
                state.symbol_selector_open = true;
            }
        }
        "y" => {
            if !state.grid_open {
                state.strategy_selector_open = true;
            }
        }
        "a" => {
            if !state.grid_open {
                state.account_popup_open = true;
            }
        }
        "i" => {
            if !state.grid_open {
                state.history_popup_open = true;
            }
        }
        other => bail!("unsupported key action `{}`", other),
    }
    Ok(())
}

fn write_index<P: AsRef<Path>>(path: P, rendered: &[RenderedScenario]) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut out = String::new();
    out.push_str("# UI Snapshot Index\n\n");
    out.push_str("Generated by `cargo run --bin ui_docs -- <mode>`.\n\n");
    for item in rendered {
        out.push_str(&format!("## {} (`{}`)\n\n", item.title, item.id));
        for snapshot in &item.snapshot_paths {
            if let Some(image_path) = &snapshot.image_path {
                let rel_image = image_path
                    .strip_prefix("docs/ui/")
                    .unwrap_or(image_path.as_str());
                out.push_str(&format!("![{}]({})\n\n", item.id, xml_escape(rel_image)));
            }
            out.push_str(&format!("- raw: `{}`\n", snapshot.raw_path));
        }
        out.push('\n');
    }
    fs::write(path.as_ref(), out)
        .with_context(|| format!("failed to write {}", path.as_ref().display()))?;
    Ok(())
}

fn collect_existing_rendered<P: AsRef<Path>>(index_path: P) -> Result<Vec<RenderedScenario>> {
    let raw = fs::read_to_string(index_path.as_ref())
        .with_context(|| format!("failed to read {}", index_path.as_ref().display()))?;
    let mut rendered = Vec::new();
    let mut current: Option<RenderedScenario> = None;

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            if let Some(prev) = current.take() {
                rendered.push(prev);
            }
            let (title, id) = if let Some((lhs, rhs)) = rest.rsplit_once(" (`") {
                let id = rhs.trim_end_matches("`)");
                (lhs.trim().to_string(), id.to_string())
            } else {
                (rest.to_string(), "unknown".to_string())
            };
            current = Some(RenderedScenario {
                id,
                title,
                snapshot_paths: Vec::new(),
            });
        } else if let Some(path) = line
            .trim()
            .strip_prefix("- raw: `")
            .and_then(|v| v.strip_suffix('`'))
        {
            if let Some(curr) = current.as_mut() {
                let image_path = infer_svg_path(Path::new(path));
                curr.snapshot_paths.push(SnapshotArtifact {
                    raw_path: path.to_string(),
                    image_path,
                });
            }
        }
    }
    if let Some(prev) = current.take() {
        rendered.push(prev);
    }
    Ok(rendered)
}

pub fn update_readme<P: AsRef<Path>>(readme_path: P, rendered: &[RenderedScenario]) -> Result<()> {
    let start_marker = "<!-- UI_DOCS:START -->";
    let end_marker = "<!-- UI_DOCS:END -->";
    let raw = fs::read_to_string(readme_path.as_ref())
        .with_context(|| format!("failed to read {}", readme_path.as_ref().display()))?;
    let start = raw
        .find(start_marker)
        .ok_or_else(|| anyhow!("README start marker not found"))?;
    let end = raw
        .find(end_marker)
        .ok_or_else(|| anyhow!("README end marker not found"))?;
    if start >= end {
        bail!("README marker order invalid");
    }
    let mut block = String::new();
    block.push_str(start_marker);
    block.push('\n');
    block.push_str("### UI Docs (Auto)\n\n");
    block.push_str("- Generated by `cargo run --bin ui_docs -- smoke|full`\n");
    block.push_str("- Full index: `docs/ui/INDEX.md`\n\n");
    for item in rendered.iter().take(4) {
        if let Some(snapshot) = item.snapshot_paths.first() {
            if let Some(image_path) = &snapshot.image_path {
                block.push_str(&format!(
                    "![{}]({})\n\n",
                    item.title,
                    xml_escape(image_path)
                ));
            }
            block.push_str(&format!("- {} raw: `{}`\n", item.title, snapshot.raw_path));
        }
    }
    block.push('\n');
    block.push_str(end_marker);
    let next = format!(
        "{}{}{}",
        &raw[..start],
        block,
        &raw[end + end_marker.len()..]
    );
    fs::write(readme_path.as_ref(), next)
        .with_context(|| format!("failed to write {}", readme_path.as_ref().display()))?;
    Ok(())
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  cargo run --bin ui-docs");
    eprintln!("  cargo run --bin ui_docs -- smoke");
    eprintln!("  cargo run --bin ui_docs -- full");
    eprintln!("  cargo run --bin ui_docs -- scenario <id>");
    eprintln!("  cargo run --bin ui_docs -- readme-only");
}

fn write_svg_preview(raw_snapshot_path: &Path) -> Result<Option<String>> {
    let raw = fs::read_to_string(raw_snapshot_path)
        .with_context(|| format!("failed to read {}", raw_snapshot_path.display()))?;
    let svg_path = raw_snapshot_path.with_extension("svg");
    let lines: Vec<&str> = raw.lines().collect();
    let width_chars = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let height_chars = lines.len();
    if width_chars == 0 || height_chars == 0 {
        return Ok(None);
    }
    let cell_w = 9usize;
    let cell_h = 18usize;
    let px_w = (width_chars * cell_w + 24) as u32;
    let px_h = (height_chars * cell_h + 24) as u32;

    let mut svg = String::new();
    svg.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    svg.push('\n');
    svg.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
        px_w, px_h, px_w, px_h
    ));
    svg.push('\n');
    svg.push_str(&format!(
        r##"<rect x="0" y="0" width="{}" height="{}" fill="#0f111a"/>"##,
        px_w, px_h
    ));
    svg.push('\n');
    svg.push_str(r##"<g font-family="Menlo, Monaco, 'Courier New', monospace" font-size="14" fill="#d8dee9">"##);
    svg.push('\n');

    for (i, line) in lines.iter().enumerate() {
        let y = 18 + (i as u32) * (cell_h as u32);
        svg.push_str(&format!(
            r#"<text x="12" y="{}" xml:space="preserve">{}</text>"#,
            y,
            xml_escape(line)
        ));
        svg.push('\n');
    }
    svg.push_str("</g>\n</svg>\n");

    fs::write(&svg_path, svg).with_context(|| format!("failed to write {}", svg_path.display()))?;
    Ok(Some(svg_path.to_string_lossy().to_string()))
}

fn infer_svg_path(raw_path: &Path) -> Option<String> {
    let svg = raw_path.with_extension("svg");
    if svg.exists() {
        Some(svg.to_string_lossy().to_string())
    } else {
        None
    }
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
