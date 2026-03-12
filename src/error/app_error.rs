use thiserror::Error;

use crate::error::exchange_error::ExchangeError;
use crate::error::execution_error::ExecutionError;
use crate::error::storage_error::StorageError;
use crate::error::strategy_error::StrategyError;
use crate::error::sync_error::SyncError;
use crate::error::ui_error::UiError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("exchange error: {0}")]
    Exchange(#[from] ExchangeError),
    #[error("execution error: {0}")]
    Execution(#[from] ExecutionError),
    #[error("sync error: {0}")]
    Sync(#[from] SyncError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("strategy error: {0}")]
    Strategy(#[from] StrategyError),
    #[error("ui error: {0}")]
    Ui(#[from] UiError),
}
