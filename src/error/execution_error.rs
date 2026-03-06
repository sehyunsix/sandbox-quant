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
        "order quantity too small for {instrument}: market={market} target_exposure={target_exposure:.4} equity_usdt={equity_usdt:.8} current_price={current_price:.8} target_notional_usdt={target_notional_usdt:.8} raw={raw_qty:.8} normalized={normalized_qty:.8} min_qty={min_qty:.8} step_size={step_size:.8}"
    )]
    OrderQtyTooSmall {
        instrument: String,
        market: String,
        target_exposure: f64,
        equity_usdt: f64,
        current_price: f64,
        target_notional_usdt: f64,
        raw_qty: f64,
        normalized_qty: f64,
        min_qty: f64,
        step_size: f64,
    },
    #[error("exchange submit failed: {0}")]
    SubmitFailed(#[from] ExchangeError),
}
