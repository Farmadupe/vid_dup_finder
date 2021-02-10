use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Args file not found at {0}")]
    ArgsFileNotFoundError(PathBuf, #[source] std::io::Error),

    #[error("Failed to parse args file at given location: {0}: {1}")]
    ArgsFileParseError(PathBuf, String),

    #[error("Library error: {0}:")]
    LibError(#[from] crate::library::LibError),

    #[error("could not parse provided spatial tolerance: {0}")]
    ParseSpatialToleranceError(String),

    #[error("could not parse provided temporal tolerance: {0}")]
    ParseTemporalToleranceError(String),
}
