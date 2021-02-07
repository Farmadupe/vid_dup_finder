pub mod concrete_cachers;
mod dct_hasher;
pub mod definitions;
pub mod errors;
pub mod file_set;
mod lib_fns;
mod library_cfg;
mod search_structures;
mod utils;
mod video_hashing;

//internal exports
//exports to app and tests
pub(crate) use concrete_cachers::DupFinderCache;
pub(crate) use definitions::DEFAULT_TOLERANCE;
//external exports
pub use errors::LibError;
pub(self) use file_set::FileSet;
pub use lib_fns::{
    find_all_matches, load_disk_caches, reload_non_videos, retry_load_failures, update_dct_cache_from_fs,
};
pub use library_cfg::{CacheCfg, FfmpegCfg, SearchCfg};
pub(crate) use utils::{ffmpeg_ops, img_ops};
pub use video_hashing::{matches::SearchOutput, temporal_hash::TemporalHash, video_dup_finder::VideoDupFinder};
pub(crate) use video_hashing::{
    temporal_hash::{Distance, HashCreationErrorKind},
    video_stats::{StatsCalculationError, VideoStats},
};

#[cfg(feature = "gui")]
pub(crate) use crate::library::video_hashing::matches::ResolutionThunk;

/////////////////////
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Tolerance {
    pub spatial: f64,
    pub temporal: f64,
}

impl From<&search_structures::ScaledTolerance> for Tolerance {
    fn from(scaled: &search_structures::ScaledTolerance) -> Self {
        Self {
            spatial: scaled.spatial as f64 / definitions::TOLERANCE_SCALING_FACTOR,
            temporal: scaled.temporal as f64 / definitions::TOLERANCE_SCALING_FACTOR,
        }
    }
}
