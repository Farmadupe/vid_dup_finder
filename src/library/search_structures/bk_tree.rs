use std::{
    collections::HashMap,
    fmt::Debug,
    sync::atomic::{AtomicBool, Ordering::Relaxed},
};

use rayon::prelude::*;

use super::ScaledTolerance;
use crate::library::{Distance, TemporalHash};

#[derive(Debug, Default)]
pub struct BkTree {
    value: Option<TemporalHash>,
    value_tainted: AtomicBool,
    children: HashMap<u32, BkTree>,
}

impl BkTree {
    pub fn new() -> Self {
        Self {
            value: None,
            value_tainted: false.into(),
            children: HashMap::default(),
        }
    }

    pub fn seed(&mut self, new_entry: TemporalHash) {
        //bktree is recursive, so we can always consider outselves to be the root node. So to insert a new element, we
        //check to see if we have a slot in our own direct children available. If we do, then we insert it. Otherwise
        // we find the right child to insert it into and tell that child to insert it.

        match &self.value {
            None => self.value = Some(new_entry),
            Some(existing_entry) => {
                let distance = existing_entry.distance(&new_entry).u32_value();

                if let Some(colliding_child) = self.children.get_mut(&distance) {
                    colliding_child.seed(new_entry);
                } else {
                    let mut new_tree = Self::new();
                    new_tree.seed(new_entry);
                    self.children.insert(distance, new_tree);
                }
            }
        }
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
            .par_iter()
            .map(|val| self.search_one(val.as_ref(), tolerance, consume))
            .filter(|vec| !vec.is_empty())
            .collect()
    }

    pub fn fetch_unmatched_items(&self, count: usize) -> Vec<&TemporalHash> {
        let mut ret = vec![];
        self.fetch_unmatched_items_inner(count, &mut ret);
        ret
    }

    pub fn into_without_unmatched(self) -> Self {
        let entries = self.into_unmatched_items();

        Self::from(entries)
    }

    pub fn len(&self) -> usize {
        if self.value.is_none() || self.value_tainted.load(Relaxed) {
            self.children.values().map(BkTree::len).sum::<usize>()
        } else {
            1 + self.children.values().map(BkTree::len).sum::<usize>()
        }
    }

    pub fn search_one(&self, value: &TemporalHash, tolerance: ScaledTolerance, consume: bool) -> Vec<TemporalHash> {
        let mut ret = vec![];
        self.search_inner(value, tolerance, consume, &mut ret);
        self.value_tainted.store(true, Relaxed);

        ret
    }

    pub fn search_inner(
        &self,
        value: &TemporalHash,
        tolerance: ScaledTolerance,
        consume: bool,
        ret: &mut Vec<TemporalHash>,
    ) {
        if self.value.is_none() {
            return;
        }

        let Distance {
            spatial: spatial_distance_from_root,
            temporal: temporal_distance_from_root,
        } = self.value.as_ref().unwrap().distance(value);

        let spatial_min_distance = spatial_distance_from_root.saturating_sub(tolerance.spatial);
        let spatial_max_distance = spatial_distance_from_root.saturating_add(tolerance.spatial);

        let temporal_min_distance = temporal_distance_from_root.saturating_sub(tolerance.temporal);
        let temporal_max_distance = temporal_distance_from_root.saturating_add(tolerance.temporal);

        if ((spatial_distance_from_root <= tolerance.spatial) && (!self.value_tainted.load(Relaxed)))
            && ((temporal_distance_from_root <= tolerance.temporal) && (!self.value_tainted.load(Relaxed)))
        {
            ret.push(self.value.clone().unwrap());
            if consume {
                self.value_tainted.store(true, Relaxed);
            }
        }

        let spatial_distance_range = spatial_min_distance..=spatial_max_distance;
        let temporal_distance_range = temporal_min_distance..=temporal_max_distance;

        //now for each candidate distance, find matching children, if any.
        let children_to_search = self
            .children
            .keys()
            .filter(|c| spatial_distance_range.contains(c as &u32) && temporal_distance_range.contains(c as &u32));

        children_to_search.for_each(|distance| {
            if let Some(child_at_distance) = self.children.get(distance) {
                child_at_distance.search_inner(value, tolerance, consume, ret);
            }
        });
    }

    pub fn fetch_unmatched_items_inner<'a>(&'a self, count: usize, ret: &mut Vec<&'a TemporalHash>) {
        if !self.children.is_empty() {
            for (_idx, child) in self.children.iter() {
                if ret.len() < count {
                    child.fetch_unmatched_items_inner(count, ret);
                } else {
                    return;
                }
            }
        }

        if ret.len() < count {
            if let Some(value) = &self.value {
                if !self.value_tainted.load(Relaxed) {
                    ret.push(value);
                }
            }
        }
    }

    pub fn into_unmatched_items(self) -> Vec<TemporalHash> {
        let mut ret = vec![];

        if let Some(value) = self.value {
            if !self.value_tainted.load(Relaxed) {
                ret.push(value)
            }
        }

        for (_distance, child) in self.children {
            ret.extend(child.into_unmatched_items());
        }

        ret
    }
}

impl<I> std::convert::From<I> for BkTree
where
    I: IntoIterator<Item = TemporalHash>,
{
    fn from(v: I) -> Self {
        let mut ret = Self::new();

        for item in v {
            ret.seed(item);
        }

        ret
    }
}
