use thiserror::Error;

use crate::v1::error::exchange_error::ExchangeError;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum ExecutionError {
    #[error("close quantity too small")]
    CloseQtyTooSmall,
    #[error("no open position")]
    NoOpenPosition,
    #[error("exchange submit failed")]
    SubmitFailed(#[from] ExchangeError),
}
