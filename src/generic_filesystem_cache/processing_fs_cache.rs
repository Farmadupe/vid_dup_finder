use std::{
    borrow::Borrow,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use rayon::prelude::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use FsCacheErrorKind::*;

use super::{
    base_fs_cache::BaseFsCache,
    errors::{FsCacheErrorKind, FsCacheResult},
};
use crate::library::file_set::FileSet;

#[derive(Serialize, Deserialize, Clone)]
struct MtimeCacheEntry<T> {
    cache_mtime: SystemTime,
    value: T,
}

pub struct ProcessingFsCache<T> {
    base_cache: BaseFsCache<MtimeCacheEntry<T>>,
    processing_fn: Box<dyn Fn(PathBuf) -> T + Send + Sync>,
}

impl<T> ProcessingFsCache<T>
where
    T: DeserializeOwned + Serialize + Send + Sync + Clone,
{
    pub fn new(
        cache_save_threshold: u32,
        cache_path: PathBuf,
        processing_fn: Box<dyn Fn(PathBuf) -> T + Send + Sync>,
    ) -> FsCacheResult<Self> {
        match BaseFsCache::new(cache_save_threshold, cache_path) {
            Ok(base_cache) => Ok(Self {
                base_cache,
                processing_fn,
            }),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self) -> FsCacheResult<()> {
        self.base_cache.save()
    }

    fn force_insert(&self, key: impl Borrow<PathBuf>, mtime: SystemTime) -> FsCacheResult<T> {
        let k = key.borrow().clone();

        let value = (self.processing_fn)(k.clone());
        let cache_entry = MtimeCacheEntry {
            cache_mtime: mtime,
            value,
        };
        self.base_cache.insert(k, cache_entry)?;

        self.get(key)
    }

    pub fn remove(&self, key: impl AsRef<Path>) -> FsCacheResult<()> {
        info!(target: "generic_cache", "Removing {:?}, {}", key.as_ref(), self.keys().len());
        self.base_cache.remove(key)
    }

    pub fn get(&self, key: impl Borrow<PathBuf>) -> FsCacheResult<T> {
        match self.base_cache.get(key.borrow()) {
            Ok(MtimeCacheEntry { cache_mtime: _, value }) => Ok(value),
            Err(e) => Err(e),
        }
    }

    fn get_mtime(&self, key: impl Borrow<PathBuf>) -> FsCacheResult<SystemTime> {
        let key = key.borrow();
        let metadata = match fs::metadata(&key) {
            Ok(metadata) => metadata,
            Err(e) => {
                return Err(CacheItemIoError {
                    src: format!("{}", e),
                    path: key.clone(),
                })
            }
        };

        let fs_mtime = match metadata.modified() {
            Ok(fs_mtime) => fs_mtime,
            Err(e) => {
                return Err(CacheItemIoError {
                    src: format!("{}", e),
                    path: key.clone(),
                })
            }
        };

        Ok(fs_mtime)
    }

    pub fn get_insert(&self, key: impl Borrow<PathBuf>) -> FsCacheResult<T> {
        let val_is_stale = |key: &Path| -> FsCacheResult<(bool, SystemTime)> {
            let cache_mtime = self.base_cache.get(key.to_path_buf())?.cache_mtime;
            let fs_mtime = self.get_mtime(key.to_path_buf())?;

            let is_stale = fs_mtime != cache_mtime;

            Ok((is_stale, fs_mtime))
        };

        //insertion required if:
        // * Item is not in cache.
        // * Cached item is out of date.
        let mut insert_required = !self.contains_key(key.borrow());

        let mut fs_mtime: Option<SystemTime> = None;

        if !insert_required {
            let x = val_is_stale(key.borrow())?;
            insert_required = x.0;
            fs_mtime = Some(x.1);
        }

        if insert_required {
            let fs_mtime = match fs_mtime {
                Some(fs_mtime) => fs_mtime,
                None => self.get_mtime(key.borrow())?,
            };

            self.force_insert(key.borrow(), fs_mtime)?;
        }

        self.get(key)
    }

    pub fn force_reload(&self, key: impl Borrow<PathBuf>) -> FsCacheResult<T> {
        self.force_insert(key.borrow(), self.get_mtime(key.borrow())?)
    }

    pub fn contains_key(&self, key: impl Borrow<PathBuf>) -> bool {
        self.base_cache.contains_key(key)
    }

    pub fn keys(&self) -> Vec<PathBuf> {
        self.base_cache.keys()
    }

    pub fn len(&self) -> usize {
        self.base_cache.len()
    }

    pub fn update_from_fs(&self, filename_enumerator: &mut FileSet) -> Result<Vec<FsCacheErrorKind>, FsCacheErrorKind> {
        let mut errs_ret = vec![];

        //First add items which are new or changed in the filesystem.
        let loading_paths = {
            let (loading_paths, errs) = filename_enumerator.enumerate_from_fs()?;
            errs_ret.extend(errs.into_iter().map(FsCacheErrorKind::from));
            loading_paths.to_owned()
        };

        //Now delete those items which have disappeared from the filesystem..
        let errs = self
            .keys()
            .into_par_iter()
            .filter(|key| filename_enumerator.includes(key) && !key.exists())
            .filter_map(|key| self.remove(key).err());
        errs_ret.par_extend(errs);

        let errs = loading_paths
            .into_par_iter()
            .filter_map(|path| self.get_insert(path.borrow()).err())
            .collect::<Vec<_>>();
        errs_ret.extend(errs);

        Ok(errs_ret)
    }
}
