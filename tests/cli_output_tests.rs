use sandbox_quant::app::commands::AppCommand;
use sandbox_quant::app::output::render_command_output;
use sandbox_quant::domain::instrument::Instrument;
use sandbox_quant::domain::market::Market;
use sandbox_quant::domain::position::PositionSnapshot;
use sandbox_quant::execution::command::{CommandSource, ExecutionCommand};
use sandbox_quant::portfolio::store::PortfolioStateStore;
use sandbox_quant::storage::event_log::{log, EventLog};
use serde_json::json;

#[test]
fn refresh_output_includes_store_summary() {
    let mut store = PortfolioStateStore::default();
    store.apply_snapshot(sandbox_quant::exchange::types::AuthoritativeSnapshot {
        balances: vec![],
        positions: vec![PositionSnapshot {
            instrument: Instrument::new("BTCUSDT"),
            market: Market::Futures,
            signed_qty: 0.25,
            entry_price: Some(65000.0),
        }],
        open_orders: vec![],
    });

    let mut event_log = EventLog::default();
    log(
        &mut event_log,
        "app.portfolio.refreshed",
        json!({ "positions": 1 }),
    );

    let output =
        render_command_output(&AppCommand::RefreshAuthoritativeState, &store, &event_log);

    assert!(output.contains("refresh completed"));
    assert!(output.contains("staleness=Fresh"));
    assert!(output.contains("positions=1"));
    assert!(output.contains("last_event=app.portfolio.refreshed"));
}

#[test]
fn execution_output_includes_last_event_kind() {
    let store = PortfolioStateStore::default();
    let mut event_log = EventLog::default();
    log(
        &mut event_log,
        "app.execution.completed",
        json!({
            "command_kind": "close_all",
            "batch_id": 7,
            "submitted": 2,
            "skipped": 1,
            "rejected": 0,
            "outcome_kind": "batch_completed",
        }),
    );

    let output = render_command_output(
        &AppCommand::Execution(ExecutionCommand::CloseAll {
            source: CommandSource::User,
        }),
        &store,
        &event_log,
    );

    assert!(output.contains("execution completed"));
    assert!(output.contains("command=close-all"));
    assert!(output.contains("submitted=2"));
    assert!(output.contains("skipped=1"));
    assert!(output.contains("rejected=0"));
}

#[test]
fn execution_output_renders_target_exposure_summary() {
    let store = PortfolioStateStore::default();
    let mut event_log = EventLog::default();
    log(
        &mut event_log,
        "app.execution.completed",
        json!({
            "command_kind": "set_target_exposure",
            "instrument": "BTCUSDT",
            "target": 0.25,
            "outcome_kind": "submitted",
        }),
    );

    let output = render_command_output(
        &AppCommand::Execution(ExecutionCommand::SetTargetExposure {
            instrument: Instrument::new("BTCUSDT"),
            target: sandbox_quant::domain::exposure::Exposure::new(0.25).expect("bounded"),
            source: CommandSource::User,
        }),
        &store,
        &event_log,
    );

    assert!(output.contains("command=set-target-exposure"));
    assert!(output.contains("instrument=BTCUSDT"));
    assert!(output.contains("target=0.25"));
}
