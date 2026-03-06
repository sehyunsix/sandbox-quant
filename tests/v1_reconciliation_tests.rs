use sandbox_quant::domain::instrument::Instrument;
use sandbox_quant::domain::market::Market;
use sandbox_quant::domain::order::{OpenOrder, OrderStatus};
use sandbox_quant::domain::position::{PositionSnapshot, Side};
use sandbox_quant::exchange::fake::FakeExchange;
use sandbox_quant::exchange::types::AuthoritativeSnapshot;
use sandbox_quant::portfolio::staleness::StalenessState;
use sandbox_quant::portfolio::store::PortfolioStateStore;
use sandbox_quant::portfolio::sync::PortfolioSyncService;

#[test]
fn refresh_from_exchange_overwrites_local_state_and_clears_staleness() {
    let btc = Instrument::new("BTCUSDT");
    let eth = Instrument::new("ETHUSDT");
    let fake = FakeExchange::new(AuthoritativeSnapshot {
        balances: vec![],
        positions: vec![PositionSnapshot {
            instrument: btc.clone(),
            market: Market::Futures,
            signed_qty: 0.5,
            entry_price: Some(65000.0),
        }],
        open_orders: vec![],
    });

    let mut store = PortfolioStateStore::default();
    store.apply_snapshot(AuthoritativeSnapshot {
        balances: vec![],
        positions: vec![PositionSnapshot {
            instrument: eth.clone(),
            market: Market::Spot,
            signed_qty: 2.0,
            entry_price: Some(2500.0),
        }],
        open_orders: vec![OpenOrder {
            order_id: None,
            client_order_id: "old-open-order".to_string(),
            instrument: eth.clone(),
            market: Market::Spot,
            side: Side::Sell,
            orig_qty: 1.0,
            executed_qty: 0.0,
            reduce_only: false,
            status: OrderStatus::Submitted,
        }],
    });
    store.mark_reconciliation_stale();

    store
        .refresh_from_exchange(&fake)
        .expect("exchange refresh should succeed");

    assert_eq!(store.staleness, StalenessState::Fresh);
    assert!(store.snapshot.positions.contains_key(&btc));
    assert!(!store.snapshot.positions.contains_key(&eth));
    assert!(store.snapshot.open_orders.is_empty());
}

#[test]
fn stale_markers_transition_without_ui_dependencies() {
    let sync = PortfolioSyncService;
    let mut store = PortfolioStateStore::default();

    sync.mark_market_data_stale(&mut store);
    assert_eq!(store.staleness, StalenessState::MarketDataStale);

    sync.mark_account_state_stale(&mut store);
    assert_eq!(store.staleness, StalenessState::AccountStateStale);

    sync.mark_reconciliation_stale(&mut store);
    assert_eq!(store.staleness, StalenessState::ReconciliationStale);
}

#[test]
fn sync_service_refreshes_authoritative_state_and_reports_counts() {
    let sync = PortfolioSyncService;
    let btc = Instrument::new("BTCUSDT");
    let fake = FakeExchange::new(AuthoritativeSnapshot {
        balances: vec![],
        positions: vec![PositionSnapshot {
            instrument: btc.clone(),
            market: Market::Futures,
            signed_qty: -0.25,
            entry_price: Some(64000.0),
        }],
        open_orders: vec![OpenOrder {
            order_id: None,
            client_order_id: "reduce-only-close".to_string(),
            instrument: btc,
            market: Market::Futures,
            side: Side::Buy,
            orig_qty: 0.25,
            executed_qty: 0.0,
            reduce_only: true,
            status: OrderStatus::Submitted,
        }],
    });
    let mut store = PortfolioStateStore::default();
    store.mark_account_state_stale();

    let report = sync
        .refresh_authoritative(&fake, &mut store)
        .expect("refresh should succeed");

    assert_eq!(store.staleness, StalenessState::Fresh);
    assert_eq!(report.positions, 1);
    assert_eq!(report.open_order_groups, 1);
    assert_eq!(report.balances, 0);
}
