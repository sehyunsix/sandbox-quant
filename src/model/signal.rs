use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Buy { trace_id: Uuid },
    Sell { trace_id: Uuid },
    Hold,
}
