use crate::v1::domain::instrument::Instrument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseSubmitResult {
    Submitted,
    Rejected,
    SkippedNoPosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseSymbolResult {
    pub instrument: Instrument,
    pub result: CloseSubmitResult,
}
