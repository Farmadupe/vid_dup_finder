use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    result::Result,
};

use itertools::{Either::*, Itertools};
use rayon::prelude::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use walkdir::WalkDir;

use crate::generic_filesystem_cache::processing_fs_cache::ProcessingFsCache;

#[derive(Error, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FileSetError {
    #[error("Path not found: {0}")]
    PathNotFoundError(PathBuf),

    #[error("File enumeration failed")]
    EnumerationError(String),
}

impl From<walkdir::Error> for FileSetError {
    fn from(e: walkdir::Error) -> Self {
        Self::EnumerationError(format!("{}", e))
    }
}

pub struct FileSet {
    source_paths: Vec<PathBuf>,
    excl_paths: Vec<PathBuf>,
    enumerated: bool,
    enumerated_paths: Vec<PathBuf>,
}

impl FileSet {
    pub fn new(
        source_paths: impl IntoIterator<Item = impl AsRef<Path>>,
        excl_paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> Self {
        let source_paths = source_paths.into_iter().map(|p| p.as_ref().to_path_buf()).collect();

        let excl_paths = excl_paths.into_iter().map(|p| p.as_ref().to_path_buf()).collect();

        Self {
            source_paths,
            excl_paths,
            enumerated: false,
            enumerated_paths: Default::default(),
        }
    }

    pub fn includes(&self, cand: impl AsRef<Path>) -> bool {
        let cand = cand.as_ref();
        any_item_includes(&self.source_paths, cand) && !any_item_includes(&self.excl_paths, cand)
    }

    pub fn enumerate_from_fs(&mut self) -> Result<(&Vec<PathBuf>, Vec<FileSetError>), FileSetError> {
        if !self.enumerated {
            match self.enumerate_from_fs_inner() {
                Ok(errs) => Ok((&self.enumerated_paths, errs)),
                Err(fatal_error) => Err(fatal_error),
            }
        } else {
            Ok((&self.enumerated_paths, Default::default()))
        }
    }

    fn enumerate_from_fs_inner(&mut self) -> Result<Vec<FileSetError>, FileSetError> {
        use FileSetError::*;

        //we will return a fatal error if any directory/file that the user
        //has specified does not exist.
        for path in self.source_paths.iter().chain(self.excl_paths.iter()) {
            if !path.exists() {
                return Err(PathNotFoundError(path.to_owned()));
            }
        }

        let paths_to_enumerate =
            self.source_paths
                .iter()
                .flat_map(WalkDir::new)
                .filter(|dir_entry_res| match &dir_entry_res {
                    Ok(dir_entry) => self.should_keep(&dir_entry),
                    Err(_) => true,
                });

        let (mut enumerated_paths, loading_errors): (Vec<_>, Vec<_>) = paths_to_enumerate
            .map(|dir_entry_res| dir_entry_res.map(|dir_entry| dir_entry.path().to_path_buf()))
            .partition_map(|dir_entry_res| match dir_entry_res {
                Ok(src_path) => Left(src_path),
                Err(e) => Right(e.into()),
            });

        //sort is required for deterministic outputs.
        enumerated_paths.sort();
        enumerated_paths.dedup();

        self.enumerated_paths = enumerated_paths;

        Ok(loading_errors.into_iter().collect())
    }

    pub fn enumerate_from_cache<T>(&mut self, cache: &ProcessingFsCache<T>) -> &Vec<PathBuf>
    where
        T: DeserializeOwned + Serialize + Send + Sync + Clone,
    {
        if !self.enumerated {
            self.enumerate_from_cache_inner(cache);
        }

        &self.enumerated_paths
    }

    fn enumerate_from_cache_inner<T>(&mut self, cache: &ProcessingFsCache<T>)
    where
        T: DeserializeOwned + Serialize + Send + Sync + Clone,
    {
        self.enumerated_paths = cache
            .keys()
            .into_par_iter()
            .filter(|k| any_item_includes(&self.source_paths, k) && !any_item_includes(&self.excl_paths, k))
            .collect()
    }

    const EXCL_EXTS: [&'static str; 5] = ["png", "jpg", "jpeg", "gif", "txt"];
    fn should_keep(&self, x: &walkdir::DirEntry) -> bool {
        x.path().is_file()
            && !any_item_includes(&self.excl_paths, x.path())
            && !Self::EXCL_EXTS.iter().any(|&ext| {
                x.path()
                    .extension()
                    .map(OsStr::to_string_lossy)
                    .unwrap_or_default()
                    .to_lowercase()
                    == ext
            })
    }
}

pub fn is_ancestor_of(reference: impl AsRef<Path>, cand: impl AsRef<Path>) -> bool {
    cand.as_ref().ancestors().any(|anc| reference.as_ref() == anc)
}

fn any_item_includes(references: impl IntoIterator<Item = impl AsRef<Path>>, cand: impl AsRef<Path>) -> bool {
    references.into_iter().any(|r| is_ancestor_of(r, &cand))
}
