#[derive(Debug, Clone, PartialEq)]
pub struct EventRecord {
    pub kind: String,
    pub payload: serde_json::Value,
}
