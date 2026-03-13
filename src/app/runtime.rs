use crate::app::bootstrap::AppBootstrap;
use crate::app::commands::AppCommand;
use crate::domain::instrument::Instrument;
use crate::domain::market::Market;
use crate::execution::command::ExecutionCommand;
use crate::execution::price_source::PriceSource;
use crate::storage::event_log::log;
use crate::strategy::command::StrategyCommand;
use serde_json::json;

#[derive(Debug, Default)]
pub struct AppRuntime {
    pub last_command: Option<AppCommand>,
}

impl AppRuntime {
    pub fn record_command(&mut self, command: AppCommand) {
        self.last_command = Some(command);
    }

    pub fn run<
        E: crate::exchange::facade::ExchangeFacade<
            Error = crate::error::exchange_error::ExchangeError,
        >,
    >(
        &mut self,
        app: &mut AppBootstrap<E>,
        command: AppCommand,
    ) -> Result<(), crate::error::app_error::AppError> {
        self.record_command(command.clone());

        match command {
            AppCommand::Portfolio(_) => {
                let report = app
                    .portfolio_sync
                    .refresh_authoritative(&app.exchange, &mut app.portfolio_store)?;
                refresh_position_prices(app)?;
                let today_realized_pnl_usdt = app.exchange.load_today_realized_pnl_usdt().ok();
                let today_funding_pnl_usdt = app.exchange.load_today_funding_pnl_usdt().ok();
                let margin_ratio = app.exchange.load_margin_ratio().ok().flatten();
                log(
                    &mut app.event_log,
                    "app.portfolio.refreshed",
                    json!({
                        "positions": report.positions,
                        "open_order_groups": report.open_order_groups,
                        "balances": report.balances,
                        "today_realized_pnl_usdt": today_realized_pnl_usdt,
                        "today_funding_pnl_usdt": today_funding_pnl_usdt,
                        "margin_ratio": margin_ratio,
                    }),
                );
            }
            AppCommand::Execution(command) => {
                let report = app
                    .portfolio_sync
                    .refresh_authoritative(&app.exchange, &mut app.portfolio_store)?;
                log(
                    &mut app.event_log,
                    "app.portfolio.refreshed",
                    json!({
                        "positions": report.positions,
                        "open_order_groups": report.open_order_groups,
                        "balances": report.balances,
                    }),
                );

                if let ExecutionCommand::SetTargetExposure { instrument, .. } = &command {
                    let market = app
                        .portfolio_store
                        .snapshot
                        .positions
                        .get(instrument)
                        .map(|position| position.market)
                        .or_else(|| {
                            if app
                                .exchange
                                .load_symbol_rules(
                                    instrument,
                                    crate::domain::market::Market::Futures,
                                )
                                .is_ok()
                            {
                                Some(crate::domain::market::Market::Futures)
                            } else if app
                                .exchange
                                .load_symbol_rules(instrument, crate::domain::market::Market::Spot)
                                .is_ok()
                            {
                                Some(crate::domain::market::Market::Spot)
                            } else {
                                None
                            }
                        });

                    if let Some(market) = market {
                        let price = app.market_data.refresh_price(
                            &app.exchange,
                            &mut app.price_store,
                            instrument.clone(),
                            market,
                        )?;
                        log(
                            &mut app.event_log,
                            "app.market_data.price_refreshed",
                            json!({
                                "instrument": instrument.0,
                                "market": format!("{market:?}"),
                                "price": price,
                            }),
                        );
                    }
                }
                let outcome = app.execution.execute(
                    &app.exchange,
                    &app.portfolio_store,
                    &app.price_store,
                    command.clone(),
                )?;

                let post_report = app
                    .portfolio_sync
                    .refresh_authoritative(&app.exchange, &mut app.portfolio_store)?;
                refresh_position_prices(app)?;
                log(
                    &mut app.event_log,
                    "app.portfolio.refreshed",
                    json!({
                        "positions": post_report.positions,
                        "open_order_groups": post_report.open_order_groups,
                        "balances": post_report.balances,
                        "phase": "post_execution",
                    }),
                );
                log(
                    &mut app.event_log,
                    "app.execution.completed",
                    execution_payload(
                        &command,
                        &outcome,
                        post_report.positions,
                        remaining_gross_exposure_usdt(&app.portfolio_store, &app.price_store),
                    ),
                );
            }
            AppCommand::Strategy(command) => match command {
                StrategyCommand::Templates | StrategyCommand::List | StrategyCommand::History => {}
                StrategyCommand::Show { watch_id } => {
                    app.strategy_store.get(app.mode, watch_id).ok_or(
                        crate::error::strategy_error::StrategyError::WatchNotFound(watch_id),
                    )?;
                }
                StrategyCommand::Start {
                    template,
                    instrument,
                    config,
                } => {
                    app.exchange
                        .load_symbol_rules(&instrument, Market::Futures)?;
                    let watch = app.strategy_store.create_watch(
                        app.mode,
                        template,
                        instrument.clone(),
                        config.clone(),
                    )?;
                    app.recorder_coordination.sync_strategy_symbols(
                        app.mode,
                        active_strategy_symbols(&app.strategy_store, app.mode),
                    )?;
                    log(
                        &mut app.event_log,
                        "app.strategy.watch_started",
                        json!({
                            "watch_id": watch.id,
                            "mode": format!("{:?}", watch.mode).to_ascii_lowercase(),
                            "template": watch.template.slug(),
                            "instrument": watch.instrument.0,
                            "state": watch.state.as_str(),
                            "risk_pct": watch.config.risk_pct,
                            "win_rate": watch.config.win_rate,
                            "r_multiple": watch.config.r_multiple,
                            "max_entry_slippage_pct": watch.config.max_entry_slippage_pct,
                            "current_step": watch.current_step,
                        }),
                    );
                }
                StrategyCommand::Stop { watch_id } => {
                    let watch = app.strategy_store.stop_watch(app.mode, watch_id)?;
                    app.recorder_coordination.sync_strategy_symbols(
                        app.mode,
                        active_strategy_symbols(&app.strategy_store, app.mode),
                    )?;
                    log(
                        &mut app.event_log,
                        "app.strategy.watch_stopped",
                        json!({
                            "watch_id": watch.id,
                            "mode": format!("{:?}", watch.mode).to_ascii_lowercase(),
                            "template": watch.template.slug(),
                            "instrument": watch.instrument.0,
                            "state": watch.state.as_str(),
                        }),
                    );
                }
            },
            AppCommand::RefreshAuthoritativeState => {
                let report = app
                    .portfolio_sync
                    .refresh_authoritative(&app.exchange, &mut app.portfolio_store)?;
                refresh_position_prices(app)?;
                let today_realized_pnl_usdt = app.exchange.load_today_realized_pnl_usdt().ok();
                let today_funding_pnl_usdt = app.exchange.load_today_funding_pnl_usdt().ok();
                let margin_ratio = app.exchange.load_margin_ratio().ok().flatten();
                log(
                    &mut app.event_log,
                    "app.portfolio.refreshed",
                    json!({
                        "positions": report.positions,
                        "open_order_groups": report.open_order_groups,
                        "balances": report.balances,
                        "today_realized_pnl_usdt": today_realized_pnl_usdt,
                        "today_funding_pnl_usdt": today_funding_pnl_usdt,
                        "margin_ratio": margin_ratio,
                    }),
                );
            }
        }

        Ok(())
    }
}

fn active_strategy_symbols(
    store: &crate::strategy::store::StrategyStore,
    mode: crate::app::bootstrap::BinanceMode,
) -> Vec<String> {
    store
        .active_watches(mode)
        .into_iter()
        .map(|watch| watch.instrument.0.clone())
        .collect()
}

fn refresh_position_prices<
    E: crate::exchange::facade::ExchangeFacade<Error = crate::error::exchange_error::ExchangeError>,
>(
    app: &mut AppBootstrap<E>,
) -> Result<(), crate::error::exchange_error::ExchangeError> {
    let instruments = app
        .portfolio_store
        .snapshot
        .positions
        .values()
        .map(|position| (position.instrument.clone(), position.market))
        .collect::<Vec<(Instrument, crate::domain::market::Market)>>();

    for (instrument, market) in instruments {
        app.market_data
            .refresh_price(&app.exchange, &mut app.price_store, instrument, market)?;
    }

    Ok(())
}

fn execution_payload(
    command: &ExecutionCommand,
    outcome: &crate::execution::service::ExecutionOutcome,
    remaining_positions: usize,
    remaining_gross_exposure_usdt: f64,
) -> serde_json::Value {
    match (command, outcome) {
        (
            ExecutionCommand::SetTargetExposure {
                instrument,
                target,
                order_type,
                ..
            },
            crate::execution::service::ExecutionOutcome::TargetExposureSubmitted { .. },
        ) => json!({
            "command_kind": "set_target_exposure",
            "instrument": instrument.0,
            "target": target.value(),
            "order_type": format_order_type(*order_type),
            "outcome_kind": "submitted",
            "remaining_positions": remaining_positions,
            "flat_confirmed": remaining_positions == 0,
            "remaining_gross_exposure_usdt": remaining_gross_exposure_usdt,
        }),
        (
            ExecutionCommand::SetTargetExposure {
                instrument,
                target,
                order_type,
                ..
            },
            crate::execution::service::ExecutionOutcome::TargetExposureAlreadyAtTarget { .. },
        ) => json!({
            "command_kind": "set_target_exposure",
            "instrument": instrument.0,
            "target": target.value(),
            "order_type": format_order_type(*order_type),
            "outcome_kind": "already-at-target",
            "remaining_positions": remaining_positions,
            "flat_confirmed": remaining_positions == 0,
            "remaining_gross_exposure_usdt": remaining_gross_exposure_usdt,
        }),
        (
            ExecutionCommand::SubmitOptionOrder {
                instrument,
                side,
                qty,
                order_type,
                ..
            },
            crate::execution::service::ExecutionOutcome::OptionOrderSubmitted { .. },
        ) => json!({
            "command_kind": "submit_option_order",
            "instrument": instrument.0,
            "side": format!("{side:?}"),
            "qty": qty,
            "order_type": format_order_type(*order_type),
            "remaining_positions": remaining_positions,
            "flat_confirmed": remaining_positions == 0,
            "remaining_gross_exposure_usdt": remaining_gross_exposure_usdt,
            "outcome_kind": "submitted",
        }),
        (
            ExecutionCommand::CloseSymbol { instrument, .. },
            crate::execution::service::ExecutionOutcome::CloseSymbol(result),
        ) => json!({
            "command_kind": "close_symbol",
            "instrument": instrument.0,
            "outcome_kind": format!("{:?}", result.result),
            "remaining_positions": remaining_positions,
            "flat_confirmed": remaining_positions == 0,
            "remaining_gross_exposure_usdt": remaining_gross_exposure_usdt,
        }),
        (
            ExecutionCommand::CloseAll { .. },
            crate::execution::service::ExecutionOutcome::CloseAll(result),
        ) => {
            let submitted = result
                .results
                .iter()
                .filter(|item| {
                    matches!(
                        item.result,
                        crate::execution::close_symbol::CloseSubmitResult::Submitted
                    )
                })
                .count();
            let skipped = result
                .results
                .iter()
                .filter(|item| {
                    matches!(
                        item.result,
                        crate::execution::close_symbol::CloseSubmitResult::SkippedNoPosition
                    )
                })
                .count();
            let rejected = result
                .results
                .iter()
                .filter(|item| {
                    matches!(
                        item.result,
                        crate::execution::close_symbol::CloseSubmitResult::Rejected
                    )
                })
                .count();
            json!({
                "command_kind": "close_all",
                "batch_id": result.batch_id.0,
                "submitted": submitted,
                "skipped": skipped,
                "rejected": rejected,
                "remaining_positions": remaining_positions,
                "flat_confirmed": remaining_positions == 0,
                "remaining_gross_exposure_usdt": remaining_gross_exposure_usdt,
                "outcome_kind": "batch_completed",
            })
        }
        _ => json!({
            "command_kind": "unknown",
            "outcome_kind": "unknown",
        }),
    }
}

fn format_order_type(order_type: crate::domain::order_type::OrderType) -> String {
    match order_type {
        crate::domain::order_type::OrderType::Market => "market".to_string(),
        crate::domain::order_type::OrderType::Limit { price } => format!("limit@{price:.2}"),
    }
}

fn remaining_gross_exposure_usdt(
    store: &crate::portfolio::store::PortfolioStateStore,
    prices: &crate::market_data::price_store::PriceStore,
) -> f64 {
    store
        .snapshot
        .positions
        .values()
        .filter(|position| !position.is_flat())
        .filter(|position| position.market != crate::domain::market::Market::Options)
        .filter_map(|position| {
            let price = prices
                .current_price(&position.instrument)
                .or(position.entry_price)?;
            Some(position.abs_qty() * price)
        })
        .sum::<f64>()
}
