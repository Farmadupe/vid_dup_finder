use std::path::PathBuf;

use super::Tolerance;

#[derive(Debug, Clone)]
pub struct FfmpegCfg {
    pub framerate: String,
    pub dimensions_x: u32,
    pub dimensions_y: u32,
    pub num_frames: u32,
    pub cropdetect: bool,
}

#[derive(Debug, Clone)]
pub struct SearchCfg {
    pub cand_dirs: Vec<PathBuf>,
    pub ref_dirs: Vec<PathBuf>,
    pub excl_dirs: Vec<PathBuf>,
    pub vec_search: bool,
    pub determ: bool,
    pub affirm_matches: bool,
    pub tolerance: Tolerance,
    pub cartesian: bool,
}

#[derive(Debug, Clone)]
pub struct CacheCfg {
    pub cache_dir: PathBuf,

    pub no_refresh_caches: bool,
    pub debug_reload_errors: bool,
    pub debug_reload_non_videos: bool,
}
