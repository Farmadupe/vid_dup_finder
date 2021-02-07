use std::{fmt::Debug, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub(crate) type FsCacheResult<T> = Result<T, FsCacheErrorKind>;

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum FsCacheErrorKind {
    #[error("Error accessing cache storage file {path}: {src}")]
    CacheFileIoError { src: String, path: PathBuf },

    #[error("Failed to enumerate files from fs")]
    FileEnumerationError {
        #[from]
        source: crate::library::file_set::FileSetError,
    },

    #[error("IO error accessing {src}: {path}")]
    CacheItemIoError { src: String, path: PathBuf },

    #[error("Key missing from cache: {0}")]
    KeyMissingError(PathBuf),

    #[error("Failed to serialize items from cache file {path}: {src}")]
    SerializationError { src: String, path: PathBuf },

    #[error("Failed to deserialize items from cache file {path}: {src}")]
    DeserializationError { src: String, path: PathBuf },
}
