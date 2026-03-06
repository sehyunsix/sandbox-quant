use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StorageError {
    #[error("write failed")]
    WriteFailed,
}
