use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use chrono::Utc;

use crate::storage::models::EventRecord;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct EventLog {
    pub records: Vec<EventRecord>,
}

impl EventLog {
    pub fn append(&mut self, record: EventRecord) {
        append_record_to_jsonl(&record);
        self.records.push(record);
    }
}

pub fn log(event_log: &mut EventLog, kind: impl Into<String>, payload: serde_json::Value) {
    event_log.append(EventRecord {
        kind: kind.into(),
        payload,
    });
}

fn append_record_to_jsonl(record: &EventRecord) {
    let path = std::env::var("SANDBOX_QUANT_EVENT_LOG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/operator-events.jsonl"));
    if let Some(parent) = path.parent() {
        if create_dir_all(parent).is_err() {
            return;
        }
    }
    let mut file = match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => file,
        Err(_) => return,
    };
    let line = serde_json::json!({
        "ts": Utc::now().to_rfc3339(),
        "kind": record.kind,
        "payload": record.payload,
    });
    let _ = writeln!(file, "{}", line);
}
