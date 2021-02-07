use super::{BkTree, ScaledTolerance, SearchVec, SimilaritySearch};
use crate::library::TemporalHash;

pub enum SearchStructEnum {
    Bk(BkTree),
    BkDeterministic(BkTree),
    SearchVec(SearchVec),
    SearchVecDeterministic(SearchVec),
}
use SearchStructEnum::*;

impl SearchStructEnum {
    pub fn new(vec: bool, determnistic: bool) -> Self {
        match (vec, determnistic) {
            (false, false) => Bk(BkTree::new()),
            (false, true) => BkDeterministic(BkTree::new()),
            (true, false) => SearchVec(SearchVec::new()),
            (true, true) => SearchVecDeterministic(SearchVec::new()),
        }
    }
}

impl SimilaritySearch for SearchStructEnum {
    fn seed(&mut self, new_entry: TemporalHash) {
        match self {
            Bk(ss) => ss.seed(new_entry),
            BkDeterministic(ss) => ss.seed(new_entry),
            SearchVec(ss) => ss.seed(new_entry),
            SearchVecDeterministic(ss) => ss.seed(new_entry),
        }
    }

    fn search<R>(&self, values: &[R], tolerance: ScaledTolerance, consume: bool) -> Vec<Vec<TemporalHash>>
    where
        R: AsRef<TemporalHash> + Send + Sync,
    {
        match self {
            Bk(ss) => ss.search(values, tolerance, consume),
            BkDeterministic(ss) => ss.search_deterministic(values, tolerance, consume),
            SearchVec(ss) => ss.search(values, tolerance, consume),
            SearchVecDeterministic(ss) => ss.search_deterministic(values, tolerance, consume),
        }
    }

    fn fetch_unmatched_items(&self, count: usize) -> Vec<&TemporalHash> {
        match self {
            Bk(ss) => ss.fetch_unmatched_items(count),
            BkDeterministic(ss) => ss.fetch_unmatched_items(count),
            SearchVec(ss) => ss.fetch_unmatched_items(count),
            SearchVecDeterministic(ss) => ss.fetch_unmatched_items(count),
        }
    }

    fn into_without_unmatched(self) -> Self {
        match self {
            Bk(ss) => Bk(ss.into_without_unmatched()),
            BkDeterministic(ss) => BkDeterministic(ss.into_without_unmatched()),
            SearchVec(ss) => SearchVec(ss.into_without_unmatched()),
            SearchVecDeterministic(ss) => SearchVecDeterministic(ss.into_without_unmatched()),
        }
    }

    fn len(&self) -> usize {
        match self {
            Bk(ss) => ss.len(),
            BkDeterministic(ss) => ss.len(),
            SearchVec(ss) => ss.len(),
            SearchVecDeterministic(ss) => ss.len(),
        }
    }
}
