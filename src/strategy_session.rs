use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::strategy_catalog::StrategyCatalog;

#[derive(Debug, Clone)]
pub struct LoadedStrategySession {
    pub catalog: StrategyCatalog,
    pub selected_source_tag: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedStrategySession {
    selected_source_tag: String,
    profiles: Vec<crate::strategy_catalog::StrategyProfile>,
}

fn strategy_session_path() -> PathBuf {
    std::env::var("SQ_STRATEGY_SESSION_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data/strategy_session.json"))
}

pub fn load_strategy_session(
    config_fast: usize,
    config_slow: usize,
    min_ticks_between_signals: u64,
) -> Result<Option<LoadedStrategySession>> {
    let path = strategy_session_path();
    load_strategy_session_from_path(
        &path,
        config_fast,
        config_slow,
        min_ticks_between_signals,
    )
}

pub fn load_strategy_session_from_path(
    path: &Path,
    config_fast: usize,
    config_slow: usize,
    min_ticks_between_signals: u64,
) -> Result<Option<LoadedStrategySession>> {
    if !path.exists() {
        return Ok(None);
    }

    let payload = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let persisted: PersistedStrategySession =
        serde_json::from_str(&payload).context("failed to parse persisted strategy session json")?;

    Ok(Some(LoadedStrategySession {
        catalog: StrategyCatalog::from_profiles(
            persisted.profiles,
            config_fast,
            config_slow,
            min_ticks_between_signals,
        ),
        selected_source_tag: Some(persisted.selected_source_tag),
    }))
}

pub fn persist_strategy_session(
    catalog: &StrategyCatalog,
    selected_source_tag: &str,
) -> Result<()> {
    let path = strategy_session_path();
    persist_strategy_session_to_path(&path, catalog, selected_source_tag)
}

pub fn persist_strategy_session_to_path(
    path: &Path,
    catalog: &StrategyCatalog,
    selected_source_tag: &str,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let payload = PersistedStrategySession {
        selected_source_tag: selected_source_tag.to_string(),
        profiles: catalog.profiles().to_vec(),
    };
    let json = serde_json::to_string_pretty(&payload)
        .context("failed to serialize persisted strategy session json")?;
    std::fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
