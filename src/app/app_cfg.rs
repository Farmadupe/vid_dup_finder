use crate::library::{CacheCfg, SearchCfg};

#[derive(Debug, Clone)]
pub struct OutputCfg {
    pub print_unique: bool,
    pub print_duplicates: bool,
    pub print_worst_entries: bool,
    pub json_output: bool,

    pub quiet: bool,
    pub very_quiet: bool,

    pub gui: bool,
}

#[derive(Debug, Clone)]
pub struct AppCfg {
    pub cache_cfg: CacheCfg,
    pub search_cfg: SearchCfg,

    pub output_cfg: OutputCfg,
    pub debug_falsepos: bool,
    pub debug_print_bad_hashes: bool,
}
