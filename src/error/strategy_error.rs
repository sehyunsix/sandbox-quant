use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum StrategyError {
    #[error("strategy watch not found: id={0}")]
    WatchNotFound(u64),
    #[error("strategy watch already armed: template={template} instrument={instrument}")]
    DuplicateWatch {
        template: &'static str,
        instrument: String,
    },
}
