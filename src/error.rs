//! Single error type for the whole app. Commands and TUI return this.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TodoError {
    #[error("storage failure: {0}")]
    Storage(String),

    #[error("task {0} not found")]
    NotFound(i64),

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("could not locate data directory for this platform")]
    NoDataDir,

    #[error("terminal/IO failure: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, TodoError>;
