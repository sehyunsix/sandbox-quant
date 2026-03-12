pub mod manager;

use crate::domain::instrument::Instrument;

#[derive(Debug, Clone, PartialEq)]
pub enum RecordCommand {
    Start { instruments: Vec<Instrument> },
    Status,
    Stop,
}
