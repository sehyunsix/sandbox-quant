use crate::domain::identifiers::BatchId;
use crate::execution::close_symbol::CloseSymbolResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseAllBatchResult {
    pub batch_id: BatchId,
    pub results: Vec<CloseSymbolResult>,
}
