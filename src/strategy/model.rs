use chrono::{DateTime, Utc};

use crate::app::bootstrap::BinanceMode;
use crate::domain::instrument::Instrument;
use crate::strategy::command::StrategyStartConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyTemplate {
    LiquidationBreakdownShort,
}

impl StrategyTemplate {
    pub fn slug(self) -> &'static str {
        match self {
            Self::LiquidationBreakdownShort => "liquidation-breakdown-short",
        }
    }

    pub fn steps(self) -> &'static [&'static str; 7] {
        match self {
            Self::LiquidationBreakdownShort => &[
                "Find a liquidation cluster above current price",
                "Wait for price to trade into that cluster",
                "Detect failure to hold above the sweep area",
                "Confirm downside continuation",
                "Enter short from best bid/ask with slippage cap",
                "Place reduce-only stop loss and take profit from actual fill",
                "End the strategy after exchange protection is live",
            ],
        }
    }

    pub fn all() -> [Self; 1] {
        [Self::LiquidationBreakdownShort]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyWatchState {
    Armed,
    Triggered,
    Completed,
    Failed,
    Stopped,
}

impl StrategyWatchState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Armed => "armed",
            Self::Triggered => "triggered",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrategyWatch {
    pub id: u64,
    pub mode: BinanceMode,
    pub template: StrategyTemplate,
    pub instrument: Instrument,
    pub state: StrategyWatchState,
    pub current_step: usize,
    pub config: StrategyStartConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StrategyWatch {
    pub fn new(
        id: u64,
        mode: BinanceMode,
        template: StrategyTemplate,
        instrument: Instrument,
        config: StrategyStartConfig,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            mode,
            template,
            instrument,
            state: StrategyWatchState::Armed,
            current_step: 1,
            config,
            created_at: now,
            updated_at: now,
        }
    }
}
