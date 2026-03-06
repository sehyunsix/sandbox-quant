#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub kind: String,
    pub payload: String,
}
