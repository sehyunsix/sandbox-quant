use sandbox_quant::app::bootstrap::{AppBootstrap, BinanceMode};
use sandbox_quant::app::commands::AppCommand;
use sandbox_quant::app::runtime::AppRuntime;
use sandbox_quant::domain::balance::BalanceSnapshot;
use sandbox_quant::domain::exposure::Exposure;
use sandbox_quant::domain::identifiers::OrderId;
use sandbox_quant::domain::instrument::Instrument;
use sandbox_quant::domain::market::Market;
use sandbox_quant::domain::order::{OpenOrder, OrderStatus};
use sandbox_quant::domain::order_type::OrderType;
use sandbox_quant::domain::position::PositionSnapshot;
use sandbox_quant::domain::position::Side;
use sandbox_quant::error::exchange_error::ExchangeError;
use sandbox_quant::exchange::fake::FakeExchange;
use sandbox_quant::exchange::symbol_rules::SymbolRules;
use sandbox_quant::exchange::types::AuthoritativeSnapshot;
use sandbox_quant::execution::command::{CommandSource, ExecutionCommand};
use sandbox_quant::portfolio::store::PortfolioStateStore;
use sandbox_quant::record::coordination::RecorderCoordination;
use sandbox_quant::strategy::command::{StrategyCommand, StrategyStartConfig};
use sandbox_quant::strategy::model::{StrategyTemplate, StrategyWatchState};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn sample_snapshot() -> AuthoritativeSnapshot {
    let instrument = Instrument::new("BTCUSDT");
    AuthoritativeSnapshot {
        balances: vec![BalanceSnapshot {
            asset: "USDT".to_string(),
            free: 1000.0,
            locked: 0.0,
        }],
        positions: vec![PositionSnapshot {
            instrument: instrument.clone(),
            market: Market::Futures,
            signed_qty: -0.3,
            entry_price: Some(50000.0),
        }],
        open_orders: vec![OpenOrder {
            order_id: Some(OrderId(1)),
            client_order_id: "close-1".to_string(),
            instrument,
            market: Market::Futures,
            side: sandbox_quant::domain::position::Side::Sell,
            orig_qty: 0.3,
            executed_qty: 0.0,
            reduce_only: true,
            status: OrderStatus::Submitted,
        }],
    }
}

fn unique_test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("sandbox-quant-{name}-{nanos}"))
}

#[test]
fn app_runtime_executes_command_and_logs_event() {
    let instrument = Instrument::new("BTCUSDT");
    let exchange = FakeExchange::new(sample_snapshot());
    exchange.set_symbol_rules(
        instrument.clone(),
        Market::Futures,
        SymbolRules {
            min_qty: 0.001,
            max_qty: 100.0,
            step_size: 0.001,
        },
    );
    exchange.set_last_price(instrument.clone(), Market::Futures, 50000.0);

    let mut app = AppBootstrap::new(exchange, PortfolioStateStore::default());
    app.portfolio_store
        .refresh_from_exchange(&app.exchange)
        .expect("seed snapshot");

    let mut runtime = AppRuntime::default();
    runtime
        .run(
            &mut app,
            AppCommand::Execution(ExecutionCommand::SetTargetExposure {
                instrument,
                target: Exposure::new(0.5).expect("bounded exposure"),
                order_type: OrderType::Market,
                source: CommandSource::User,
            }),
        )
        .expect("execution command should succeed");

    assert_eq!(app.event_log.records.len(), 5);
    assert_eq!(app.event_log.records[0].kind, "app.portfolio.refreshed");
    assert_eq!(
        app.event_log.records[1].kind,
        "app.market_data.price_refreshed"
    );
    assert_eq!(app.event_log.records[2].kind, "app.execution.started");
    assert_eq!(app.event_log.records[3].kind, "app.portfolio.refreshed");
    assert_eq!(app.event_log.records[4].kind, "app.execution.completed");
    assert_eq!(app.event_log.records[1].payload["price"], 50000.0);
}

#[test]
fn app_runtime_refreshes_portfolio_and_logs_event() {
    let exchange = FakeExchange::new(sample_snapshot());
    exchange.set_last_price(Instrument::new("BTCUSDT"), Market::Futures, 50000.0);
    let mut app = AppBootstrap::new(exchange, PortfolioStateStore::default());
    let mut runtime = AppRuntime::default();

    runtime
        .run(&mut app, AppCommand::RefreshAuthoritativeState)
        .expect("refresh should succeed");

    assert_eq!(app.portfolio_store.snapshot.positions.len(), 1);
    assert_eq!(app.event_log.records.len(), 1);
    assert_eq!(app.event_log.records[0].kind, "app.portfolio.refreshed");
    assert_eq!(app.event_log.records[0].payload["positions"], 1);
}

#[test]
fn app_runtime_executes_target_exposure_from_flat_position() {
    let instrument = Instrument::new("BTCUSDT");
    let exchange = FakeExchange::new(AuthoritativeSnapshot {
        balances: vec![BalanceSnapshot {
            asset: "USDT".to_string(),
            free: 1000.0,
            locked: 0.0,
        }],
        positions: vec![],
        open_orders: vec![],
    });
    exchange.set_symbol_rules(
        instrument.clone(),
        Market::Futures,
        SymbolRules {
            min_qty: 0.001,
            max_qty: 100.0,
            step_size: 0.001,
        },
    );
    exchange.set_last_price(instrument.clone(), Market::Futures, 50000.0);

    let mut app = AppBootstrap::new(exchange, PortfolioStateStore::default());
    app.portfolio_store
        .refresh_from_exchange(&app.exchange)
        .expect("seed snapshot");

    let mut runtime = AppRuntime::default();
    runtime
        .run(
            &mut app,
            AppCommand::Execution(ExecutionCommand::SetTargetExposure {
                instrument,
                target: Exposure::new(0.5).expect("bounded exposure"),
                order_type: OrderType::Market,
                source: CommandSource::User,
            }),
        )
        .expect("flat target exposure should succeed");

    assert_eq!(app.event_log.records.len(), 5);
    assert_eq!(app.event_log.records[0].kind, "app.portfolio.refreshed");
    assert_eq!(
        app.event_log.records[1].kind,
        "app.market_data.price_refreshed"
    );
    assert_eq!(app.event_log.records[2].kind, "app.execution.started");
    assert_eq!(app.event_log.records[3].kind, "app.portfolio.refreshed");
    assert_eq!(app.event_log.records[4].kind, "app.execution.completed");
}

#[test]
fn app_runtime_surfaces_exchange_submit_failure_detail() {
    let instrument = Instrument::new("BTCUSDT");
    let exchange = FakeExchange::new(AuthoritativeSnapshot {
        balances: vec![BalanceSnapshot {
            asset: "USDT".to_string(),
            free: 1000.0,
            locked: 0.0,
        }],
        positions: vec![],
        open_orders: vec![],
    });
    exchange.set_symbol_rules(
        instrument.clone(),
        Market::Futures,
        SymbolRules {
            min_qty: 0.001,
            max_qty: 100.0,
            step_size: 0.001,
        },
    );
    exchange.set_last_price(instrument.clone(), Market::Futures, 50000.0);
    exchange.set_next_order_submit_result(Err(ExchangeError::RemoteReject {
        code: -2010,
        message: "insufficient margin".to_string(),
    }));

    let mut app = AppBootstrap::new(exchange, PortfolioStateStore::default());
    app.portfolio_store
        .refresh_from_exchange(&app.exchange)
        .expect("seed snapshot");

    let mut runtime = AppRuntime::default();
    let error = runtime
        .run(
            &mut app,
            AppCommand::Execution(ExecutionCommand::SetTargetExposure {
                instrument,
                target: Exposure::new(0.5).expect("bounded exposure"),
                order_type: OrderType::Market,
                source: CommandSource::User,
            }),
        )
        .expect_err("submit failure should surface");

    assert_eq!(
        error.to_string(),
        "execution error: exchange submit failed: remote rejected request: code=-2010 message=insufficient margin"
    );
}

#[test]
fn app_runtime_executes_option_order_and_logs_event() {
    let instrument = Instrument::new("BTC-260327-200000-C");
    let exchange = FakeExchange::new(AuthoritativeSnapshot {
        balances: vec![BalanceSnapshot {
            asset: "USDT".to_string(),
            free: 1000.0,
            locked: 0.0,
        }],
        positions: vec![],
        open_orders: vec![],
    });
    exchange.set_symbol_rules(
        instrument.clone(),
        Market::Options,
        SymbolRules {
            min_qty: 0.01,
            max_qty: 100.0,
            step_size: 0.01,
        },
    );

    let mut app = AppBootstrap::new(exchange, PortfolioStateStore::default());
    app.portfolio_store
        .refresh_from_exchange(&app.exchange)
        .expect("seed snapshot");

    let mut runtime = AppRuntime::default();
    runtime
        .run(
            &mut app,
            AppCommand::Execution(ExecutionCommand::SubmitOptionOrder {
                instrument: instrument.clone(),
                side: Side::Buy,
                qty: 0.01,
                order_type: OrderType::Limit { price: 5.0 },
                source: CommandSource::User,
            }),
        )
        .expect("option order should succeed");

    assert_eq!(app.event_log.records[0].kind, "app.portfolio.refreshed");
    assert_eq!(app.event_log.records[1].kind, "app.execution.started");
    assert_eq!(app.event_log.records[2].kind, "app.portfolio.refreshed");
    assert_eq!(app.event_log.records[3].kind, "app.execution.completed");
    assert_eq!(
        app.event_log.records[3].payload["command_kind"],
        "submit_option_order"
    );
}

#[test]
fn app_runtime_starts_strategy_watch_and_logs_event() {
    let instrument = Instrument::new("BTCUSDT");
    let exchange = FakeExchange::new(AuthoritativeSnapshot {
        balances: vec![],
        positions: vec![],
        open_orders: vec![],
    });
    exchange.set_symbol_rules(
        instrument.clone(),
        Market::Futures,
        SymbolRules {
            min_qty: 0.001,
            max_qty: 100.0,
            step_size: 0.001,
        },
    );
    let mut app = AppBootstrap::new(exchange, PortfolioStateStore::default());
    app.recorder_coordination = RecorderCoordination::new(unique_test_dir("strategy-start"));
    let mut runtime = AppRuntime::default();

    runtime
        .run(
            &mut app,
            AppCommand::Strategy(StrategyCommand::Start {
                template: StrategyTemplate::LiquidationBreakdownShort,
                instrument: instrument.clone(),
                config: StrategyStartConfig {
                    risk_pct: 0.005,
                    win_rate: 0.8,
                    r_multiple: 1.5,
                    max_entry_slippage_pct: 0.001,
                },
            }),
        )
        .expect("strategy start should succeed");

    assert_eq!(app.event_log.records.len(), 1);
    assert_eq!(app.event_log.records[0].kind, "app.strategy.watch_started");
    assert_eq!(app.event_log.records[0].payload["instrument"], "BTCUSDT");
    let watch = app
        .strategy_store
        .get(BinanceMode::Demo, 1)
        .expect("watch stored");
    assert_eq!(watch.state, StrategyWatchState::Armed);
}

#[test]
fn app_runtime_stops_strategy_watch_and_moves_it_to_history() {
    let instrument = Instrument::new("BTCUSDT");
    let exchange = FakeExchange::new(AuthoritativeSnapshot {
        balances: vec![],
        positions: vec![],
        open_orders: vec![],
    });
    exchange.set_symbol_rules(
        instrument.clone(),
        Market::Futures,
        SymbolRules {
            min_qty: 0.001,
            max_qty: 100.0,
            step_size: 0.001,
        },
    );
    let mut app = AppBootstrap::new(exchange, PortfolioStateStore::default());
    app.recorder_coordination = RecorderCoordination::new(unique_test_dir("strategy-stop"));
    let mut runtime = AppRuntime::default();

    runtime
        .run(
            &mut app,
            AppCommand::Strategy(StrategyCommand::Start {
                template: StrategyTemplate::LiquidationBreakdownShort,
                instrument,
                config: StrategyStartConfig {
                    risk_pct: 0.005,
                    win_rate: 0.8,
                    r_multiple: 1.5,
                    max_entry_slippage_pct: 0.001,
                },
            }),
        )
        .expect("start should succeed");
    runtime
        .run(
            &mut app,
            AppCommand::Strategy(StrategyCommand::Stop { watch_id: 1 }),
        )
        .expect("stop should succeed");

    assert_eq!(app.event_log.records[1].kind, "app.strategy.watch_stopped");
    assert!(app
        .strategy_store
        .active_watches(BinanceMode::Demo)
        .is_empty());
    assert_eq!(app.strategy_store.history(BinanceMode::Demo).len(), 1);
}

#[test]
fn app_runtime_separates_strategy_watches_by_mode() {
    let instrument = Instrument::new("BTCUSDT");
    let exchange = FakeExchange::new(AuthoritativeSnapshot {
        balances: vec![],
        positions: vec![],
        open_orders: vec![],
    });
    exchange.set_symbol_rules(
        instrument.clone(),
        Market::Futures,
        SymbolRules {
            min_qty: 0.001,
            max_qty: 100.0,
            step_size: 0.001,
        },
    );
    let mut app = AppBootstrap::new(exchange, PortfolioStateStore::default());
    app.recorder_coordination = RecorderCoordination::new(unique_test_dir("strategy-mode-split"));
    let mut runtime = AppRuntime::default();

    runtime
        .run(
            &mut app,
            AppCommand::Strategy(StrategyCommand::Start {
                template: StrategyTemplate::LiquidationBreakdownShort,
                instrument: instrument.clone(),
                config: StrategyStartConfig {
                    risk_pct: 0.005,
                    win_rate: 0.8,
                    r_multiple: 1.5,
                    max_entry_slippage_pct: 0.001,
                },
            }),
        )
        .expect("demo start should succeed");

    app.mode = BinanceMode::Real;
    runtime
        .run(
            &mut app,
            AppCommand::Strategy(StrategyCommand::Start {
                template: StrategyTemplate::LiquidationBreakdownShort,
                instrument,
                config: StrategyStartConfig {
                    risk_pct: 0.005,
                    win_rate: 0.8,
                    r_multiple: 1.5,
                    max_entry_slippage_pct: 0.001,
                },
            }),
        )
        .expect("real start should succeed");

    assert_eq!(
        app.strategy_store.active_watches(BinanceMode::Demo).len(),
        1
    );
    assert_eq!(
        app.strategy_store.active_watches(BinanceMode::Real).len(),
        1
    );
}

#[test]
fn app_runtime_refreshes_after_close_all_and_reports_remaining_positions() {
    let instrument = Instrument::new("BTCUSDT");
    let exchange = FakeExchange::new(AuthoritativeSnapshot {
        balances: vec![BalanceSnapshot {
            asset: "USDT".to_string(),
            free: 1000.0,
            locked: 0.0,
        }],
        positions: vec![PositionSnapshot {
            instrument: instrument.clone(),
            market: Market::Futures,
            signed_qty: -0.3,
            entry_price: Some(50000.0),
        }],
        open_orders: vec![],
    });
    exchange.set_symbol_rules(
        instrument.clone(),
        Market::Futures,
        SymbolRules {
            min_qty: 0.001,
            max_qty: 100.0,
            step_size: 0.001,
        },
    );
    let mut app = AppBootstrap::new(exchange, PortfolioStateStore::default());
    app.portfolio_store
        .refresh_from_exchange(&app.exchange)
        .expect("seed snapshot");
    app.exchange.replace_snapshot(AuthoritativeSnapshot {
        balances: vec![BalanceSnapshot {
            asset: "USDT".to_string(),
            free: 1000.0,
            locked: 0.0,
        }],
        positions: vec![],
        open_orders: vec![],
    });

    let mut runtime = AppRuntime::default();
    runtime
        .run(
            &mut app,
            AppCommand::Execution(ExecutionCommand::CloseAll {
                source: CommandSource::User,
            }),
        )
        .expect("close-all should succeed");

    assert_eq!(app.event_log.records[1].kind, "app.execution.started");
    assert_eq!(app.event_log.records[2].kind, "app.portfolio.refreshed");
    assert_eq!(app.event_log.records[3].kind, "app.execution.completed");
    assert_eq!(app.event_log.records[3].payload["remaining_positions"], 0);
}
