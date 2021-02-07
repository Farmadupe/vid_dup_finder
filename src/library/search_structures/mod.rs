mod bk_tree;
pub use bk_tree::BkTree;

mod search_vec;
pub use search_vec::SearchVec;

mod search_struct_enum;

pub use search_struct_enum::SearchStructEnum;

use crate::library::{definitions::TOLERANCE_SCALING_FACTOR, TemporalHash, Tolerance};

#[derive(Clone, Copy, Debug)]
pub struct ScaledTolerance {
    pub spatial: u32,
    pub temporal: u32,
}

impl From<&Tolerance> for ScaledTolerance {
    fn from(tol: &Tolerance) -> Self {
        Self {
            spatial: (tol.spatial * TOLERANCE_SCALING_FACTOR) as u32,
            temporal: (tol.temporal * TOLERANCE_SCALING_FACTOR) as u32,
        }
    }
}

pub trait SimilaritySearch {
    fn seed(&mut self, new_entry: TemporalHash);

    fn search<R>(&self, values: &[R], tolerance: ScaledTolerance, consume: bool) -> Vec<Vec<TemporalHash>>
    where
        R: AsRef<TemporalHash> + Send + Sync;

    fn fetch_unmatched_items(&self, count: usize) -> Vec<&TemporalHash>;

    fn into_without_unmatched(self) -> Self;

    fn len(&self) -> usize;
}
