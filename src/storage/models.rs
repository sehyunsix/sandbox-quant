#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EventRecord {
    pub kind: String,
    pub payload: serde_json::Value,
}
