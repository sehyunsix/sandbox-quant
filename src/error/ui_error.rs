use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum UiError {
    #[error("invalid command")]
    InvalidCommand,
}
