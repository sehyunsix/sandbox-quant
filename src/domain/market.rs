#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Market {
    Spot,
    Futures,
    Options,
}
