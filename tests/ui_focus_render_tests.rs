use ratatui::backend::TestBackend;
use ratatui::Terminal;

use std::collections::HashMap;

use sandbox_quant::event::{AppEvent, AssetPnlEntry};
use sandbox_quant::model::order::OrderSide;
use sandbox_quant::order_manager::OrderHistoryStats;
use sandbox_quant::ui::ui_projection::UiProjection;
use sandbox_quant::ui::{self, AppState, GridTab};

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
        text.contains("Focus View (Drill-down)"),
        "focus popup title should be present in frame buffer"
    );
    assert!(
        text.contains("Strategy Metrics"),
        "focus popup should show strategy metrics panel"
    );
}

#[test]
/// Verifies close-all confirmation popup rendering:
/// when `close_all_confirm_open` is enabled, confirmation title and hint should be visible.
fn render_close_all_confirm_popup_when_enabled() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.set_close_all_confirm_open(true);

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(text.contains("Confirm Close-All"));
    assert!(text.contains("Y/Enter"));
    assert!(text.contains("N/Esc"));
}

#[test]
/// Verifies grid strategy selection surface:
/// grid popup should render strategy rows and show selector marker for the current index.
fn render_grid_popup_with_strategy_selector() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.grid_open = true;
    state.grid_strategy_index = 1;

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(
        text.contains("Portfolio Grid"),
        "grid popup title should be present"
    );
    assert!(
        text.contains("MA(Fast 5/20)"),
        "strategy table should include selectable configured strategies"
    );
    assert!(
        text.contains("BTCUSDT"),
        "strategy table should show selected symbol"
    );
    assert!(
        text.contains("Strategy"),
        "grid strategy navigation hint should be visible"
    );
}

#[test]
/// Verifies grid positions tab rendering:
/// selecting positions tab should show position table title and row headers.
fn render_grid_popup_positions_tab() {
    let backend = TestBackend::new(140, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.grid_open = true;
    state.grid_tab = GridTab::Positions;

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(text.contains("Portfolio Grid"));
    assert!(text.contains("Positions"));
    assert!(text.contains("Symbol"));
    assert!(text.contains("OrderId"));
    assert!(text.contains("Close"));
    assert!(text.contains("Stop"));
    assert!(text.contains("StopType"));
    assert!(text.contains("EV"));
    assert!(text.contains("Score"));
    assert!(text.contains("Gate"));
    assert!(text.contains("UnrPnL"));
}

#[test]
/// Verifies positions EV columns still render concrete values after layout changes.
fn render_grid_popup_positions_tab_shows_ev_values() {
    let backend = TestBackend::new(140, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.grid_open = true;
    state.grid_tab = GridTab::Positions;

    let mut pnl = HashMap::new();
    pnl.insert(
        "BTCUSDT".to_string(),
        AssetPnlEntry {
            is_futures: false,
            side: Some(OrderSide::Buy),
            position_qty: 0.01,
            entry_price: 63000.0,
            realized_pnl_usdt: 1.0,
            unrealized_pnl_usdt: 2.0,
        },
    );
    state.apply(AppEvent::AssetPnlUpdate { by_symbol: pnl });
    state.apply(AppEvent::EvSnapshotUpdate {
        symbol: "BTCUSDT".to_string(),
        source_tag: "c01".to_string(),
        ev: 1.234,
        p_win: 0.77,
        gate_mode: "soft".to_string(),
        gate_blocked: false,
    });

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(text.contains("+1.234"));
    assert!(text.contains("0.77"));
    assert!(text.contains("SOFT"));
}

#[test]
/// Verifies periodic SYS-scope EV snapshots are rendered in positions table.
fn render_grid_popup_positions_tab_shows_sys_ev_values() {
    let backend = TestBackend::new(140, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT (FUT)", "MA(Config)", 120, 60_000, "1m");
    state.grid_open = true;
    state.grid_tab = GridTab::Positions;

    let mut pnl = HashMap::new();
    pnl.insert(
        "BTCUSDT (FUT)".to_string(),
        AssetPnlEntry {
            is_futures: true,
            side: Some(OrderSide::Buy),
            position_qty: 0.02,
            entry_price: 67000.0,
            realized_pnl_usdt: -1.0,
            unrealized_pnl_usdt: 3.0,
        },
    );
    state.apply(AppEvent::AssetPnlUpdate { by_symbol: pnl });
    state.apply(AppEvent::EvSnapshotUpdate {
        symbol: "BTCUSDT (FUT)".to_string(),
        source_tag: "sys".to_string(),
        ev: 0.5,
        p_win: 0.55,
        gate_mode: "shadow".to_string(),
        gate_blocked: false,
    });

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(text.contains("+0.500"));
    assert!(text.contains("0.55"));
    assert!(text.contains("SHADOW"));
}

#[test]
/// Verifies compact-height main layout:
/// even on short terminal height, main view should still render the Position panel.
fn render_main_view_keeps_position_panel_in_compact_terminal() {
    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(
        text.contains("Position"),
        "position panel title should remain visible in compact terminal"
    );
}

#[test]
/// Verifies grid rendering for dynamically registered strategies:
/// a custom strategy item and its source-tag keyed stats must be displayed in table output.
fn render_grid_popup_with_registered_custom_strategy() {
    let backend = TestBackend::new(160, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.grid_open = true;
    state.strategy_items = vec!["MA(Custom 8/29) [c01]".to_string()];
    state.strategy_item_symbols = vec!["BTCUSDT".to_string()];
    state.strategy_item_active = vec![false];
    state.strategy_item_created_at_ms = vec![0];
    state.strategy_item_total_running_ms = vec![0];
    state.grid_strategy_index = 0;
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
    assert!(text.contains("c01"));
}

#[test]
/// Verifies strategy config editor rendering:
/// when editor mode is enabled, popup title and editable fields should be visible.
fn render_strategy_editor_popup_when_enabled() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state
        .strategy_items
        .push("MA(Custom 8/29) [c01]".to_string());
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

#[test]
/// Verifies popup layering in grid mode:
/// when grid and strategy editor are both open, strategy config popup must still be visible.
fn render_strategy_editor_popup_over_grid() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.grid_open = true;
    state.strategy_editor_open = true;
    state.strategy_editor_fast = 8;
    state.strategy_editor_slow = 29;
    state.strategy_editor_cooldown = 3;

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(text.contains("Strategy Config"));
    assert!(text.contains("Fast Period"));
}

#[test]
/// Verifies split ON/OFF grid layout:
/// total summary row and both strategy panels must render alongside strategy rows.
fn render_grid_popup_with_total_and_split_panels() {
    let backend = TestBackend::new(140, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.grid_open = true;
    state.strategy_item_active = vec![true, false, false];

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(text.contains("Total"));
    assert!(text.contains("ON Total"));
    assert!(text.contains("OFF Total"));
    assert!(text.contains("MA(Config)"));
    assert!(text.contains("MA(Fast 5/20)"));
}

#[test]
/// Verifies strategy list windowing:
/// when strategy rows exceed panel height, selected lower-row strategy should still be visible.
fn render_grid_popup_scrolls_to_selected_strategy_row() {
    let backend = TestBackend::new(120, 32);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "S00", 120, 60_000, "1m");
    state.grid_open = true;
    state.strategy_items = (0..15).map(|n| format!("S{:02}", n)).collect();
    state.strategy_item_symbols = vec!["BTCUSDT".to_string(); 15];
    state.strategy_item_active = vec![true; 15];
    state.strategy_item_created_at_ms = vec![0; 15];
    state.strategy_item_total_running_ms = vec![0; 15];
    state.grid_strategy_index = 12;

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(
        text.contains("S12"),
        "selected lower-row strategy should be visible via windowed rendering"
    );
}

#[test]
/// Verifies asset table multi-symbol population:
/// grid popup should include symbols from symbol/strategy lists, not only current symbol.
fn render_grid_popup_asset_table_includes_multiple_symbols() {
    let backend = TestBackend::new(140, 36);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.grid_open = true;
    state.symbol_items = vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()];
    state.strategy_item_symbols = vec![
        "BTCUSDT".to_string(),
        "ETHUSDT".to_string(),
        "SOLUSDT".to_string(),
    ];
    state.strategy_items = vec![
        "MA(Config)".to_string(),
        "MA(Fast 5/20)".to_string(),
        "MA(Slow 20/60)".to_string(),
    ];
    state.balances.insert("USDT".to_string(), 120.0);
    state.balances.insert("XRP".to_string(), 30.0);
    state.strategy_item_active = vec![true, false, false];
    state.strategy_item_created_at_ms = vec![0, 0, 0];
    state.strategy_item_total_running_ms = vec![0, 0, 0];

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(text.contains("BTCUSDT"));
    assert!(text.contains("ETHUSDT"));
    assert!(text.contains("SOLUSDT"));
}

#[test]
/// Verifies system-log grid tab rendering:
/// selecting tab 5 should show system log rows inside the grid popup.
fn render_grid_popup_system_log_tab() {
    let backend = TestBackend::new(140, 36);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.grid_open = true;
    state.grid_tab = GridTab::SystemLog;
    state.log_messages.push("system: warmup complete".to_string());
    state.log_messages.push("system: ws connected".to_string());

    terminal
        .draw(|frame| ui::render(frame, &state))
        .expect("render should succeed");

    let text = buffer_text(&terminal);
    assert!(text.contains("System Log"));
    assert!(text.contains("system: ws connected"));
}

#[test]
/// Verifies projection asset aggregation:
/// balances and configured symbols should both contribute to asset row count.
fn projection_asset_aggregation_includes_balance_assets() {
    let mut state = AppState::new("BTCUSDT", "MA(Config)", 120, 60_000, "1m");
    state.symbol_items = vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()];
    state.strategy_item_symbols = vec![
        "BTCUSDT".to_string(),
        "ETHUSDT".to_string(),
        "SOLUSDT".to_string(),
    ];
    state.balances.insert("USDT".to_string(), 120.0);
    state.balances.insert("XRP".to_string(), 30.0);

    let v2 = UiProjection::from_legacy(&state);
    let symbols: Vec<String> = v2.assets.iter().map(|a| a.symbol.clone()).collect();
    assert_eq!(v2.assets.len(), 5);
    assert!(symbols.iter().any(|s| s == "BTCUSDT"));
    assert!(symbols.iter().any(|s| s == "ETHUSDT"));
    assert!(symbols.iter().any(|s| s == "SOLUSDT"));
    assert!(symbols.iter().any(|s| s == "USDT"));
    assert!(symbols.iter().any(|s| s == "XRP"));
}
