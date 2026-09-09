use std::io;

/// Errors from optioncalendar-core calendar operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("invalid event: {0}")]
    InvalidEvent(String),

    #[error("invalid date '{0}': use YYYY-MM-DD or YYYY-MM-DDTHH:MM")]
    InvalidDate(String),
}

pub type Result<T> = std::result::Result<T, Error>;
