use crate::domain::exposure::Exposure;
use crate::domain::instrument::Instrument;
use crate::domain::order_type::OrderType;
use crate::domain::position::Side;

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
        order_type: OrderType,
        source: CommandSource,
    },
    SubmitOptionOrder {
        instrument: Instrument,
        side: Side,
        qty: f64,
        order_type: OrderType,
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
