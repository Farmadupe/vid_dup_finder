use std::{
    borrow::Borrow,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
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

    fn fs_mtime(key: &Path) -> FsCacheResult<SystemTime> {
        let metadata = match fs::metadata(&key) {
            Ok(metadata) => metadata,
            Err(e) => {
                return Err(CacheItemIoError {
                    src: format!("{}", e),
                    path: key.to_path_buf(),
                })
            }
        };

        let fs_mtime = match metadata.modified() {
            Ok(fs_mtime) => fs_mtime,
            Err(e) => {
                return Err(CacheItemIoError {
                    src: format!("{}", e),
                    path: key.to_path_buf(),
                })
            }
        };

        Ok(fs_mtime)
    }

    // helper function to get whether a particular path has been updated in the filesystem.
    // Contains a hacky workaround for a problem where SSHFS (and presumably FUSE underneath)
    // reports different mtimes for files compared to a backing BTRFS filesystem (FUSE/sshfs probably
    // reports less granular mtimes?), where a file will only be considered stale if the mtime
    // is different by more than DURATION_TOLERANCE.
    fn val_is_stale(&self, key: &Path) -> FsCacheResult<(bool, SystemTime)> {
        const DURATION_TOLERANCE_SECS: i64 = 2;

        let cache_mtime = self.base_cache.get(key)?.cache_mtime;
        let fs_mtime = Self::fs_mtime(key)?;

        //original implementation used the following code, which produced errors as SystemTime::duration_since
        // appears to return an error if only the nanos portion of the fields differ
        // let time_difference = if cache_mtime < fs_mtime {
        //     cache_mtime.duration_since(fs_mtime)
        // } else {
        //     fs_mtime.duration_since(cache_mtime)
        // };

        // To fix the problem the durations are converted seconds since unix epoch.
        let cache_mtime_secs = cache_mtime.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
        let fs_mtime_secs = fs_mtime.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;

        let is_stale = (cache_mtime_secs - fs_mtime_secs).abs() > DURATION_TOLERANCE_SECS;

        Ok((is_stale, fs_mtime))
    }

    pub fn get_insert(&self, key: impl Borrow<PathBuf>) -> FsCacheResult<T> {
        //insertion required if:
        // * Item is not in cache.
        // * Cached item is out of date.
        let key_present = self.contains_key(key.borrow());

        let (key_stale, fs_mtime) = if key_present {
            let (key_stale, fs_mtime) = self.val_is_stale(key.borrow())?;
            (Some(key_stale), Some(fs_mtime))
        } else {
            (None, None)
        };

        if let Some(true) = key_stale {
            println!("key_present: {}, key_stale: {:?}", key_present, key_stale);
        }

        if !key_present || matches!(key_stale, Some(true)) {
            let fs_mtime = match fs_mtime {
                Some(fs_mtime) => fs_mtime,
                None => Self::fs_mtime(key.borrow())?,
            };

            self.force_insert(key.borrow(), fs_mtime)?;
        }

        self.get(key)
    }

    pub fn force_reload(&self, key: impl Borrow<PathBuf>) -> FsCacheResult<T> {
        self.force_insert(key.borrow(), Self::fs_mtime(key.borrow())?)
    }

    pub fn contains_key(&self, key: &Path) -> bool {
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
