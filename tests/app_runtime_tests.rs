use sandbox_quant::app::bootstrap::AppBootstrap;
use sandbox_quant::app::commands::AppCommand;
use sandbox_quant::app::runtime::AppRuntime;
use sandbox_quant::domain::balance::BalanceSnapshot;
use sandbox_quant::domain::exposure::Exposure;
use sandbox_quant::domain::identifiers::OrderId;
use sandbox_quant::domain::instrument::Instrument;
use sandbox_quant::domain::market::Market;
use sandbox_quant::domain::order::{OpenOrder, OrderStatus};
use sandbox_quant::domain::position::PositionSnapshot;
use sandbox_quant::exchange::fake::FakeExchange;
use sandbox_quant::exchange::symbol_rules::SymbolRules;
use sandbox_quant::exchange::types::AuthoritativeSnapshot;
use sandbox_quant::execution::command::{CommandSource, ExecutionCommand};
use sandbox_quant::portfolio::store::PortfolioStateStore;

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
                source: CommandSource::User,
            }),
        )
        .expect("execution command should succeed");

    assert_eq!(app.event_log.records.len(), 2);
    assert_eq!(app.event_log.records[0].kind, "app.market_data.price_refreshed");
    assert_eq!(app.event_log.records[1].kind, "app.execution.completed");
    assert_eq!(app.event_log.records[0].payload["price"], 50000.0);
}

#[test]
fn app_runtime_refreshes_portfolio_and_logs_event() {
    let exchange = FakeExchange::new(sample_snapshot());
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
