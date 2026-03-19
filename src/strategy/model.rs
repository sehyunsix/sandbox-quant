use chrono::{DateTime, Utc};

use crate::app::bootstrap::BinanceMode;
use crate::domain::instrument::Instrument;
use crate::strategy::command::StrategyStartConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyTemplate {
    LiquidationBreakdownShort,
    PriceSmaCrossLong,
    PriceSmaCrossShort,
    PriceSmaCrossLongFast,
    PriceSmaCrossShortFast,
}

impl StrategyTemplate {
    pub fn slug(self) -> &'static str {
        match self {
            Self::LiquidationBreakdownShort => "liquidation-breakdown-short",
            Self::PriceSmaCrossLong => "price-sma-cross-long",
            Self::PriceSmaCrossShort => "price-sma-cross-short",
            Self::PriceSmaCrossLongFast => "price-sma-cross-long-fast",
            Self::PriceSmaCrossShortFast => "price-sma-cross-short-fast",
        }
    }

    pub fn steps(self) -> &'static [&'static str] {
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
            Self::PriceSmaCrossLong => &[
                "Read raw futures kline closes from the selected interval",
                "Compute fast and slow simple moving averages on price",
                "Enter long when the fast average crosses above the slow average",
                "Manage the position with stop loss, take profit, or bearish cross exit",
                "Close any remaining position at the end of the backtest window",
            ],
            Self::PriceSmaCrossShort => &[
                "Read raw futures kline closes from the selected interval",
                "Compute fast and slow simple moving averages on price",
                "Enter short when the fast average crosses below the slow average",
                "Manage the position with stop loss, take profit, or bullish cross exit",
                "Close any remaining position at the end of the backtest window",
            ],
            Self::PriceSmaCrossLongFast => &[
                "Read raw futures kline closes from the selected interval",
                "Compute faster 9/21 simple moving averages on price",
                "Enter long when the fast average crosses above the slow average",
                "Manage the position with stop loss, take profit, or bearish cross exit",
                "Close any remaining position at the end of the backtest window",
            ],
            Self::PriceSmaCrossShortFast => &[
                "Read raw futures kline closes from the selected interval",
                "Compute faster 9/21 simple moving averages on price",
                "Enter short when the fast average crosses below the slow average",
                "Manage the position with stop loss, take profit, or bullish cross exit",
                "Close any remaining position at the end of the backtest window",
            ],
        }
    }

    pub fn all() -> [Self; 5] {
        [
            Self::LiquidationBreakdownShort,
            Self::PriceSmaCrossLong,
            Self::PriceSmaCrossShort,
            Self::PriceSmaCrossLongFast,
            Self::PriceSmaCrossShortFast,
        ]
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
