use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::concrete_cachers::{FetchOperationError, HashStatsCreationError};
use crate::generic_filesystem_cache::errors::FsCacheErrorKind;

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum LibError {
    #[error("Paths are present in dup list and refernce dirs: {0}")]
    SamePathInRefAndCandError(String),

    // #[error("FFMPEG not found")]
    // FfmpegMissingError,
    #[error("Error processing video: {0}")]
    ProcessingError(#[from] HashStatsCreationError),

    #[error("Error in  cache: {0}")]
    CacheError(#[from] FsCacheErrorKind),

    #[error("Failed to resolve thunk: {0}")]
    ResolutionError(String),
}

impl From<FetchOperationError> for LibError {
    fn from(e: FetchOperationError) -> Self {
        match e {
            FetchOperationError::NotVideo => panic!(),
            FetchOperationError::ShortVideo => panic!(),
            FetchOperationError::ProcessingError(e) => Self::ProcessingError(e),
            FetchOperationError::CacheError(e) => Self::CacheError(e),
        }
    }
}
