use std::path::{Path, PathBuf};

use ffmpeg_ops::{create_images_into_memory, is_video_file, FfmpegErrorKind};
use img_ops::ImgOpsError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::utils::framified_video::FramifiedVideo;
use crate::{
    generic_filesystem_cache::{errors::FsCacheErrorKind, processing_fs_cache::ProcessingFsCache},
    library::*,
};

pub mod dct_hash_loader;
pub mod frame_loader;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedVideoData {
    pub hash: TemporalHash,
    pub stats: VideoStats,
}

#[derive(Clone, Debug, Error, Serialize, Deserialize)]
pub enum HashStatsCreationError {
    #[error("Hash Creation Processing Error: {0}")]
    Hash(#[from] HashCreationErrorKind),

    #[error("Stats Calculation Processing Error: {0}")]
    Stats(#[from] StatsCalculationError),
}

#[derive(Clone, Debug, Error, Serialize, Deserialize)]
pub enum ImgOrFfmpegError {
    #[error(transparent)]
    Img(#[from] ImgOpsError),

    #[error(transparent)]
    Ffmpeg(#[from] FfmpegErrorKind),
}

#[derive(Clone, Debug, Error, Serialize, Deserialize)]
pub enum FetchOperationError {
    #[error("Not a video")]
    NotVideo,

    #[error("Short video")]
    ShortVideo,

    #[error("Error while processing video: {0}")]
    ProcessingError(#[from] HashStatsCreationError),

    #[error("Cache Error: {0}")]
    CacheError(#[from] FsCacheErrorKind),
}

impl FetchOperationError {
    pub fn is_processing_error(&self) -> bool {
        matches!(&self, Self::ProcessingError(_))
    }

    pub fn is_not_video(&self) -> bool {
        matches!(&self, Self::NotVideo)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum DupFinderCacheEntry {
    NotVideo,
    ShortVideo,
    Video(CachedVideoData),
    ProcessingError(HashStatsCreationError),
}

impl
    From<(
        Result<TemporalHash, HashCreationErrorKind>,
        Result<VideoStats, StatsCalculationError>,
    )> for DupFinderCacheEntry
{
    fn from(
        (hash_creation_result, stats_creation_result): (
            Result<TemporalHash, HashCreationErrorKind>,
            Result<VideoStats, StatsCalculationError>,
        ),
    ) -> Self {
        match (hash_creation_result, stats_creation_result) {
            (Ok(hash), Ok(stats)) => Self::Video(CachedVideoData { hash, stats }),
            (Ok(_hash), Err(stats_err)) => Self::ProcessingError(stats_err.into()),
            (Err(HashCreationErrorKind::VideoTooShortError(_)), Ok(_stats)) => Self::ShortVideo,
            (Err(hash_err), Ok(_stats)) => Self::ProcessingError(hash_err.into()),
            (Err(hash_err), Err(_stats_err)) => Self::ProcessingError(hash_err.into()),
        }
    }
}

pub struct DupFinderCache(ProcessingFsCache<DupFinderCacheEntry>);

impl DupFinderCache {
    pub fn new(
        cache_save_thresold: u32,
        cache_path: PathBuf,
    ) -> crate::generic_filesystem_cache::errors::FsCacheResult<Self> {
        let hash_fn = Self::create_load_fn();
        let ret = ProcessingFsCache::new(cache_save_thresold, cache_path, hash_fn)?;
        Ok(Self(ret))
    }

    pub fn get_hash<P: AsRef<Path>>(&self, src_path: P) -> Result<TemporalHash, FetchOperationError> {
        let nested_result = self.0.get(src_path.as_ref().to_path_buf());
        match flatten_fetch_result(nested_result) {
            Ok(data) => Ok(data.hash),
            Err(e) => Err(e),
        }
    }

    pub fn force_reload_hash<P: AsRef<Path>>(&self, src_path: P) -> Result<TemporalHash, FetchOperationError> {
        let _ = self.0.force_reload(src_path.as_ref().to_path_buf());
        self.get_hash(src_path.as_ref().to_path_buf())
    }

    pub fn get_stats<P: AsRef<Path>>(&self, src_path: P) -> Result<VideoStats, FetchOperationError> {
        let nested_result = self.0.get(src_path.as_ref().to_path_buf());
        match flatten_fetch_result(nested_result) {
            Ok(data) => Ok(data.stats),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self) -> Result<(), FsCacheErrorKind> {
        self.0.save()
    }

    pub fn cached_src_paths(&self) -> Vec<PathBuf> {
        self.0
            .keys()
            .into_iter()
            .filter(|src_path| {
                let fetch_result = self.get_hash(src_path);
                matches!(fetch_result, Ok(_) | Err(FetchOperationError::ProcessingError(_)))
            })
            .collect()
    }

    pub fn err_video_paths(&self) -> Vec<PathBuf> {
        self.0
            .keys()
            .into_iter()
            .filter(|src_path| matches!(self.0.get(src_path), Ok(DupFinderCacheEntry::ProcessingError(_))))
            .collect()
    }

    pub fn non_video_paths(&self) -> Vec<PathBuf> {
        self.0
            .keys()
            .into_iter()
            .filter(|src_path| matches!(self.0.get(src_path), Ok(DupFinderCacheEntry::NotVideo)))
            .collect()
    }

    pub fn keys(&self) -> Vec<PathBuf> {
        self.0.keys()
    }

    pub fn contains(&self, key: &Path) -> bool {
        self.0.contains_key(key.to_path_buf())
    }

    pub fn update_from_fs(&self, filename_enumerator: &mut FileSet) -> Result<Vec<FsCacheErrorKind>, FsCacheErrorKind> {
        self.0.update_from_fs(filename_enumerator)
    }

    // expose inner type. Slight hack to allow file_set::enumerate_from_cache to work.
    // (solution would be to define a trait which exposes the correct parts of the inner type, which could be
    // implemented here. But it's not worth it for just this one instance.)
    pub fn inner(&self) -> &ProcessingFsCache<DupFinderCacheEntry> {
        &self.0
    }

    fn create_load_fn() -> Box<dyn Fn(PathBuf) -> DupFinderCacheEntry + Send + Sync> {
        let hash_closure = Self::create_hash_fn();
        let stats_closure = Self::create_stats_fn();

        let closure = move |p: PathBuf| match is_video_file(p.clone()) {
            Ok(true) => {
                let hash_result = hash_closure(p.clone());
                let stats_result = stats_closure(p);
                DupFinderCacheEntry::from((hash_result, stats_result))
            }
            Ok(false) => DupFinderCacheEntry::NotVideo,
            Err(_e) => DupFinderCacheEntry::NotVideo,
            //Err(e) => DupFinderCacheEntry::ProcessingError(HashStatsCreationError::FileDeterminationError(e)),
        };

        Box::new(closure)
    }

    fn create_hash_fn() -> Box<dyn Fn(PathBuf) -> Result<TemporalHash, HashCreationErrorKind> + Send + Sync> {
        let closure = move |p: PathBuf| {
            let frames = Self::load_fn_cropdetect(p.as_path());
            let hash = frames.and_then(|frames| crate::library::concrete_cachers::dct_hash_loader::load(&frames));

            if let Err(ref e) = hash {
                warn!("warning: {}", e);
            }

            hash
        };
        Box::new(closure)
    }

    fn create_stats_fn() -> Box<dyn Fn(PathBuf) -> Result<VideoStats, StatsCalculationError> + Send + Sync> {
        let closure = move |p: PathBuf| {
            let ret = VideoStats::new(p);

            if let Err(ref e) = ret {
                warn!("{}", e);
            }

            ret
        };

        Box::new(closure)
    }

    fn load_fn_cropdetect(file_path: &Path) -> Result<FramifiedVideo, HashCreationErrorKind> {
        let cfg = FfmpegCfg {
            dimensions_x: definitions::RESIZE_IMAGE_X as u32,
            dimensions_y: definitions::RESIZE_IMAGE_Y as u32,
            num_frames: definitions::HASH_NUM_IMAGES as u32,
            framerate: definitions::HASH_FRAMERATE.to_string(),
            cropdetect: true,
        };
        create_images_into_memory(file_path, &cfg).map_err(|e| HashCreationErrorKind::ImgOrFfmpegError {
            path: file_path.to_path_buf(),
            error: e,
        })
    }
}

pub type CacheFetchResult = Result<CachedVideoData, FetchOperationError>;

//helper function to flatten results from fetch operations.
//Would prefer to implement trait "From<Result<DupFinderCacheEntry, FsCacheErrorKind>> for CacheFetchResult"
//but this is not possible until std::ops::Try is stabilized
fn flatten_fetch_result(r: Result<DupFinderCacheEntry, FsCacheErrorKind>) -> CacheFetchResult {
    use FetchOperationError::*;
    match r {
        Ok(DupFinderCacheEntry::NotVideo) => Err(NotVideo),
        Ok(DupFinderCacheEntry::ShortVideo) => Err(ShortVideo),
        Ok(DupFinderCacheEntry::Video(data)) => Ok(data),
        Ok(DupFinderCacheEntry::ProcessingError(e)) => Err(ProcessingError(e)),
        Err(e) => Err(CacheError(e)),
    }
}
