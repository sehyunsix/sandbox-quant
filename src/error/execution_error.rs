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
    #[error(
        "order quantity too small for {instrument}: raw={raw_qty:.8} normalized={normalized_qty:.8} min_qty={min_qty:.8} step_size={step_size:.8}"
    )]
    OrderQtyTooSmall {
        instrument: String,
        raw_qty: f64,
        normalized_qty: f64,
        min_qty: f64,
        step_size: f64,
    },
    #[error("exchange submit failed: {0}")]
    SubmitFailed(#[from] ExchangeError),
}
