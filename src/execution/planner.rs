use crate::domain::instrument::Instrument;
use crate::domain::position::Side;

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionPlan {
    pub instrument: Instrument,
    pub side: Side,
    pub qty: f64,
    pub reduce_only: bool,
}
