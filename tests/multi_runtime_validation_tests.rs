use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use sandbox_quant::binance::rest::BinanceRestClient;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::risk_module::{EndpointRateLimits, RiskModule};
use sandbox_quant::runtime::strategy_registry::StrategyWorkerRegistry;
use tokio::sync::mpsc;

fn test_risk_module(global: u32) -> RiskModule {
    let rest = Arc::new(BinanceRestClient::new(
        "https://demo-api.binance.com",
        "https://demo-fapi.binance.com",
        "k",
        "s",
        "fk",
        "fs",
        5000,
    ));
    RiskModule::new(
        rest,
        global,
        EndpointRateLimits {
            orders_per_minute: 1_000,
            account_per_minute: 1_000,
            market_data_per_minute: 1_000,
        },
    )
}

#[test]
/// Validates concurrent intent pressure against a shared global budget:
/// even with multiple threads racing, accepted intents must never exceed the configured limit.
fn concurrent_intents_do_not_violate_global_rate_limit() {
    let limit = 25usize;
    let risk = Arc::new(Mutex::new(test_risk_module(limit as u32)));
    let approved = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let risk = Arc::clone(&risk);
        let approved = Arc::clone(&approved);
        handles.push(thread::spawn(move || {
            for _ in 0..20 {
                let mut guard = risk.lock().expect("mutex poisoned");
                if guard.reserve_rate_budget() {
                    approved.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }
    for h in handles {
        h.join().expect("thread join failed");
    }

    assert_eq!(approved.load(Ordering::Relaxed), limit);
}

#[test]
/// Validates multi-runtime dispatch coverage for 10 symbols x 3 strategies:
/// each registered worker receives at least one tick for its symbol without starvation.
fn ten_symbols_three_strategies_receive_ticks_without_starvation() {
    let mut registry = StrategyWorkerRegistry::default();
    let mut receivers = Vec::new();

    for sym_idx in 0..10 {
        let symbol = format!("SYM{}USDT", sym_idx);
        for strategy in ["cfg", "fst", "slw"] {
            let worker_id = format!("{}:{}:spot", strategy, symbol);
            let (tx, rx) = mpsc::channel::<Tick>(8);
            registry.register(worker_id, symbol.clone(), tx);
            receivers.push((symbol.clone(), rx));
        }
    }

    for sym_idx in 0..10 {
        let symbol = format!("SYM{}USDT", sym_idx);
        registry.dispatch_tick(Tick {
            symbol,
            price: 100.0 + sym_idx as f64,
            qty: 1.0,
            timestamp_ms: sym_idx as u64,
            is_buyer_maker: false,
            trade_id: sym_idx as u64,
        });
    }

    for (_symbol, mut rx) in receivers {
        assert!(rx.try_recv().is_ok(), "worker did not receive any tick");
    }
}
