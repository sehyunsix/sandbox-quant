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
    let last_event = event_log
        .records
        .last()
        .map(|event| event.kind.as_str())
        .unwrap_or("none");
    format!("execution completed\nlast_event={last_event}")
}
