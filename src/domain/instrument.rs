#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Instrument(pub String);

impl Instrument {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
