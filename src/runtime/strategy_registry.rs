use std::collections::{HashMap, HashSet};

use tokio::sync::mpsc;

use crate::model::tick::Tick;

#[derive(Default)]
pub struct StrategyWorkerRegistry {
    workers: HashMap<String, StrategyWorkerHandle>,
    workers_by_symbol: HashMap<String, HashSet<String>>,
}

struct StrategyWorkerHandle {
    symbol: String,
    tick_tx: mpsc::Sender<Tick>,
}

impl StrategyWorkerRegistry {
    pub fn register(
        &mut self,
        worker_id: impl Into<String>,
        symbol: impl Into<String>,
        tick_tx: mpsc::Sender<Tick>,
    ) {
        let worker_id = worker_id.into();
        let symbol = symbol.into().to_ascii_uppercase();

        if let Some(existing) = self.workers.remove(&worker_id) {
            if let Some(ids) = self.workers_by_symbol.get_mut(&existing.symbol) {
                ids.remove(&worker_id);
                if ids.is_empty() {
                    self.workers_by_symbol.remove(&existing.symbol);
                }
            }
        }

        self.workers.insert(
            worker_id.clone(),
            StrategyWorkerHandle {
                symbol: symbol.clone(),
                tick_tx,
            },
        );
        self.workers_by_symbol
            .entry(symbol)
            .or_default()
            .insert(worker_id);
    }

    pub fn unregister(&mut self, worker_id: &str) {
        let Some(existing) = self.workers.remove(worker_id) else {
            return;
        };
        if let Some(ids) = self.workers_by_symbol.get_mut(&existing.symbol) {
            ids.remove(worker_id);
            if ids.is_empty() {
                self.workers_by_symbol.remove(&existing.symbol);
            }
        }
    }

    pub fn dispatch_tick(&self, tick: Tick) {
        let symbol = tick.symbol.to_ascii_uppercase();
        let Some(worker_ids) = self.workers_by_symbol.get(&symbol) else {
            return;
        };

        for worker_id in worker_ids {
            if let Some(worker) = self.workers.get(worker_id) {
                let _ = worker.tick_tx.try_send(tick.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StrategyWorkerRegistry;
    use crate::model::tick::Tick;
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
    fn unregister_removes_worker_from_dispatch_path() {
        let mut registry = StrategyWorkerRegistry::default();
        let (btc_tx, mut btc_rx) = mpsc::channel(4);
        registry.register("ma-cfg-btc", "BTCUSDT", btc_tx);
        registry.unregister("ma-cfg-btc");

        registry.dispatch_tick(tick("BTCUSDT", 100.0));
        assert!(btc_rx.try_recv().is_err());
    }
}
