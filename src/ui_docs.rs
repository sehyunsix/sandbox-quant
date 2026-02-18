use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use serde::Deserialize;

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
    #[serde(default)]
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
    pub snapshot_paths: Vec<String>,
}

fn default_width() -> u16 {
    180
}

fn default_height() -> u16 {
    50
}

pub fn run_cli(args: &[String]) -> Result<()> {
    if args.is_empty() {
        print_usage();
        return Ok(());
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
                snapshots.push(snapshot_path.to_string_lossy().to_string());
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
        snapshots.push(default_path);
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
    state.ws_connected = true;
    state.current_equity_usdt = Some(10_000.0);
    state.initial_equity_usdt = Some(9_800.0);
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
    state
}

fn apply_key_action(state: &mut AppState, key: &str) -> Result<()> {
    match key.to_ascii_lowercase().as_str() {
        "g" => {
            state.v2_grid_open = !state.v2_grid_open;
            if !state.v2_grid_open {
                state.strategy_editor_open = false;
            }
        }
        "1" => {
            if state.v2_grid_open {
                state.v2_grid_tab = GridTab::Assets;
            }
        }
        "2" => {
            if state.v2_grid_open {
                state.v2_grid_tab = GridTab::Strategies;
            }
        }
        "3" => {
            if state.v2_grid_open {
                state.v2_grid_tab = GridTab::Risk;
            }
        }
        "4" => {
            if state.v2_grid_open {
                state.v2_grid_tab = GridTab::Network;
            }
        }
        "tab" => {
            if state.v2_grid_open && state.v2_grid_tab == GridTab::Strategies {
                state.v2_grid_select_on_panel = !state.v2_grid_select_on_panel;
            }
        }
        "c" => {
            if state.v2_grid_open && state.v2_grid_tab == GridTab::Strategies {
                state.strategy_editor_open = true;
            }
        }
        "esc" => {
            if state.strategy_editor_open {
                state.strategy_editor_open = false;
            } else if state.v2_grid_open {
                state.v2_grid_open = false;
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
            if !state.v2_grid_open {
                state.symbol_selector_open = true;
            }
        }
        "y" => {
            if !state.v2_grid_open {
                state.strategy_selector_open = true;
            }
        }
        "a" => {
            if !state.v2_grid_open {
                state.account_popup_open = true;
            }
        }
        "i" => {
            if !state.v2_grid_open {
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
            out.push_str(&format!("- `{}`\n", snapshot));
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
            .strip_prefix("- `")
            .and_then(|v| v.strip_suffix('`'))
        {
            if let Some(curr) = current.as_mut() {
                curr.snapshot_paths.push(path.to_string());
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
        if let Some(path) = item.snapshot_paths.first() {
            block.push_str(&format!("- {}: `{}`\n", item.title, path));
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
    eprintln!("  cargo run --bin ui_docs -- smoke");
    eprintln!("  cargo run --bin ui_docs -- full");
    eprintln!("  cargo run --bin ui_docs -- scenario <id>");
    eprintln!("  cargo run --bin ui_docs -- readme-only");
}
