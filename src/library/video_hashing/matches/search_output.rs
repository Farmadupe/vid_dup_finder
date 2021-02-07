use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use rayon::prelude::*;

#[cfg(feature = "gui")]
use super::match_group_resolution_thunk::ResolutionThunk;
use super::MatchGroup;
use crate::library::{concrete_cachers::DupFinderCache, Tolerance};

#[derive(Debug, Clone)]
pub struct SearchOutput {
    //A collection of individual matches, and a HashSet of each of the files contained within (required for performance)
    dup_groups: Vec<MatchGroup>,
    dup_files: HashSet<PathBuf>,
    unique_files: HashSet<PathBuf>,

    //record whether there were any references in the search.
    search_included_references: bool,
}

impl SearchOutput {
    pub fn new(
        dup_groups: Vec<MatchGroup>,
        unique_files: impl IntoIterator<Item = impl AsRef<Path>>,
        search_included_references: bool,
    ) -> Self {
        let unique_files = unique_files.into_iter().map(|p| p.as_ref().to_path_buf()).collect();

        let entries_set = dup_groups
            .iter()
            .flat_map(|group| group.duplicates().map(&Path::to_path_buf))
            .collect::<HashSet<_>>();

        Self {
            dup_groups,
            unique_files,
            dup_files: entries_set,
            search_included_references,
        }
    }

    pub fn dup_groups(&self) -> impl Iterator<Item = &MatchGroup> {
        self.dup_groups.iter()
    }

    pub fn contains(&self, cand: &Path) -> bool {
        self.dup_files.contains(cand)
    }

    pub fn dup_hashes(&self) -> impl Iterator<Item = &PathBuf> {
        self.dup_files.iter()
    }
    pub fn unique_hashes(&self) -> impl Iterator<Item = &PathBuf> {
        self.unique_files.iter()
    }

    pub fn len(&self) -> usize {
        self.dup_groups.len()
    }

    #[cfg(feature = "gui")]
    pub fn create_resolution_thunks(&self, cache: &DupFinderCache) -> Vec<ResolutionThunk> {
        self.dup_groups
            .par_iter()
            .map(|group| group.create_resolution_thunk(cache))
            .collect()
    }

    pub fn dups_with_lowest_pngsize(&self, cache: &DupFinderCache) -> Vec<PathBuf> {
        self.dup_groups
            .iter()
            .flat_map(|group| group.dups_with_lowest_pngsize(cache))
            .collect()
    }

    pub fn affirmed(&self, cache: &DupFinderCache) -> Self {
        let new_dup_groups = self.dup_groups.par_iter().flat_map(|mg| mg.affirmed(cache)).collect();

        let ret = Self::new(
            new_dup_groups,
            self.unique_files.clone(),
            self.search_included_references,
        );

        ret
    }

    pub fn false_positives(&self, cache: &DupFinderCache) -> Self {
        //first affirm all dupgroups. Peel off groups for which affirmation failed.
        let false_pos_groups = self
            .dup_groups
            .par_iter()
            .filter(|group| group.affirmed(cache).len() != 1)
            .cloned()
            .collect();

        let ret = Self::new(
            false_pos_groups,
            self.unique_files.clone(),
            self.search_included_references,
        );

        ret
    }

    pub fn cartesian_product(self, tol: Tolerance, dct_cache: &DupFinderCache) -> Self {
        let self_cartesian = self
            .dup_groups
            .into_iter()
            .flat_map(|group| group.cartesian_product(tol, dct_cache))
            .collect();

        Self::new(
            self_cartesian,
            self.unique_files.clone(),
            self.search_included_references,
        )
    }
}
