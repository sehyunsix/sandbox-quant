use crate::app::commands::AppCommand;
use crate::portfolio::store::PortfolioStateStore;
use crate::storage::event_log::EventLog;
use std::collections::BTreeMap;

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
    let aggregated_balances = aggregate_visible_balances(store);
    let visible_positions = store
        .snapshot
        .positions
        .values()
        .filter(|position| !position.is_flat())
        .collect::<Vec<_>>();

    let mut lines = vec![
        "refresh completed".to_string(),
        format!("staleness={:?}", store.staleness),
        format!("last_event={last_event}"),
        format!("balances ({})", aggregated_balances.len()),
    ];

    let balance_lines = aggregated_balances
        .iter()
        .take(8)
        .map(|(asset, balance)| {
            format!(
                "  - {} free={:.8} locked={:.8} total={:.8}",
                asset,
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

    lines.push(format!("positions ({})", visible_positions.len()));
    let position_lines = visible_positions
        .into_iter()
        .take(12)
        .map(|position| {
            let side = position
                .side()
                .map(|side| format!("{side:?}"))
                .unwrap_or_else(|| "Flat".to_string());
            let market = match position.market {
                crate::domain::market::Market::Spot => "SPOT",
                crate::domain::market::Market::Futures => "FUTURES",
            };
            format!(
                "  - {} market={} side={} qty={:.8} entry={}",
                position.instrument.0,
                market,
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

fn aggregate_visible_balances(
    store: &PortfolioStateStore,
) -> BTreeMap<String, crate::domain::balance::BalanceSnapshot> {
    let mut aggregated = BTreeMap::new();

    for balance in store
        .snapshot
        .balances
        .iter()
        .filter(|balance| balance.total().abs() > f64::EPSILON)
    {
        let entry = aggregated
            .entry(balance.asset.clone())
            .or_insert(crate::domain::balance::BalanceSnapshot {
                asset: balance.asset.clone(),
                free: 0.0,
                locked: 0.0,
            });
        entry.free += balance.free;
        entry.locked += balance.locked;
    }

    aggregated
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
