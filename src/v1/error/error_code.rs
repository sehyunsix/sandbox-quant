#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ExchangeRateLimited,
    ExchangeInvalidResponse,
    ExecutionCloseQtyTooSmall,
    ExecutionRejected,
    SyncStreamStale,
    StorageWriteFailed,
    UiInvalidCommand,
}
