use sandbox_quant::app::bootstrap::BinanceMode;
use sandbox_quant::app::commands::{AppCommand, PortfolioView};
use sandbox_quant::app::output::render_command_output;
use sandbox_quant::domain::balance::BalanceSnapshot;
use sandbox_quant::domain::instrument::Instrument;
use sandbox_quant::domain::market::Market;
use sandbox_quant::domain::order::{OpenOrder, OrderStatus};
use sandbox_quant::domain::order_type::OrderType;
use sandbox_quant::domain::position::PositionSnapshot;
use sandbox_quant::domain::position::Side;
use sandbox_quant::execution::command::{CommandSource, ExecutionCommand};
use sandbox_quant::market_data::price_store::PriceStore;
use sandbox_quant::portfolio::store::PortfolioStateStore;
use sandbox_quant::storage::event_log::{log, EventLog};
use sandbox_quant::strategy::command::{StrategyCommand, StrategyStartConfig};
use sandbox_quant::strategy::model::StrategyTemplate;
use sandbox_quant::strategy::store::StrategyStore;
use serde_json::json;

#[test]
fn refresh_output_includes_store_summary() {
    let mut store = PortfolioStateStore::default();
    store.apply_snapshot(sandbox_quant::exchange::types::AuthoritativeSnapshot {
        balances: vec![BalanceSnapshot {
            asset: "USDT".to_string(),
            free: 100.0,
            locked: 10.0,
        }],
        positions: vec![
            PositionSnapshot {
                instrument: Instrument::new("BTCUSDT"),
                market: Market::Futures,
                signed_qty: 0.25,
                entry_price: Some(65000.0),
            },
            PositionSnapshot {
                instrument: Instrument::new("ETHUSDT"),
                market: Market::Futures,
                signed_qty: 0.0,
                entry_price: None,
            },
        ],
        open_orders: vec![OpenOrder {
            order_id: None,
            client_order_id: "close-1".to_string(),
            instrument: Instrument::new("BTCUSDT"),
            market: Market::Futures,
            side: sandbox_quant::domain::position::Side::Sell,
            orig_qty: 0.25,
            executed_qty: 0.0,
            reduce_only: true,
            status: OrderStatus::Submitted,
        }],
    });

    let mut event_log = EventLog::default();
    let prices = PriceStore::default();
    log(
        &mut event_log,
        "app.portfolio.refreshed",
        json!({
            "positions": 1,
            "today_realized_pnl_usdt": 12.34,
            "today_funding_pnl_usdt": 5.67,
            "margin_ratio": 0.1234
        }),
    );
    log(
        &mut event_log,
        "app.execution.completed",
        json!({
            "command_kind": "set_target_exposure",
            "instrument": "BTCUSDT",
            "target": 0.2500,
            "order_type": "market",
            "outcome_kind": "submitted",
        }),
    );

    let output = render_command_output(
        &AppCommand::Portfolio(PortfolioView::Overview),
        &store,
        &prices,
        &event_log,
        &StrategyStore::default(),
        BinanceMode::Demo,
    );

    assert!(output.contains("portfolio"));
    assert!(output.contains("account"));
    assert!(output.contains("total_equity_usdt=110.00"));
    assert!(output.contains("available_quote_usdt=100.00"));
    assert!(output.contains("risk"));
    assert!(output.contains("gross_exposure_usdt=16250.00"));
    assert!(output.contains("net_exposure_usdt=16250.00"));
    assert!(output.contains("leverage=147.7273"));
    assert!(output.contains("margin_ratio=0.1234"));
    assert!(output.contains("pnl"));
    assert!(output.contains("unrealized_pnl_usdt=0.00"));
    assert!(output.contains("today_realized_pnl_usdt=12.34"));
    assert!(output.contains("today_funding_pnl_usdt=5.67"));
    assert!(output.contains("staleness=Fresh"));
    assert!(output.contains("balances (1)"));
    assert!(output.contains("positions (1)"));
    assert!(output.contains("open orders (1)"));
    assert!(output.contains("USDT free=100.00000000"));
    assert!(output.contains("BTCUSDT market=FUTURES side=Buy"));
    assert!(output.contains("notional=16250.00"));
    assert!(output.contains("current_exposure=147.7273"));
    assert!(output.contains("target_exposure=0.2500"));
    assert!(output.contains("target_delta=-147.4773"));
    assert!(!output.contains("ETHUSDT"));
    assert!(output.contains("last_event=app.execution.completed"));
}

#[test]
fn portfolio_positions_output_hides_balance_and_order_sections() {
    let mut store = PortfolioStateStore::default();
    store.apply_snapshot(sandbox_quant::exchange::types::AuthoritativeSnapshot {
        balances: vec![BalanceSnapshot {
            asset: "USDT".to_string(),
            free: 100.0,
            locked: 10.0,
        }],
        positions: vec![PositionSnapshot {
            instrument: Instrument::new("BTCUSDT"),
            market: Market::Futures,
            signed_qty: 0.25,
            entry_price: Some(65000.0),
        }],
        open_orders: vec![OpenOrder {
            order_id: None,
            client_order_id: "close-1".to_string(),
            instrument: Instrument::new("BTCUSDT"),
            market: Market::Futures,
            side: sandbox_quant::domain::position::Side::Sell,
            orig_qty: 0.25,
            executed_qty: 0.0,
            reduce_only: true,
            status: OrderStatus::Submitted,
        }],
    });
    let mut event_log = EventLog::default();
    let prices = PriceStore::default();
    log(
        &mut event_log,
        "app.portfolio.refreshed",
        json!({
            "positions": 1,
            "today_realized_pnl_usdt": 12.34,
            "today_funding_pnl_usdt": 5.67,
            "margin_ratio": 0.1234
        }),
    );
    log(
        &mut event_log,
        "app.execution.completed",
        json!({
            "command_kind": "set_target_exposure",
            "instrument": "BTCUSDT",
            "target": 0.2500,
            "order_type": "market",
            "outcome_kind": "submitted",
        }),
    );

    let output = render_command_output(
        &AppCommand::Portfolio(PortfolioView::Positions),
        &store,
        &prices,
        &event_log,
        &StrategyStore::default(),
        BinanceMode::Demo,
    );

    assert!(output.contains("portfolio positions"));
    assert!(output.contains("positions (1)"));
    assert!(!output.contains("account"));
    assert!(!output.contains("balances ("));
    assert!(!output.contains("open orders ("));
}

#[test]
fn execution_output_includes_last_event_kind() {
    let store = PortfolioStateStore::default();
    let mut event_log = EventLog::default();
    let prices = PriceStore::default();
    log(
        &mut event_log,
        "app.execution.completed",
        json!({
            "command_kind": "close_all",
            "batch_id": 7,
            "submitted": 2,
            "skipped": 1,
            "rejected": 0,
            "remaining_positions": 1,
            "flat_confirmed": false,
            "remaining_gross_exposure_usdt": 128.55,
            "outcome_kind": "batch_completed",
        }),
    );

    let output = render_command_output(
        &AppCommand::Execution(ExecutionCommand::CloseAll {
            source: CommandSource::User,
        }),
        &store,
        &prices,
        &event_log,
        &StrategyStore::default(),
        BinanceMode::Demo,
    );

    assert!(output.contains("execution completed"));
    assert!(output.contains("command=close-all"));
    assert!(output.contains("submitted=2"));
    assert!(output.contains("skipped=1"));
    assert!(output.contains("rejected=0"));
    assert!(output.contains("remaining_positions=1"));
    assert!(output.contains("flat_confirmed=false"));
    assert!(output.contains("remaining_gross_exposure_usdt=128.55"));
}

#[test]
fn execution_output_renders_close_symbol_summary() {
    let store = PortfolioStateStore::default();
    let mut event_log = EventLog::default();
    let prices = PriceStore::default();
    log(
        &mut event_log,
        "app.execution.completed",
        json!({
            "command_kind": "close_symbol",
            "instrument": "BTCUSDT",
            "remaining_positions": 0,
            "flat_confirmed": true,
            "remaining_gross_exposure_usdt": 0.0,
            "outcome_kind": "Submitted",
        }),
    );

    let output = render_command_output(
        &AppCommand::Execution(ExecutionCommand::CloseSymbol {
            instrument: Instrument::new("BTCUSDT"),
            source: CommandSource::User,
        }),
        &store,
        &prices,
        &event_log,
        &StrategyStore::default(),
        BinanceMode::Demo,
    );

    assert!(output.contains("command=close-symbol"));
    assert!(output.contains("instrument=BTCUSDT"));
    assert!(output.contains("remaining_positions=0"));
    assert!(output.contains("flat_confirmed=true"));
    assert!(output.contains("remaining_gross_exposure_usdt=0.00"));
    assert!(output.contains("outcome=Submitted"));
}

#[test]
fn execution_output_renders_target_exposure_summary() {
    let store = PortfolioStateStore::default();
    let mut event_log = EventLog::default();
    let prices = PriceStore::default();
    log(
        &mut event_log,
        "app.execution.completed",
        json!({
            "command_kind": "set_target_exposure",
            "instrument": "BTCUSDT",
            "target": 0.25,
            "order_type": "market",
            "remaining_positions": 1,
            "flat_confirmed": false,
            "remaining_gross_exposure_usdt": 8123.45,
            "outcome_kind": "submitted",
        }),
    );

    let output = render_command_output(
        &AppCommand::Execution(ExecutionCommand::SetTargetExposure {
            instrument: Instrument::new("BTCUSDT"),
            target: sandbox_quant::domain::exposure::Exposure::new(0.25).expect("bounded"),
            order_type: OrderType::Market,
            source: CommandSource::User,
        }),
        &store,
        &prices,
        &event_log,
        &StrategyStore::default(),
        BinanceMode::Demo,
    );

    assert!(output.contains("command=set-target-exposure"));
    assert!(output.contains("instrument=BTCUSDT"));
    assert!(output.contains("target=0.25"));
    assert!(output.contains("order_type=market"));
    assert!(output.contains("remaining_positions=1"));
    assert!(output.contains("flat_confirmed=false"));
    assert!(output.contains("remaining_gross_exposure_usdt=8123.45"));
}

#[test]
fn execution_output_renders_already_at_target_summary() {
    let store = PortfolioStateStore::default();
    let mut event_log = EventLog::default();
    let prices = PriceStore::default();
    log(
        &mut event_log,
        "app.execution.completed",
        json!({
            "command_kind": "set_target_exposure",
            "instrument": "BTCUSDT",
            "target": 0.3,
            "order_type": "market",
            "remaining_positions": 1,
            "flat_confirmed": false,
            "remaining_gross_exposure_usdt": 5999.99,
            "outcome_kind": "already-at-target",
        }),
    );

    let output = render_command_output(
        &AppCommand::Execution(ExecutionCommand::SetTargetExposure {
            instrument: Instrument::new("BTCUSDT"),
            target: sandbox_quant::domain::exposure::Exposure::new(0.3).expect("bounded"),
            order_type: OrderType::Market,
            source: CommandSource::User,
        }),
        &store,
        &prices,
        &event_log,
        &StrategyStore::default(),
        BinanceMode::Demo,
    );

    assert!(output.contains("command=set-target-exposure"));
    assert!(output.contains("outcome=already-at-target"));
    assert!(output.contains("remaining_positions=1"));
    assert!(output.contains("flat_confirmed=false"));
    assert!(output.contains("remaining_gross_exposure_usdt=5999.99"));
}

#[test]
fn portfolio_output_renders_options_positions_and_orders() {
    let mut store = PortfolioStateStore::default();
    store.apply_snapshot(sandbox_quant::exchange::types::AuthoritativeSnapshot {
        balances: vec![BalanceSnapshot {
            asset: "USDT".to_string(),
            free: 20000.0,
            locked: 0.0,
        }],
        positions: vec![PositionSnapshot {
            instrument: Instrument::new("BTC-260327-200000-C"),
            market: Market::Options,
            signed_qty: 0.01,
            entry_price: Some(5.0),
        }],
        open_orders: vec![OpenOrder {
            order_id: Some(sandbox_quant::domain::identifiers::OrderId(191)),
            client_order_id: "api_1".to_string(),
            instrument: Instrument::new("BTC-260327-200000-C"),
            market: Market::Options,
            side: Side::Buy,
            orig_qty: 0.01,
            executed_qty: 0.0,
            reduce_only: false,
            status: OrderStatus::Submitted,
        }],
    });
    let event_log = EventLog::default();
    let prices = PriceStore::default();

    let output = render_command_output(
        &AppCommand::Portfolio(PortfolioView::Overview),
        &store,
        &prices,
        &event_log,
        &StrategyStore::default(),
        BinanceMode::Demo,
    );

    assert!(output.contains("BTC-260327-200000-C market=OPTIONS side=Buy"));
    assert!(output.contains("current_exposure=-"));
    assert!(output.contains("target_exposure=-"));
    assert!(output.contains("open orders (1)"));
    assert!(output.contains("BTC-260327-200000-C OPTIONS side=Buy"));
}

#[test]
fn execution_output_renders_option_order_summary() {
    let store = PortfolioStateStore::default();
    let mut event_log = EventLog::default();
    let prices = PriceStore::default();
    log(
        &mut event_log,
        "app.execution.completed",
        json!({
            "command_kind": "submit_option_order",
            "instrument": "BTC-260327-200000-C",
            "side": "Buy",
            "qty": 0.01,
            "order_type": "limit@5.00",
            "remaining_positions": 0,
            "flat_confirmed": true,
            "remaining_gross_exposure_usdt": 0.0,
            "outcome_kind": "submitted",
        }),
    );

    let output = render_command_output(
        &AppCommand::Execution(ExecutionCommand::SubmitOptionOrder {
            instrument: Instrument::new("BTC-260327-200000-C"),
            side: Side::Buy,
            qty: 0.01,
            order_type: OrderType::Limit { price: 5.0 },
            source: CommandSource::User,
        }),
        &store,
        &prices,
        &event_log,
        &StrategyStore::default(),
        BinanceMode::Demo,
    );

    assert!(output.contains("command=option-order"));
    assert!(output.contains("instrument=BTC-260327-200000-C"));
    assert!(output.contains("side=Buy"));
    assert!(output.contains("qty=0.01"));
    assert!(output.contains("order_type=limit@5.00"));
}

#[test]
fn strategy_templates_output_renders_steps() {
    let store = PortfolioStateStore::default();
    let prices = PriceStore::default();
    let event_log = EventLog::default();

    let output = render_command_output(
        &AppCommand::Strategy(StrategyCommand::Templates),
        &store,
        &prices,
        &event_log,
        &StrategyStore::default(),
        BinanceMode::Demo,
    );

    assert!(output.contains("strategy templates"));
    assert!(output.contains("template=liquidation-breakdown-short"));
    assert!(output.contains("1. Find a liquidation cluster above current price"));
    assert!(output.contains("7. End the strategy after exchange protection is live"));
}

#[test]
fn strategy_start_output_renders_watch_summary() {
    let store = PortfolioStateStore::default();
    let prices = PriceStore::default();
    let mut event_log = EventLog::default();
    let mut strategy_store = StrategyStore::default();
    let watch = strategy_store
        .create_watch(
            BinanceMode::Demo,
            StrategyTemplate::LiquidationBreakdownShort,
            Instrument::new("BTCUSDT"),
            StrategyStartConfig {
                risk_pct: 0.005,
                win_rate: 0.8,
                r_multiple: 1.5,
                max_entry_slippage_pct: 0.001,
            },
        )
        .expect("watch created");
    log(
        &mut event_log,
        "app.strategy.watch_started",
        json!({
            "watch_id": watch.id,
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

    let output = render_command_output(
        &AppCommand::Strategy(StrategyCommand::Start {
            template: StrategyTemplate::LiquidationBreakdownShort,
            instrument: Instrument::new("BTCUSDT"),
            config: StrategyStartConfig {
                risk_pct: 0.005,
                win_rate: 0.8,
                r_multiple: 1.5,
                max_entry_slippage_pct: 0.001,
            },
        }),
        &store,
        &prices,
        &event_log,
        &strategy_store,
        BinanceMode::Demo,
    );

    assert!(output.contains("strategy started"));
    assert!(output.contains("watch_id=1"));
    assert!(output.contains("template=liquidation-breakdown-short"));
    assert!(output.contains("instrument=BTCUSDT"));
    assert!(output.contains("state=armed"));
}

#[test]
fn strategy_list_output_renders_active_watch() {
    let store = PortfolioStateStore::default();
    let prices = PriceStore::default();
    let event_log = EventLog::default();
    let mut strategy_store = StrategyStore::default();
    strategy_store
        .create_watch(
            BinanceMode::Demo,
            StrategyTemplate::LiquidationBreakdownShort,
            Instrument::new("BTCUSDT"),
            StrategyStartConfig {
                risk_pct: 0.005,
                win_rate: 0.8,
                r_multiple: 1.5,
                max_entry_slippage_pct: 0.001,
            },
        )
        .expect("watch created");

    let output = render_command_output(
        &AppCommand::Strategy(StrategyCommand::List),
        &store,
        &prices,
        &event_log,
        &strategy_store,
        BinanceMode::Demo,
    );

    assert!(output.contains("strategy watches"));
    assert!(output.contains("active=1"));
    assert!(output.contains("template=liquidation-breakdown-short"));
    assert!(output.contains("state=armed"));
}
