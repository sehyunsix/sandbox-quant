use sandbox_quant::domain::exposure::Exposure;
use sandbox_quant::domain::balance::BalanceSnapshot;
use sandbox_quant::domain::instrument::Instrument;
use sandbox_quant::domain::market::Market;
use sandbox_quant::domain::position::{PositionSnapshot, Side};
use sandbox_quant::exchange::fake::FakeExchange;
use sandbox_quant::exchange::symbol_rules::SymbolRules;
use sandbox_quant::execution::close_all::CloseAllBatchResult;
use sandbox_quant::execution::close_symbol::{CloseSubmitResult, CloseSymbolResult};
use sandbox_quant::execution::service::ExecutionService;
use sandbox_quant::portfolio::store::PortfolioStateStore;
use sandbox_quant::exchange::types::AuthoritativeSnapshot;
use sandbox_quant::domain::identifiers::BatchId;

#[test]
fn position_snapshot_derives_side_and_abs_qty_from_signed_qty() {
    let short = PositionSnapshot {
        instrument: Instrument::new("BTCUSDT"),
        market: Market::Futures,
        signed_qty: -0.25,
        entry_price: Some(65000.0),
    };

    assert_eq!(short.side(), Some(Side::Sell));
    assert!((short.abs_qty() - 0.25).abs() < 1e-9);
    assert!(!short.is_flat());
}

#[test]
fn exposure_is_bounded_to_signed_unit_interval() {
    assert!(Exposure::new(1.0).is_some());
    assert!(Exposure::new(-1.0).is_some());
    assert!(Exposure::new(1.1).is_none());
}

#[test]
fn authoritative_snapshot_populates_positions_and_open_orders_in_store() {
    let mut store = PortfolioStateStore::default();
    let snapshot = AuthoritativeSnapshot {
        balances: vec![],
        positions: vec![PositionSnapshot {
            instrument: Instrument::new("ETHUSDT"),
            market: Market::Spot,
            signed_qty: 2.0,
            entry_price: Some(2500.0),
        }],
        open_orders: vec![],
    };

    store.apply_snapshot(snapshot);

    assert!(store.snapshot.positions.contains_key(&Instrument::new("ETHUSDT")));
    assert!(store.snapshot.open_orders.is_empty());
}

#[test]
fn close_all_batch_result_collects_per_symbol_submit_outcomes() {
    let batch = CloseAllBatchResult {
        batch_id: BatchId(7),
        results: vec![
            CloseSymbolResult {
                instrument: Instrument::new("BTCUSDT"),
                result: CloseSubmitResult::Submitted,
            },
            CloseSymbolResult {
                instrument: Instrument::new("ETHUSDT"),
                result: CloseSubmitResult::SkippedNoPosition,
            },
        ],
    };

    assert_eq!(batch.results.len(), 2);
    assert_eq!(batch.results[0].result, CloseSubmitResult::Submitted);
    assert_eq!(batch.results[1].result, CloseSubmitResult::SkippedNoPosition);
}

#[test]
fn execution_service_submits_close_against_authoritative_store_without_ui() {
    let instrument = Instrument::new("BTCUSDT");
    let fake = FakeExchange::new(AuthoritativeSnapshot::default());
    fake.set_symbol_rules(
        instrument.clone(),
        Market::Futures,
        SymbolRules {
            min_qty: 0.001,
            max_qty: 100.0,
            step_size: 0.001,
        },
    );

    let mut store = PortfolioStateStore::default();
    store.apply_snapshot(AuthoritativeSnapshot {
        balances: vec![],
        positions: vec![PositionSnapshot {
            instrument: instrument.clone(),
            market: Market::Futures,
            signed_qty: -0.3,
            entry_price: Some(64000.0),
        }],
        open_orders: vec![],
    });

    let mut service = ExecutionService::default();
    let result = service
        .close_symbol(&fake, &store, &instrument)
        .expect("close submit should succeed");

    assert_eq!(result.result, CloseSubmitResult::Submitted);
    let requests = fake.close_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].instrument, instrument);
    assert_eq!(requests[0].side, Side::Buy);
    assert!((requests[0].qty - 0.3).abs() < 1e-9);
    assert!(requests[0].reduce_only);
}

#[test]
fn execution_service_plans_target_exposure_from_authoritative_store() {
    let instrument = Instrument::new("BTCUSDT");
    let mut store = PortfolioStateStore::default();
    store.apply_snapshot(AuthoritativeSnapshot {
        balances: vec![BalanceSnapshot {
            asset: "USDT".to_string(),
            free: 1000.0,
            locked: 0.0,
        }],
        positions: vec![PositionSnapshot {
            instrument: instrument.clone(),
            market: Market::Futures,
            signed_qty: -0.25,
            entry_price: Some(50000.0),
        }],
        open_orders: vec![],
    });

    let service = ExecutionService::default();
    let plan = service
        .plan_target_exposure(&store, &instrument, Exposure::new(0.5).expect("bounded exposure"))
        .expect("planning should succeed");

    assert_eq!(plan.instrument, instrument);
    assert_eq!(plan.side, Side::Buy);
    assert!((plan.qty - 0.26).abs() < 1e-9);
    assert!(!plan.reduce_only);
}
