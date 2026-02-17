use ratatui::Terminal;
use ratatui::backend::TestBackend;

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
