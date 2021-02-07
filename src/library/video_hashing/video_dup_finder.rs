use std::collections::{hash_map::RandomState, HashSet};

use super::matches::MatchGroup;
use crate::library::{
    search_structures::{SearchStructEnum, SimilaritySearch},
    *,
};

pub struct VideoDupFinder {}

impl VideoDupFinder {
    pub fn new() -> Self {
        Self {}
    }

    pub fn find_all(
        &mut self,
        hashes: impl IntoIterator<Item = TemporalHash>,
        tolerance: Tolerance,
        deterministic_search: bool,
        vec_search: bool,
    ) -> Vec<MatchGroup> {
        let mut search_struct = SearchStructEnum::new(vec_search, deterministic_search);
        for hash in hashes {
            search_struct.seed(hash);
        }

        let mut match_groups: Vec<MatchGroup> = vec![];

        let chunk_size = 5_000;

        // trace!("{}", search_struct.len());

        while search_struct.len() > 0 {
            let items_to_match = search_struct.fetch_unmatched_items(chunk_size);

            let matches = search_struct
                .search(&items_to_match, (&tolerance).into(), true)
                .into_iter()
                // Single length matches are meaningless here (because the search structure
                // contains items_to_match -- meaning that the single item in the match is a
                // member of items_to_match -- meaning that there was no match.). So remove them.
                .filter(|group| group.len() > 1)
                .map(|group| group.into_iter().map(|x| x.src_path().to_path_buf()).into())
                .collect::<Vec<MatchGroup>>();

            info!(
                target:"search",
                "Processed {} items of {}. Found {} matches (made of {} videos).",
                items_to_match.len(),
                search_struct.len(),
                matches.len(),
                matches.iter().map(|m| m.len()).sum::<usize>()
            );

            match_groups.extend(matches);
            search_struct = search_struct.into_without_unmatched();
        }

        match_groups
    }

    pub fn find_with_refs(
        &mut self,
        ref_hashes: impl IntoIterator<Item = TemporalHash>,
        new_hashes: impl IntoIterator<Item = TemporalHash>,
        tolerance: Tolerance,
        deterministic_search: bool,
        vec_search: bool,
    ) -> Vec<MatchGroup> {
        let mut search_struct = SearchStructEnum::new(vec_search, deterministic_search);

        let new_hashes = new_hashes.into_iter().collect::<HashSet<_, RandomState>>();

        for hash in &new_hashes {
            search_struct.seed(hash.clone());
        }

        ref_hashes
            .into_iter()
            .flat_map(|ref_hash| {
                //Since ref_hash is always a single item, the search will only ever return a Vec of
                //length 0 (no matches found), or 1 (match found.)
                let matches = search_struct.search(&[&ref_hash], (&tolerance).into(), true);

                matches.into_iter().map(move |entries| {
                    MatchGroup::with_reference(
                        ref_hash.src_path().to_path_buf(),
                        entries
                            .into_iter()
                            .map(|x| x.src_path().to_path_buf())
                            .collect::<Vec<_>>(),
                    )
                })
            })
            .collect()
    }
}
