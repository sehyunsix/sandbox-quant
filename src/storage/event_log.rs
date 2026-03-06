use crate::storage::models::EventRecord;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EventLog {
    pub records: Vec<EventRecord>,
}

impl EventLog {
    pub fn append(&mut self, record: EventRecord) {
        self.records.push(record);
    }
}
