use crate::domain::instrument::Instrument;
use crate::strategy::model::StrategyTemplate;

#[derive(Debug, Clone, PartialEq)]
pub struct StrategyStartConfig {
    pub risk_pct: f64,
    pub win_rate: f64,
    pub r_multiple: f64,
    pub max_entry_slippage_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrategyCommand {
    Templates,
    Start {
        template: StrategyTemplate,
        instrument: Instrument,
        config: StrategyStartConfig,
    },
    List,
    Show {
        watch_id: u64,
    },
    Stop {
        watch_id: u64,
    },
    History,
}
