use crate::v1::domain::exposure::Exposure;
use crate::v1::domain::instrument::Instrument;

#[derive(Debug, Clone, PartialEq)]
pub enum CommandSource {
    User,
    System,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionCommand {
    SetTargetExposure {
        instrument: Instrument,
        target: Exposure,
        source: CommandSource,
    },
    CloseSymbol {
        instrument: Instrument,
        source: CommandSource,
    },
    CloseAll {
        source: CommandSource,
    },
}
