use std::path::PathBuf;

mod base_fs_cache;
pub mod errors;
pub mod processing_fs_cache;

//Types defining the on-disk format of the filesystem cacher.
type CacheDiskFormat<T> = std::collections::HashMap<PathBuf, T>;
