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
    let mut lines = vec![
        "refresh completed".to_string(),
        format!("staleness={:?}", store.staleness),
        format!("last_event={last_event}"),
        format!("balances ({})", store.snapshot.balances.len()),
    ];

    let balance_lines = store
        .snapshot
        .balances
        .iter()
        .filter(|balance| balance.total().abs() > f64::EPSILON)
        .take(8)
        .map(|balance| {
            format!(
                "  - {} free={:.8} locked={:.8} total={:.8}",
                balance.asset,
                balance.free,
                balance.locked,
                balance.total()
            )
        })
        .collect::<Vec<_>>();

    if balance_lines.is_empty() {
        lines.push("  - none".to_string());
    } else {
        lines.extend(balance_lines);
    }

    lines.push(format!("positions ({})", store.snapshot.positions.len()));
    let position_lines = store
        .snapshot
        .positions
        .values()
        .take(12)
        .map(|position| {
            let side = position
                .side()
                .map(|side| format!("{side:?}"))
                .unwrap_or_else(|| "Flat".to_string());
            format!(
                "  - {} {:?} side={} qty={:.8} entry={}",
                position.instrument.0,
                position.market,
                side,
                position.abs_qty(),
                position
                    .entry_price
                    .map(|price| format!("{price:.8}"))
                    .unwrap_or_else(|| "-".to_string())
            )
        })
        .collect::<Vec<_>>();

    if position_lines.is_empty() {
        lines.push("  - none".to_string());
    } else {
        lines.extend(position_lines);
    }

    lines.push(format!("open orders ({})", store.snapshot.open_orders.len()));
    let order_lines = store
        .snapshot
        .open_orders
        .iter()
        .take(12)
        .flat_map(|(instrument, orders)| {
            orders.iter().map(move |order| {
                format!(
                    "  - {} {:?} side={:?} qty={:.8} filled={:.8} reduce_only={} status={:?}",
                    instrument.0,
                    order.market,
                    order.side,
                    order.orig_qty,
                    order.executed_qty,
                    order.reduce_only,
                    order.status
                )
            })
        })
        .collect::<Vec<_>>();

    if order_lines.is_empty() {
        lines.push("  - none".to_string());
    } else {
        lines.extend(order_lines);
    }

    lines.join("\n")
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
