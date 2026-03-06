use crate::app::commands::AppCommand;
use crate::portfolio::store::PortfolioStateStore;
use crate::storage::event_log::EventLog;

pub fn render_command_output(
    command: &AppCommand,
    store: &PortfolioStateStore,
    event_log: &EventLog,
) -> String {
    match command {
        AppCommand::RefreshAuthoritativeState => render_refresh_summary(store, event_log),
        AppCommand::Execution(_) => render_execution_summary(event_log),
    }
}

fn render_refresh_summary(store: &PortfolioStateStore, event_log: &EventLog) -> String {
    let last_event = event_log
        .records
        .last()
        .map(|event| event.kind.as_str())
        .unwrap_or("none");
    format!(
        "refresh completed\nstaleness={:?}\nbalances={}\npositions={}\nopen_order_groups={}\nlast_event={}",
        store.staleness,
        store.snapshot.balances.len(),
        store.snapshot.positions.len(),
        store.snapshot.open_orders.len(),
        last_event
    )
}

fn render_execution_summary(event_log: &EventLog) -> String {
    let Some(last_event) = event_log.records.last() else {
        return "execution completed\nlast_event=none".to_string();
    };

    if last_event.kind != "app.execution.completed" {
        return format!("execution completed\nlast_event={}", last_event.kind);
    }

    match last_event.payload["command_kind"].as_str() {
        Some("set_target_exposure") => format!(
            "execution completed\ncommand=set-target-exposure\ninstrument={}\ntarget={}\noutcome={}",
            last_event.payload["instrument"].as_str().unwrap_or("unknown"),
            last_event.payload["target"].as_f64().unwrap_or_default(),
            last_event.payload["outcome_kind"].as_str().unwrap_or("unknown"),
        ),
        Some("close_symbol") => format!(
            "execution completed\ncommand=close-symbol\ninstrument={}\noutcome={}",
            last_event.payload["instrument"].as_str().unwrap_or("unknown"),
            last_event.payload["outcome_kind"].as_str().unwrap_or("unknown"),
        ),
        Some("close_all") => format!(
            "execution completed\ncommand=close-all\nbatch_id={}\nsubmitted={}\nskipped={}\nrejected={}\noutcome={}",
            last_event.payload["batch_id"].as_u64().unwrap_or_default(),
            last_event.payload["submitted"].as_u64().unwrap_or_default(),
            last_event.payload["skipped"].as_u64().unwrap_or_default(),
            last_event.payload["rejected"].as_u64().unwrap_or_default(),
            last_event.payload["outcome_kind"].as_str().unwrap_or("unknown"),
        ),
        _ => format!("execution completed\nlast_event={}", last_event.kind),
    }
}
