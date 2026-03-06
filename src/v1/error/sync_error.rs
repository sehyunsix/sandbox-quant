use thiserror::Error;

use crate::v1::error::exchange_error::ExchangeError;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum SyncError {
    #[error("stream stale")]
    StreamStale,
    #[error("snapshot fetch failed")]
    SnapshotFetchFailed(#[from] ExchangeError),
}
