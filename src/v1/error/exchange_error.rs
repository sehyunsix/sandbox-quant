use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ExchangeError {
    #[error("network timeout")]
    NetworkTimeout,
    #[error("rate limited")]
    RateLimited,
    #[error("invalid response")]
    InvalidResponse,
}
