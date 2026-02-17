use std::collections::{BTreeSet, HashMap};

use tokio::sync::mpsc;

use crate::model::tick::Tick;

#[derive(Default)]
pub struct StrategyWorkerRegistry {
    workers: HashMap<String, StrategyWorkerHandle>,
    workers_by_symbol: HashMap<String, BTreeSet<String>>,
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

    /// Return worker ids for the symbol in deterministic lexical order.
    pub fn worker_ids_for_symbol(&self, symbol: &str) -> Vec<String> {
        let key = symbol.trim().to_ascii_uppercase();
        self.workers_by_symbol
            .get(&key)
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default()
    }
}
