use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;

#[derive(Debug, Clone, Copy, Default)]
pub struct StrategyStats {
    pub wins: u32,
    pub losses: u32,
    pub realized_pnl: f64,
}

impl StrategyStats {
    pub fn total(&self) -> u32 {
        self.wins + self.losses
    }

    pub fn win_rate_percent(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            (self.wins as f64 / total as f64) * 100.0
        }
    }
}

#[derive(Debug)]
pub struct StrategyStatsStore {
    #[allow(dead_code)]
    path: PathBuf,
    data: Mutex<HashMap<String, StrategyStats>>,
}

impl StrategyStatsStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(Self {
            path: path.to_path_buf(),
            data: Mutex::new(HashMap::new()),
        })
    }

    pub fn snapshot(&self) -> Result<HashMap<String, StrategyStats>> {
        let guard = self
            .data
            .lock()
            .map_err(|_| anyhow::anyhow!("strategy stats lock poisoned"))?;
        Ok(guard.clone())
    }

    pub fn increment(
        &self,
        strategy_label: &str,
        wins: u32,
        losses: u32,
        realized_pnl_delta: f64,
    ) -> Result<()> {
        let mut guard = self
            .data
            .lock()
            .map_err(|_| anyhow::anyhow!("strategy stats lock poisoned"))?;
        let entry = guard.entry(strategy_label.to_string()).or_default();
        entry.wins = entry.wins.saturating_add(wins);
        entry.losses = entry.losses.saturating_add(losses);
        entry.realized_pnl += realized_pnl_delta;
        Ok(())
    }
}
