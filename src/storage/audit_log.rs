use crate::storage::models::EventRecord;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AuditLog {
    pub records: Vec<EventRecord>,
}
