use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use rayon::prelude::*;

use super::ScaledTolerance;
use crate::library::*;

#[derive(Debug, Default)]
struct SearchVecEntry {
    value_tainted: AtomicBool,
    value: TemporalHash,
}

impl From<TemporalHash> for SearchVecEntry {
    fn from(val: TemporalHash) -> Self {
        Self {
            value_tainted: false.into(),
            value: val,
        }
    }
}

#[derive(Debug, Default)]
pub struct SearchVec {
    entries: Vec<SearchVecEntry>,
}

//struct SearchVec<T>(Vec<T>);

impl SearchVec {
    pub fn new() -> Self {
        Self { entries: vec![] }
    }

    pub fn seed(&mut self, new_entry: TemporalHash) {
        self.entries.push(new_entry.into())
    }

    pub fn search_deterministic<R>(
        &self,
        values: &[R],
        tolerance: ScaledTolerance,
        consume: bool,
    ) -> Vec<Vec<TemporalHash>>
    where
        R: AsRef<TemporalHash>,
    {
        values
            .iter()
            .map(|val| self.search_one(val.as_ref(), tolerance, consume))
            .filter(|vec| !vec.is_empty())
            .collect()
    }

    pub fn search<R>(&self, values: &[R], tolerance: ScaledTolerance, consume: bool) -> Vec<Vec<TemporalHash>>
    where
        R: AsRef<TemporalHash> + Send + Sync,
    {
        values
            .into_par_iter()
            .map(|val| self.search_one(val.as_ref(), tolerance, consume))
            .filter(|vec| !vec.is_empty())
            .collect()
    }

    pub fn fetch_unmatched_items(&self, count: usize) -> Vec<&TemporalHash> {
        self.entries
            .iter()
            .filter(|entry| !entry.value_tainted.load(Relaxed))
            .map(|entry| &entry.value)
            .take(count)
            .collect()
    }

    pub fn into_without_unmatched(mut self) -> Self {
        for i in (0..self.entries.len()).rev() {
            if self.entries[i].value_tainted.load(Relaxed) {
                self.entries.swap_remove(i);
            }
        }

        self
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn search_one(&self, value: &TemporalHash, tolerance: ScaledTolerance, consume: bool) -> Vec<TemporalHash> {
        let mut ret = vec![];

        for entry in self.entries.iter() {
            let Distance {
                spatial: spatial_dist,
                temporal: temporal_dist,
            } = value.distance(&entry.value);
            if (spatial_dist <= tolerance.spatial && !entry.value_tainted.load(Relaxed))
                && (temporal_dist <= tolerance.temporal && !entry.value_tainted.load(Relaxed))
            {
                ret.push(entry.value.clone());
                if consume {
                    entry.value_tainted.store(true, Relaxed);
                }
            }
        }

        ret
    }
}

impl<I, R> std::convert::From<I> for SearchVec
where
    I: IntoIterator<Item = R>,
    R: AsRef<TemporalHash>,
{
    fn from(v: I) -> Self {
        let mut ret = Self::new();

        for item in v.into_iter() {
            ret.seed(item.as_ref().clone());
        }

        ret
    }
}
