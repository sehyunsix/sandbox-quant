use thiserror::Error;

use crate::v1::error::exchange_error::ExchangeError;
use crate::v1::error::execution_error::ExecutionError;
use crate::v1::error::storage_error::StorageError;
use crate::v1::error::sync_error::SyncError;
use crate::v1::error::ui_error::UiError;

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
    #[error("ui error: {0}")]
    Ui(#[from] UiError),
}
