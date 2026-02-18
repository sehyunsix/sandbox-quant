usqe sandbox_quant::model::tick::Tick;
use sandbox_quant::runtime::strategy_registry::StrategyWorkerRegistry;
use tokio::sync::mpsc;

fn tick(symbol: &str, price: f64) -> Tick {
    Tick {
        symbol: symbol.to_string(),
        price,
        qty: 0.1,
        timestamp_ms: 1,
        is_buyer_maker: false,
        trade_id: 1,
    }
}

#[test]
/// Verifies symbol-based routing: only workers registered for the same symbol
/// should receive the dispatched tick.
fn dispatches_only_to_matching_symbol_workers() {
    let mut registry = StrategyWorkerRegistry::default();
    let (btc_tx, mut btc_rx) = mpsc::channel(4);
    let (eth_tx, mut eth_rx) = mpsc::channel(4);
    registry.register("ma-cfg-btc", "BTCUSDT", btc_tx);
    registry.register("ma-cfg-eth", "ETHUSDT", eth_tx);

    registry.dispatch_tick(tick("BTCUSDT", 100.0));
    assert!(btc_rx.try_recv().is_ok());
    assert!(eth_rx.try_recv().is_err());
}

#[test]
/// Verifies registry cleanup: once a worker is unregistered it must no
/// longer receive ticks for its former symbol.
fn unregister_removes_worker_from_dispatch_path() {
    let mut registry = StrategyWorkerRegistry::default();
    let (btc_tx, mut btc_rx) = mpsc::channel(4);
    registry.register("ma-cfg-btc", "BTCUSDT", btc_tx);
    registry.unregister("ma-cfg-btc");

    registry.dispatch_tick(tick("BTCUSDT", 100.0));
    assert!(btc_rx.try_recv().is_err());
}

#[test]
/// Verifies deterministic scheduling order under fixed input:
/// worker ids for a symbol are returned in lexical order.
fn worker_ids_are_deterministic_for_symbol() {
    let mut registry = StrategyWorkerRegistry::default();
    let (tx_a, _rx_a) = mpsc::channel(1);
    let (tx_b, _rx_b) = mpsc::channel(1);
    let (tx_c, _rx_c) = mpsc::channel(1);
    registry.register("slw:BTCUSDT:spot", "BTCUSDT", tx_a);
    registry.register("cfg:BTCUSDT:spot", "BTCUSDT", tx_b);
    registry.register("fst:BTCUSDT:spot", "BTCUSDT", tx_c);

    let ids = registry.worker_ids_for_symbol("BTCUSDT");
    assert_eq!(
        ids,
        vec![
            "cfg:BTCUSDT:spot".to_string(),
            "fst:BTCUSDT:spot".to_string(),
            "slw:BTCUSDT:spot".to_string()
        ]
    );
}
