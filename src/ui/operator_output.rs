use crate::app::bootstrap::BinanceMode;
use crate::app::commands::{AppCommand, PortfolioView};
use crate::execution::price_source::PriceSource;
use crate::market_data::price_store::PriceStore;
use crate::portfolio::store::PortfolioStateStore;
use crate::storage::event_log::EventLog;
use crate::strategy::command::StrategyCommand;
use crate::strategy::store::StrategyStore;
use std::collections::BTreeMap;

pub fn render_command_output(
    command: &AppCommand,
    store: &PortfolioStateStore,
    prices: &PriceStore,
    event_log: &EventLog,
    strategy_store: &StrategyStore,
    mode: BinanceMode,
) -> String {
    match command {
        AppCommand::Portfolio(view) => render_portfolio_output(view, store, prices, event_log),
        AppCommand::RefreshAuthoritativeState => render_refresh_summary(store, prices, event_log),
        AppCommand::Execution(_) => render_execution_summary(event_log),
        AppCommand::Strategy(command) => {
            render_strategy_output(command, event_log, strategy_store, mode)
        }
    }
}

fn render_strategy_output(
    command: &StrategyCommand,
    event_log: &EventLog,
    store: &StrategyStore,
    mode: BinanceMode,
) -> String {
    match command {
        StrategyCommand::Templates => {
            let mut lines = vec![
                "strategy templates".to_string(),
                format!("mode={}", format_mode(mode)),
                "templates=1".to_string(),
                "template=liquidation-breakdown-short".to_string(),
            ];
            for (index, step) in crate::strategy::model::StrategyTemplate::LiquidationBreakdownShort
                .steps()
                .iter()
                .enumerate()
            {
                lines.push(format!("{}. {}", index + 1, step));
            }
            lines.join("\n")
        }
        StrategyCommand::List => {
            let watches = store.active_watches(mode);
            let mut lines = vec![
                "strategy watches".to_string(),
                format!("mode={}", format_mode(mode)),
                format!("active={}", watches.len()),
            ];
            if watches.is_empty() {
                lines.push("- none".to_string());
            } else {
                lines.extend(watches.into_iter().map(|watch| {
                    format!(
                        "- id={} template={} instrument={} state={} step={}/7",
                        watch.id,
                        watch.template.slug(),
                        watch.instrument.0,
                        watch.state.as_str(),
                        watch.current_step
                    )
                }));
            }
            lines.join("\n")
        }
        StrategyCommand::History => {
            let history = store.history(mode);
            let mut lines = vec![
                "strategy history".to_string(),
                format!("mode={}", format_mode(mode)),
                format!("runs={}", history.len()),
            ];
            if history.is_empty() {
                lines.push("- none".to_string());
            } else {
                lines.extend(history.iter().rev().take(10).map(|watch| {
                    format!(
                        "- id={} template={} instrument={} state={} updated_at={}",
                        watch.id,
                        watch.template.slug(),
                        watch.instrument.0,
                        watch.state.as_str(),
                        watch.updated_at.to_rfc3339(),
                    )
                }));
            }
            lines.join("\n")
        }
        StrategyCommand::Show { watch_id } => {
            let Some(watch) = store.get(mode, *watch_id) else {
                return format!(
                    "strategy watch\nmode={}\nwatch_id={watch_id}\nstate=missing",
                    format_mode(mode)
                );
            };
            let mut lines = vec![
                "strategy watch".to_string(),
                format!("mode={}", format_mode(mode)),
                format!("watch_id={}", watch.id),
                format!("template={}", watch.template.slug()),
                format!("instrument={}", watch.instrument.0),
                format!("state={}", watch.state.as_str()),
                format!("current_step={}/7", watch.current_step),
                format!("risk_pct={}", watch.config.risk_pct),
                format!("win_rate={}", watch.config.win_rate),
                format!("r_multiple={}", watch.config.r_multiple),
                format!(
                    "max_entry_slippage_pct={}",
                    watch.config.max_entry_slippage_pct
                ),
            ];
            for (index, step) in watch.template.steps().iter().enumerate() {
                let marker = if watch.current_step == index + 1 {
                    ">"
                } else {
                    "-"
                };
                lines.push(format!("{marker} {}. {}", index + 1, step));
            }
            lines.join("\n")
        }
        StrategyCommand::Start { .. } => {
            let Some(last_event) = event_log.records.last() else {
                return "strategy started\nlast_event=none".to_string();
            };
            format!(
                "strategy started\nmode={}\nwatch_id={}\ntemplate={}\ninstrument={}\nstate={}\nrisk_pct={}\nwin_rate={}\nr_multiple={}\nmax_entry_slippage_pct={}\ncurrent_step={}/7",
                last_event.payload["mode"].as_str().unwrap_or("unknown"),
                last_event.payload["watch_id"].as_u64().unwrap_or_default(),
                last_event.payload["template"].as_str().unwrap_or("unknown"),
                last_event.payload["instrument"].as_str().unwrap_or("unknown"),
                last_event.payload["state"].as_str().unwrap_or("unknown"),
                last_event.payload["risk_pct"].as_f64().unwrap_or_default(),
                last_event.payload["win_rate"].as_f64().unwrap_or_default(),
                last_event.payload["r_multiple"].as_f64().unwrap_or_default(),
                last_event.payload["max_entry_slippage_pct"].as_f64().unwrap_or_default(),
                last_event.payload["current_step"].as_u64().unwrap_or_default(),
            )
        }
        StrategyCommand::Stop { .. } => {
            let Some(last_event) = event_log.records.last() else {
                return "strategy stopped\nlast_event=none".to_string();
            };
            format!(
                "strategy stopped\nmode={}\nwatch_id={}\ntemplate={}\ninstrument={}\nstate={}",
                last_event.payload["mode"].as_str().unwrap_or("unknown"),
                last_event.payload["watch_id"].as_u64().unwrap_or_default(),
                last_event.payload["template"].as_str().unwrap_or("unknown"),
                last_event.payload["instrument"]
                    .as_str()
                    .unwrap_or("unknown"),
                last_event.payload["state"].as_str().unwrap_or("unknown"),
            )
        }
    }
}

fn format_mode(mode: BinanceMode) -> &'static str {
    match mode {
        BinanceMode::Real => "real",
        BinanceMode::Demo => "demo",
    }
}

fn render_portfolio_output(
    view: &PortfolioView,
    store: &PortfolioStateStore,
    prices: &PriceStore,
    event_log: &EventLog,
) -> String {
    match view {
        PortfolioView::Overview => render_refresh_summary_with_header(
            "portfolio",
            store,
            prices,
            event_log,
            true,
            true,
            true,
        ),
        PortfolioView::Positions => render_refresh_summary_with_header(
            "portfolio positions",
            store,
            prices,
            event_log,
            true,
            false,
            false,
        ),
        PortfolioView::Balances => render_refresh_summary_with_header(
            "portfolio balances",
            store,
            prices,
            event_log,
            false,
            true,
            false,
        ),
        PortfolioView::Orders => render_refresh_summary_with_header(
            "portfolio orders",
            store,
            prices,
            event_log,
            false,
            false,
            true,
        ),
    }
}

fn render_refresh_summary(
    store: &PortfolioStateStore,
    prices: &PriceStore,
    event_log: &EventLog,
) -> String {
    render_refresh_summary_with_header(
        "refresh completed",
        store,
        prices,
        event_log,
        true,
        true,
        true,
    )
}

fn render_refresh_summary_with_header(
    header: &str,
    store: &PortfolioStateStore,
    prices: &PriceStore,
    event_log: &EventLog,
    show_positions: bool,
    show_balances: bool,
    show_orders: bool,
) -> String {
    let last_event = event_log
        .records
        .last()
        .map(|event| event.kind.as_str())
        .unwrap_or("none");
    let latest_refresh = event_log
        .records
        .iter()
        .rev()
        .find(|event| event.kind == "app.portfolio.refreshed");
    let aggregated_balances = aggregate_visible_balances(store);
    let total_equity_usdt = aggregated_balances
        .values()
        .map(|balance| balance.total())
        .sum::<f64>();
    let available_quote_usdt = aggregated_balances
        .iter()
        .filter(|(asset, _)| asset.as_str() == "USDT" || asset.as_str() == "USDC")
        .map(|(_, balance)| balance.free)
        .sum::<f64>();
    let visible_positions = store
        .snapshot
        .positions
        .values()
        .filter(|position| !position.is_flat())
        .collect::<Vec<_>>();
    let gross_exposure_usdt = visible_positions
        .iter()
        .filter(|position| position.market != crate::domain::market::Market::Options)
        .filter_map(|position| {
            let price = prices
                .current_price(&position.instrument)
                .or(position.entry_price)?;
            Some(position.abs_qty() * price)
        })
        .sum::<f64>();
    let net_exposure_usdt = visible_positions
        .iter()
        .filter(|position| position.market != crate::domain::market::Market::Options)
        .filter_map(|position| {
            let price = prices
                .current_price(&position.instrument)
                .or(position.entry_price)?;
            Some(position.signed_qty * price)
        })
        .sum::<f64>();
    let unrealized_pnl_usdt = visible_positions
        .iter()
        .filter_map(|position| {
            let current_price = prices.current_price(&position.instrument)?;
            let entry_price = position.entry_price?;
            Some((current_price - entry_price) * position.signed_qty)
        })
        .sum::<f64>();
    let gross_exposure_usdt = normalize_display_value(gross_exposure_usdt);
    let net_exposure_usdt = normalize_display_value(net_exposure_usdt);
    let unrealized_pnl_usdt = normalize_display_value(unrealized_pnl_usdt);
    let leverage = if total_equity_usdt > f64::EPSILON {
        normalize_display_value(gross_exposure_usdt / total_equity_usdt)
    } else {
        0.0
    };
    let margin_ratio_text = if visible_positions.is_empty() && store.snapshot.open_orders.is_empty()
    {
        "n/a".to_string()
    } else {
        latest_refresh
            .and_then(|event| event.payload["margin_ratio"].as_f64())
            .map(|value| format!("{value:.4}"))
            .unwrap_or_else(|| "n/a".to_string())
    };

    let mut lines = vec![
        header.to_string(),
        format!("staleness={:?}", store.staleness),
        format!("last_event={last_event}"),
    ];

    if header == "portfolio" {
        lines.push("account".to_string());
        lines.push(format!("  total_equity_usdt={total_equity_usdt:.2}"));
        lines.push(format!("  available_quote_usdt={available_quote_usdt:.2}"));
        lines.push("risk".to_string());
        lines.push(format!("  positions={}", visible_positions.len()));
        lines.push(format!(
            "  open_orders={}",
            store.snapshot.open_orders.len()
        ));
        lines.push(format!("  gross_exposure_usdt={gross_exposure_usdt:.2}"));
        lines.push(format!("  net_exposure_usdt={net_exposure_usdt:.2}"));
        lines.push(format!("  leverage={leverage:.4}"));
        lines.push(format!("  margin_ratio={margin_ratio_text}"));
        lines.push("pnl".to_string());
        lines.push(format!("  unrealized_pnl_usdt={unrealized_pnl_usdt:.2}"));
        lines.push(format!(
            "  today_realized_pnl_usdt={}",
            latest_refresh
                .and_then(|event| event.payload["today_realized_pnl_usdt"].as_f64())
                .map(|value| format!("{value:.2}"))
                .unwrap_or_else(|| "n/a".to_string())
        ));
        lines.push(format!(
            "  today_funding_pnl_usdt={}",
            latest_refresh
                .and_then(|event| event.payload["today_funding_pnl_usdt"].as_f64())
                .map(|value| format!("{value:.2}"))
                .unwrap_or_else(|| "n/a".to_string())
        ));
    }

    if show_balances {
        lines.push(format!("balances ({})", aggregated_balances.len()));
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
    }

    if show_positions {
        lines.push(format!("positions ({})", visible_positions.len()));
        let position_lines = visible_positions
            .into_iter()
            .take(12)
            .map(|position| {
                let side = position
                    .side()
                    .map(|side| format!("{side:?}"))
                    .unwrap_or_else(|| "Flat".to_string());
                let market = format_market(position.market);
                let notional = if position.market == crate::domain::market::Market::Options {
                    None
                } else {
                    position.entry_price.map(|price| position.abs_qty() * price)
                };
                let exposure = notional.and_then(|notional| {
                    if total_equity_usdt > f64::EPSILON {
                        Some(notional / total_equity_usdt)
                    } else {
                        None
                    }
                });
                let target_exposure = if position.market == crate::domain::market::Market::Options {
                    None
                } else {
                    latest_target_exposure(event_log, &position.instrument)
                };
                let target_delta = match (target_exposure, exposure) {
                    (Some(target), Some(current)) => Some(target - current),
                    _ => None,
                };
                format!(
                    "  - {} market={} side={} qty={:.8} entry={} notional={} current_exposure={} target_exposure={} target_delta={}",
                    position.instrument.0,
                    market,
                    side,
                    position.abs_qty(),
                    position
                        .entry_price
                        .map(|price| format!("{price:.8}"))
                        .unwrap_or_else(|| "-".to_string()),
                    notional
                        .map(|value| format!("{value:.2}"))
                        .unwrap_or_else(|| "-".to_string()),
                    exposure
                        .map(|value| format!("{value:.4}"))
                        .unwrap_or_else(|| "-".to_string()),
                    target_exposure
                        .map(|value| format!("{value:.4}"))
                        .unwrap_or_else(|| "-".to_string()),
                    target_delta
                        .map(|value| format!("{value:.4}"))
                        .unwrap_or_else(|| "-".to_string()),
                )
            })
            .collect::<Vec<_>>();

        if position_lines.is_empty() {
            lines.push("  - none".to_string());
        } else {
            lines.extend(position_lines);
        }
    }

    if show_orders {
        lines.push(format!(
            "open orders ({})",
            store.snapshot.open_orders.len()
        ));
        let order_lines = store
            .snapshot
            .open_orders
            .iter()
            .take(12)
            .flat_map(|(instrument, orders)| {
                orders.iter().map(move |order| {
                    format!(
                        "  - {} {} side={:?} qty={:.8} filled={:.8} reduce_only={} status={:?}",
                        instrument.0,
                        format_market(order.market),
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
        let entry = aggregated.entry(balance.asset.clone()).or_insert(
            crate::domain::balance::BalanceSnapshot {
                asset: balance.asset.clone(),
                free: 0.0,
                locked: 0.0,
            },
        );
        entry.free += balance.free;
        entry.locked += balance.locked;
    }

    aggregated
}

fn normalize_display_value(value: f64) -> f64 {
    if value.abs() <= f64::EPSILON {
        0.0
    } else {
        value
    }
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
            "execution completed\ncommand=set-target-exposure\ninstrument={}\ntarget={}\norder_type={}\nremaining_positions={}\nflat_confirmed={}\nremaining_gross_exposure_usdt={:.2}\noutcome={}",
            last_event.payload["instrument"].as_str().unwrap_or("unknown"),
            last_event.payload["target"].as_f64().unwrap_or_default(),
            last_event.payload["order_type"].as_str().unwrap_or("unknown"),
            last_event.payload["remaining_positions"]
                .as_u64()
                .unwrap_or_default(),
            last_event.payload["flat_confirmed"].as_bool().unwrap_or(false),
            normalize_display_value(
                last_event.payload["remaining_gross_exposure_usdt"]
                    .as_f64()
                    .unwrap_or_default(),
            ),
            last_event.payload["outcome_kind"].as_str().unwrap_or("unknown"),
        ),
        Some("submit_option_order") => format!(
            "execution completed\ncommand=option-order\ninstrument={}\nside={}\nqty={}\norder_type={}\nremaining_positions={}\nflat_confirmed={}\nremaining_gross_exposure_usdt={:.2}\noutcome={}",
            last_event.payload["instrument"].as_str().unwrap_or("unknown"),
            last_event.payload["side"].as_str().unwrap_or("unknown"),
            last_event.payload["qty"].as_f64().unwrap_or_default(),
            last_event.payload["order_type"].as_str().unwrap_or("unknown"),
            last_event.payload["remaining_positions"]
                .as_u64()
                .unwrap_or_default(),
            last_event.payload["flat_confirmed"].as_bool().unwrap_or(false),
            normalize_display_value(
                last_event.payload["remaining_gross_exposure_usdt"]
                    .as_f64()
                    .unwrap_or_default(),
            ),
            last_event.payload["outcome_kind"].as_str().unwrap_or("unknown"),
        ),
        Some("close_symbol") => format!(
            "execution completed\ncommand=close-symbol\ninstrument={}\nremaining_positions={}\nflat_confirmed={}\nremaining_gross_exposure_usdt={:.2}\noutcome={}",
            last_event.payload["instrument"].as_str().unwrap_or("unknown"),
            last_event.payload["remaining_positions"]
                .as_u64()
                .unwrap_or_default(),
            last_event.payload["flat_confirmed"].as_bool().unwrap_or(false),
            normalize_display_value(
                last_event.payload["remaining_gross_exposure_usdt"]
                    .as_f64()
                    .unwrap_or_default(),
            ),
            last_event.payload["outcome_kind"].as_str().unwrap_or("unknown"),
        ),
        Some("close_all") => format!(
            "execution completed\ncommand=close-all\nbatch_id={}\nsubmitted={}\nskipped={}\nrejected={}\nremaining_positions={}\nflat_confirmed={}\nremaining_gross_exposure_usdt={:.2}\noutcome={}",
            last_event.payload["batch_id"].as_u64().unwrap_or_default(),
            last_event.payload["submitted"].as_u64().unwrap_or_default(),
            last_event.payload["skipped"].as_u64().unwrap_or_default(),
            last_event.payload["rejected"].as_u64().unwrap_or_default(),
            last_event.payload["remaining_positions"]
                .as_u64()
                .unwrap_or_default(),
            last_event.payload["flat_confirmed"].as_bool().unwrap_or(false),
            normalize_display_value(
                last_event.payload["remaining_gross_exposure_usdt"]
                    .as_f64()
                    .unwrap_or_default(),
            ),
            last_event.payload["outcome_kind"].as_str().unwrap_or("unknown"),
        ),
        _ => format!("execution completed\nlast_event={}", last_event.kind),
    }
}

fn latest_target_exposure(
    event_log: &EventLog,
    instrument: &crate::domain::instrument::Instrument,
) -> Option<f64> {
    event_log
        .records
        .iter()
        .rev()
        .find(|event| {
            event.kind == "app.execution.completed"
                && event.payload["command_kind"].as_str() == Some("set_target_exposure")
                && event.payload["instrument"].as_str() == Some(instrument.0.as_str())
        })
        .and_then(|event| event.payload["target"].as_f64())
}

fn format_market(market: crate::domain::market::Market) -> &'static str {
    match market {
        crate::domain::market::Market::Spot => "SPOT",
        crate::domain::market::Market::Futures => "FUTURES",
        crate::domain::market::Market::Options => "OPTIONS",
    }
}
