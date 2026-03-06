use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ExchangeError {
    #[error("network timeout")]
    NetworkTimeout,
    #[error("rate limited")]
    RateLimited,
    #[error("authentication failed")]
    AuthenticationFailed,
    #[error("missing configuration: {0}")]
    MissingConfiguration(&'static str),
    #[error("invalid timestamp")]
    InvalidTimestamp,
    #[error("invalid response")]
    InvalidResponse,
    #[error("remote rejected request: code={code} message={message}")]
    RemoteReject { code: i64, message: String },
    #[error("transport failure")]
    TransportFailure,
    #[error("unsupported market operation")]
    UnsupportedMarketOperation,
}
