use crate::storage::models::EventRecord;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct EventLog {
    pub records: Vec<EventRecord>,
}

impl EventLog {
    pub fn append(&mut self, record: EventRecord) {
        self.records.push(record);
    }
}

pub fn log(event_log: &mut EventLog, kind: impl Into<String>, payload: serde_json::Value) {
    event_log.append(EventRecord {
        kind: kind.into(),
        payload,
    });
}
