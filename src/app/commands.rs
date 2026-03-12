use crate::execution::command::ExecutionCommand;
use crate::strategy::command::StrategyCommand;

#[derive(Debug, Clone, PartialEq)]
pub enum PortfolioView {
    Overview,
    Positions,
    Balances,
    Orders,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppCommand {
    Execution(ExecutionCommand),
    Strategy(StrategyCommand),
    Portfolio(PortfolioView),
    RefreshAuthoritativeState,
}
