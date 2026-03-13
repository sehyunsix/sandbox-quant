use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::app::bootstrap::BinanceMode;
use crate::error::storage_error::StorageError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecorderCoordination {
    base_dir: PathBuf,
}

impl Default for RecorderCoordination {
    fn default() -> Self {
        Self::new("var")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StrategySymbolFile {
    mode: String,
    strategy_symbols: Vec<String>,
    updated_at: String,
}

impl RecorderCoordination {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    pub fn sync_strategy_symbols(
        &self,
        mode: BinanceMode,
        strategy_symbols: Vec<String>,
    ) -> Result<(), StorageError> {
        fs::create_dir_all(&self.base_dir).map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
        let payload = StrategySymbolFile {
            mode: mode.as_str().to_string(),
            strategy_symbols: normalize_symbols(strategy_symbols),
            updated_at: Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_vec_pretty(&payload).map_err(|error| {
            StorageError::WriteFailedWithContext {
                message: error.to_string(),
            }
        })?;
        atomic_write(self.strategy_symbols_path(mode), &json)
    }

    pub fn strategy_symbols(&self, mode: BinanceMode) -> Result<Vec<String>, StorageError> {
        let path = self.strategy_symbols_path(mode);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = fs::read(&path).map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
        let payload: StrategySymbolFile = serde_json::from_slice(&bytes).map_err(|error| {
            StorageError::WriteFailedWithContext {
                message: error.to_string(),
            }
        })?;
        Ok(normalize_symbols(payload.strategy_symbols))
    }

    pub fn db_path(&self, mode: BinanceMode) -> PathBuf {
        self.base_dir.join(format!("market-v2-{}.duckdb", mode.as_str()))
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    fn strategy_symbols_path(&self, mode: BinanceMode) -> PathBuf {
        self.base_dir
            .join(format!("record-{}.strategy-symbols.json", mode.as_str()))
    }
}

fn normalize_symbols(symbols: Vec<String>) -> Vec<String> {
    let mut normalized = symbols
        .into_iter()
        .map(|symbol| symbol.trim().to_ascii_uppercase())
        .filter(|symbol| !symbol.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn atomic_write(path: PathBuf, bytes: &[u8]) -> Result<(), StorageError> {
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, bytes).map_err(|error| StorageError::WriteFailedWithContext {
        message: error.to_string(),
    })?;
    fs::rename(&tmp_path, &path).map_err(|error| StorageError::WriteFailedWithContext {
        message: error.to_string(),
    })
}
