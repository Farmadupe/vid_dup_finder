use std::ffi::OsString;
use std::path::PathBuf;

use vid_dup_finder_lib::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReportVerbosity {
    Quiet,
    Default,
    Verbose,
}

#[derive(Debug, Clone)]
pub struct OutputCfg {
    pub print_unique: bool,
    pub print_duplicates: bool,
    pub json_output: bool,
    pub output_thumbs_dir: Option<PathBuf>,

    pub verbosity: ReportVerbosity,

    pub gui: bool,
    pub gui_trash_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct DirCfg {
    pub cand_dirs: Vec<PathBuf>,
    pub ref_dirs: Vec<PathBuf>,
    pub excl_dirs: Vec<PathBuf>,
    pub excl_exts: Vec<OsString>,
}

#[derive(Debug, Clone)]
pub struct CacheCfg {
    pub cache_path: Option<PathBuf>,
    pub no_update_cache: bool,
}

#[derive(Debug, Clone)]
pub struct AppCfg {
    pub cache_cfg: CacheCfg,
    pub dir_cfg: DirCfg,

    pub output_cfg: OutputCfg,

    pub update_cache_only: bool,
    pub tolerance: NormalizedTolerance,
}
