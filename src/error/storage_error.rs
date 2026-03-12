use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StorageError {
    #[error("write failed")]
    WriteFailed,
    #[error("write failed: {message}")]
    WriteFailedWithContext { message: String },
    #[error("recorder already running: mode={mode}")]
    RecorderAlreadyRunning { mode: String },
    #[error("recorder not running: mode={mode}")]
    RecorderNotRunning { mode: String },
    #[error("database init failed: path={path} message={message}")]
    DatabaseInitFailed { path: String, message: String },
}
