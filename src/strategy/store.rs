use std::collections::BTreeMap;

use chrono::Utc;

use crate::app::bootstrap::BinanceMode;
use crate::domain::instrument::Instrument;
use crate::error::strategy_error::StrategyError;
use crate::strategy::command::StrategyStartConfig;
use crate::strategy::model::{StrategyTemplate, StrategyWatch, StrategyWatchState};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct StrategyStore {
    next_watch_id: u64,
    active: BTreeMap<u64, StrategyWatch>,
    history: Vec<StrategyWatch>,
}

impl StrategyStore {
    pub fn create_watch(
        &mut self,
        mode: BinanceMode,
        template: StrategyTemplate,
        instrument: Instrument,
        config: StrategyStartConfig,
    ) -> Result<StrategyWatch, StrategyError> {
        if self.active.values().any(|watch| {
            watch.mode == mode
                && watch.template == template
                && watch.instrument == instrument
                && watch.state == StrategyWatchState::Armed
        }) {
            return Err(StrategyError::DuplicateWatch {
                template: template.slug(),
                instrument: instrument.0.clone(),
            });
        }

        let id = self.next_id();
        let watch = StrategyWatch::new(id, mode, template, instrument, config);
        self.active.insert(id, watch.clone());
        Ok(watch)
    }

    pub fn active_watches(&self, mode: BinanceMode) -> Vec<&StrategyWatch> {
        self.active
            .values()
            .filter(|watch| watch.mode == mode)
            .collect()
    }

    pub fn history(&self, mode: BinanceMode) -> Vec<&StrategyWatch> {
        self.history
            .iter()
            .filter(|watch| watch.mode == mode)
            .collect()
    }

    pub fn get(&self, mode: BinanceMode, watch_id: u64) -> Option<&StrategyWatch> {
        self.active
            .get(&watch_id)
            .filter(|watch| watch.mode == mode)
            .or_else(|| self.history.iter().rev().find(|watch| watch.id == watch_id))
            .filter(|watch| watch.mode == mode)
    }

    pub fn stop_watch(
        &mut self,
        mode: BinanceMode,
        watch_id: u64,
    ) -> Result<StrategyWatch, StrategyError> {
        if self.active.get(&watch_id).map(|watch| watch.mode) != Some(mode) {
            return Err(StrategyError::WatchNotFound(watch_id));
        }
        let mut watch = self
            .active
            .remove(&watch_id)
            .ok_or(StrategyError::WatchNotFound(watch_id))?;
        watch.state = StrategyWatchState::Stopped;
        watch.updated_at = Utc::now();
        self.history.push(watch.clone());
        Ok(watch)
    }

    fn next_id(&mut self) -> u64 {
        self.next_watch_id += 1;
        self.next_watch_id
    }
}
