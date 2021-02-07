use std::{borrow::Borrow, fmt::Debug, path::{Path, PathBuf}, sync::{
        atomic::{AtomicU32, Ordering::Relaxed},
        RwLock,
    }};

use serde::{de::DeserializeOwned, Serialize};

use crate::generic_filesystem_cache::{
    errors::{
        FsCacheErrorKind::{self, *},
        FsCacheResult,
    },
    CacheDiskFormat,
};

#[derive(Default, Debug)]
pub struct BaseFsCache<T> {
    loaded_from_disk: bool,
    cache_save_threshold: u32,
    cache_modified_count: AtomicU32,
    cache_path: PathBuf,
    cache: RwLock<CacheDiskFormat<T>>,
}

impl<T> BaseFsCache<T>
where
    T: DeserializeOwned + Serialize + Send + Sync + Clone,
{
    pub fn new(cache_save_threshold: u32, cache_path: PathBuf) -> FsCacheResult<Self> {
        let mut ret = Self {
            loaded_from_disk: false,
            cache_save_threshold,
            cache_modified_count: Default::default(),
            cache_path,
            cache: Default::default(),
        };

        match ret.load_cache_from_disk() {
            Ok(()) => Ok(ret),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self) -> FsCacheResult<()> {
        let modified_count = self.cache_modified_count.load(Relaxed);
        if modified_count != 0 {
            self.save_inner()
        } else {
            Ok(())
        }
    }

    fn save_inner(&self) -> FsCacheResult<()> {
        use std::io::BufWriter;

        //The cache file and its directory may not exist yet. So first create the directory
        //first if necessary.
        if !&self.cache_path.exists() {
            if let Some(ref parent_dir) = self.cache_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent_dir) {
                    return Err(CacheFileIoError {
                        src: format!("{}", e),
                        path: self.cache_path.clone(),
                    });
                }
            }
        }

        //If the application dies or gets killed while saving, we risk losing the cache.
        //So we will first save the cache to a temporary file and rename it into the real
        //cache file.
        let temp_store_path = self.cache_path.with_extension("tmp");

        info!(
            target: "cache_changes",
            "saving updated cache at {} of size {}",

            self.cache_path.display(),
            match self.cache.read() {
                Err(_) => unreachable!(),
                Ok(cache) => cache.len()
            }
        );

        let temp_cache_file = match std::fs::File::create(&temp_store_path) {
            Ok(temp_cache_file) => Ok(temp_cache_file),
            Err(e) => Err(CacheFileIoError {
                src: format!("{}", e),
                path: self.cache_path.to_path_buf(),
            }),
        }?;

        let mut cache_buf = BufWriter::new(temp_cache_file);

        let readable_cache = match self.cache.read() {
            Ok(cache) => cache,
            Err(_) => unreachable!(),
        };

        if let Err(e) = bincode::serialize_into(&mut cache_buf, &*readable_cache) {
            return Err(SerializationError {
                src: format!("{}", e),
                path: self.cache_path.to_path_buf(),
            });
        }

        let temp_cache_file = match cache_buf.into_inner() {
            Err(e) => {
                return Err(CacheFileIoError {
                    src: format!("{}", e),
                    path: self.cache_path.to_path_buf(),
                })
            }
            Ok(x) => x,
        };

        if let Err(e) = temp_cache_file.sync_all() {
            return Err(CacheFileIoError {
                src: format!("{}", e),
                path: self.cache_path.to_path_buf(),
            });
        }

        //now move the store to replace the old one.
        if let Err(e) = std::fs::rename(temp_store_path, &self.cache_path) {
            return Err(CacheFileIoError {
                src: format!("{}", e),
                path: self.cache_path.to_path_buf(),
            });
        }

        Ok(())
    }

    fn load_cache_from_disk(&mut self) -> FsCacheResult<()> {
        //Try and read from disk. If there is nothing  available, this is not an error.
        //It just means that no cached values can be used. If so then go ahead and return early
        //as there is no deserialization to do.
        if !&self.cache_path.exists() {
            info!(target: "cache_changes",
                "No existing cache file found at {}.", self.cache_path.display()
            );
            self.cache = Default::default();
            self.loaded_from_disk = true;
            return Ok(());
        }

        let f = match std::fs::File::open(&self.cache_path) {
            Ok(f) => f,
            Err(e) => {
                return Err(CacheFileIoError {
                    src: format!("{}", e),
                    path: self.cache_path.clone(),
                })
            }
        };

        let reader = std::io::BufReader::new(f);
        let decode_result = bincode::deserialize_from(reader);

        //we may fail to read the hash file. This most likely to occur in development if <T> is changed.
        match decode_result {
            Ok(cache_file_data) => {
                self.cache = cache_file_data;
                self.loaded_from_disk = true;
                Ok(())
            }
            Err(e) => Err(DeserializationError {
                src: format!("{}", e),
                path: self.cache_path.to_path_buf(),
            }),
        }
    }

    /////////////////////////////
    // Wrappers for HashMap.
    /////////////////////////////

    pub fn insert(&self, key: PathBuf, item: T) -> FsCacheResult<()> {
        let cache_modified_count = self.cache_modified_count.fetch_add(1, Relaxed);

        info!(target: "cache_changes",
            "inserting : {}",
            key.display()
        );
        let cache_entry = item;
        {
            let mut writeable_cache = match self.cache.write() {
                Ok(cache) => cache,
                Err(_) => unreachable!(),
            };
            writeable_cache.insert(key, cache_entry);
        }
        self.update_transaction_count_and_save_if_necessary(cache_modified_count)
    }

    pub fn remove(&self, key: impl AsRef<Path>) -> FsCacheResult<()> {
        {
            //info!(target: "cache_changes", "Removing from cache: {}", key.borrow().display());
            let mut writeable_cache = match self.cache.write() {
                Ok(cache) => cache,
                Err(_) => unreachable!(),
            };
            writeable_cache.remove(key.as_ref());
        }
        let cache_modified_count = self.cache_modified_count.fetch_add(1, Relaxed);
        self.update_transaction_count_and_save_if_necessary(cache_modified_count)
    }

    fn update_transaction_count_and_save_if_necessary(&self, prev_count: u32) -> FsCacheResult<()> {
        // We need to defend against
        // 1) multiple saves of data when only one should be performed
        // 2) Failing to reset the cache_modified_count to 0. I think we
        // can guarantee both of these things with Relaxed accesses.
        //
        // todo: I think the above two points are true, but we should probably
        // guarantee better behaviour than that. I think at worst here, every
        // operation could trigger a save of the cache as cache_modified_count
        // isn't guaranteed to be sensibly propagated between threads.
        if prev_count == self.cache_save_threshold - 1 {
            self.cache_modified_count.store(0, Relaxed);
            self.save_inner()
        } else {
            Ok(())
        }
    }

    pub fn get(&self, key: impl Borrow<PathBuf>) -> Result<T, FsCacheErrorKind> {
        match self.cache.read() {
            Err(_) => unreachable!(),
            Ok(readable_cache) => match readable_cache.get(key.borrow()) {
                Some(value) => Ok(value.clone()),
                None => Err(FsCacheErrorKind::KeyMissingError(key.borrow().to_path_buf())),
            },
        }
    }

    pub fn contains_key(&self, key: impl Borrow<PathBuf>) -> bool {
        match self.cache.read() {
            Err(_) => unreachable!(),
            Ok(cache) => cache.contains_key(key.borrow()),
        }
    }

    pub fn keys(&self) -> Vec<PathBuf> {
        match self.cache.read() {
            Ok(cache) => cache,
            Err(_) => unreachable!(),
        }
        .keys()
        .cloned()
        .collect()
    }

    pub fn len(&self) -> usize {
        match self.cache.read() {
            Ok(cache) => cache.len(),
            Err(_) => unreachable!(),
        }
    }
}
