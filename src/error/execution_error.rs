use thiserror::Error;

use crate::error::exchange_error::ExchangeError;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum ExecutionError {
    #[error("close quantity too small")]
    CloseQtyTooSmall,
    #[error("no open position for close command")]
    NoOpenPosition,
    #[error("symbol could not be resolved on exchange: {0}")]
    UnknownInstrument(String),
    #[error("missing price context")]
    MissingPriceContext,
    #[error("exchange submit failed: {0}")]
    SubmitFailed(#[from] ExchangeError),
}
