use crate::v1::domain::identifiers::BatchId;
use crate::v1::execution::close_symbol::CloseSymbolResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseAllBatchResult {
    pub batch_id: BatchId,
    pub results: Vec<CloseSymbolResult>,
}
