use ratatui::Terminal;
use ratatui::backend::TestBackend;

use sandbox_quant::order_manager::OrderHistoryStats;
use sandbox_quant::ui::{self, AppState};

fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
    let buf = terminal.backend().buffer();
    let area = buf.area;
    let mut out = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
/// Verifies focus drill-down UI route:
/// enabling `focus_popup_open` must render the dedicated focus popup title
/// so operators can visually confirm the drill-down is active.
fn render_focus_popup_when_enabled() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.focus_popup_open = true;

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(
        text.contains("Focus View (V2 Drill-down)"),
        "focus popup title should be present in frame buffer"
    );
}

#[test]
/// Verifies grid strategy selection surface:
/// grid popup should render strategy rows and show selector marker for the current index.
fn render_grid_popup_with_strategy_selector() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.v2_grid_open = true;
    state.v2_grid_strategy_index = 1;

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(
        text.contains("Portfolio Grid (V2)"),
        "grid popup title should be present"
    );
    assert!(
        text.contains("MA(Fast 5/20)"),
        "strategy table should include selectable configured strategies"
    );
    assert!(
        text.contains("Strategy"),
        "grid strategy navigation hint should be visible"
    );
}

#[test]
/// Verifies grid rendering for dynamically registered strategies:
/// a custom strategy item and its source-tag keyed stats must be displayed in table output.
fn render_grid_popup_with_registered_custom_strategy() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.v2_grid_open = true;
    state.strategy_items.push("MA(Custom 8/29) [c01]".to_string());
    state.v2_grid_strategy_index = state.strategy_items.len() - 1;
    state.strategy_stats.insert(
        "c01".to_string(),
        OrderHistoryStats {
            trade_count: 3,
            win_count: 2,
            lose_count: 1,
            realized_pnl: 1.25,
        },
    );

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(text.contains("MA(Custom 8/29) [c01]"));
    assert!(text.contains("+1.2500"));
}

#[test]
/// Verifies strategy config editor rendering:
/// when editor mode is enabled, popup title and editable fields should be visible.
fn render_strategy_editor_popup_when_enabled() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.strategy_items.push("MA(Custom 8/29) [c01]".to_string());
    state.strategy_editor_open = true;
    state.strategy_editor_index = state.strategy_items.len() - 1;
    state.strategy_editor_fast = 8;
    state.strategy_editor_slow = 29;
    state.strategy_editor_cooldown = 3;

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(text.contains("Strategy Config"));
    assert!(text.contains("MA(Custom 8/29) [c01]"));
    assert!(text.contains("Fast Period"));
    assert!(text.contains("Slow Period"));
}
