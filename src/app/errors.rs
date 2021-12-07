use std::path::PathBuf;

use thiserror::Error;
use vid_dup_finder_lib::*;

use video_hash_filesystem_cache::*;

#[derive(Error, Debug)]
pub enum AppError {
    /////////////////////////////////
    // Argument parsing
    #[error("Args file not found at {0}")]
    ArgsFileNotFound(PathBuf, #[source] std::io::Error),

    #[error("Failed to parse args file at given location: {0}: {1}")]
    ArgsFileParse(PathBuf, String),

    #[error("could not parse provided spatial tolerance: {0}")]
    ParseTolerance(String),

    /////////////////////////////////
    //Impossible combination of --files, --with-refs --exclude given.
    //It's important to get the wording of these right because these errors
    //are very easy to trigger.
    #[error("Path occurs in both --files and --with-refs: {0}")]
    PathInFilesAndRefs(PathBuf),

    #[error("Path in --files is excluded by --exclude. Path: {src_path}, Exclusion: {excl_path}")]
    SrcPathExcludedError {
        src_path: PathBuf,
        excl_path: PathBuf,
    },

    #[error(
        "Path in --with-refs is excluded by --exclude. Path: {src_path}, Exclusion: {excl_path}"
    )]
    RefPathExcludedError {
        src_path: PathBuf,
        excl_path: PathBuf,
    },

    #[error("Path in --files not found: {0}")]
    CandPathNotFoundError(PathBuf),

    #[error("Path in --with-refs not found: {0}")]
    RefPathNotFoundError(PathBuf),

    #[error("Path in --exclude not found: {0}")]
    ExclPathNotFoundError(PathBuf),

    /////////////////////////////////
    //Other file projection problems
    #[error("Video file search error, at path: {1}")]
    FileSearchError(PathBuf, walkdir::Error),

    /////////////////////////////////
    //hash cache problems
    #[error(transparent)]
    CacheErrror(#[from] VdfCacheError),

    #[error("Hash Creation Error: {0}")]
    CreateHashError(#[from] HashCreationErrorKind),

    /////////////////////////////////
    //gui
    #[error("Failed to start the GUI")]
    #[allow(dead_code)] // variant is unused when gui is not compiled
    GuiStartError,

    #[error(
        "Ffmpeg command not found. Vid Dup Finder cannot run unless Ffmpeg is installed:
* Debian-based systems: 
    # apt-get install ffmpeg
* Yum-based systems: 
    # yum install ffmpeg
* Windows:
    1) Download the correct installer from <https://ffmpeg.org/download.html>
    2) run the installer and install ffmpeg to any directory
    3) add the directory into the PATH environment variable"
    )]
    FfmpegNotFound,
}

impl AppError {
    pub fn from_cand_exclusion_error(e: FileProjectionError) -> Self {
        match e {
            FileProjectionError::SrcPathExcluded { src_path, excl_path } => Self::SrcPathExcludedError {src_path, excl_path},
            _ => panic!("AppError::from_cand_exclusion_error called with incorrect variant of FileProjectionError. Expected FileProjectionError::SrcPathExcludedError")
        }
    }

    pub fn from_ref_exclusion_error(e: FileProjectionError) -> Self {
        match e {
            FileProjectionError::SrcPathExcluded { src_path, excl_path } => Self::RefPathExcludedError {src_path, excl_path},
            _ => panic!("AppError::from_ref_exclusion_error called with incorrect variant of FileProjectionError. Expected FileProjectionError::SrcPathExcludedError")
        }
    }
}
