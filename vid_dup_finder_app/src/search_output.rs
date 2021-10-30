use std::path::Path;

use vid_dup_finder_lib::*;

// #[cfg(all(target_family = "unix", feature = "gui"))]
// use super::match_group_resolution_thunk::ResolutionThunk;

#[derive(Debug, Clone)]
pub struct SearchOutput {
    dup_groups: Vec<MatchGroup>,
}

impl SearchOutput {
    pub fn new(dup_groups: Vec<MatchGroup>) -> Self {
        Self { dup_groups }
    }

    pub fn dup_groups(&self) -> impl Iterator<Item = &MatchGroup> {
        self.dup_groups.iter()
    }

    pub fn dup_paths(&self) -> impl Iterator<Item = &Path> {
        self.dup_groups.iter().flat_map(|group| group.duplicates())
    }
}
