use std::{
    convert::From,
    path::{Path, PathBuf},
    vec::Vec,
};

use itertools::Itertools;

use crate::library::{ffmpeg_ops::FfmpegErrorKind, *};

#[derive(Debug, Default, PartialEq, Eq, Clone, Hash)]
pub struct MatchGroup {
    reference: Option<PathBuf>,
    duplicates: Vec<PathBuf>,
}

#[derive(Debug)]
pub enum MatchGroupErrorKind {
    Ffmpeg(FfmpegErrorKind),
    Image(image::ImageError),
}

impl From<FfmpegErrorKind> for MatchGroupErrorKind {
    fn from(err: FfmpegErrorKind) -> Self {
        Self::Ffmpeg(err)
    }
}

impl From<image::ImageError> for MatchGroupErrorKind {
    fn from(err: image::ImageError) -> Self {
        Self::Image(err)
    }
}

impl MatchGroup {
    pub fn new(entries: Vec<PathBuf>) -> Self {
        Self {
            reference: None,
            duplicates: entries,
        }
    }

    pub fn with_reference(reference: PathBuf, entries: Vec<PathBuf>) -> Self {
        let mut ret = Self {
            reference: Some(reference),
            duplicates: entries,
        };

        ret.duplicates.sort_by_key(|e| e.as_os_str().len());
        ret
    }

    pub fn len(&self) -> usize {
        self.duplicates.len()
            + match self.reference {
                Some(_) => 1,
                None => 0,
            }
    }

    pub fn reference(&self) -> Option<&Path> {
        match self.reference {
            Some(ref reference) => Some(&reference),
            None => None,
        }
    }

    pub fn duplicates(&self) -> impl Iterator<Item = &Path> {
        self.duplicates.iter().map(&PathBuf::as_path)
    }

    pub fn affirmed(&self, cache: &DupFinderCache) -> Vec<Self> {
        if self.reference.is_some() {
            match self.affirmed_reference(cache) {
                Some(group) => vec![group],
                None => vec![],
            }
        } else {
            self.affirmed_noreference(cache)
        }
    }

    fn affirmed_reference(&self, cache: &DupFinderCache) -> Option<Self> {
        let ref_stats = cache.get_stats(self.reference.as_ref().unwrap()).unwrap();

        let mut affirmed_entries = self
            .duplicates
            .iter()
            .cloned()
            .filter(|entry| {
                let entry_stats = cache.get_stats(entry).unwrap();
                ref_stats.is_match(&entry_stats)
            })
            .collect::<Vec<_>>();

        affirmed_entries.sort_by_key(|e| e.as_os_str().len());

        if affirmed_entries.is_empty() {
            None
        } else {
            Some(MatchGroup {
                reference: self.reference.clone(),
                duplicates: affirmed_entries,
            })
        }
    }

    fn affirmed_noreference(&self, cache: &DupFinderCache) -> Vec<Self> {
        //helper function for affirmed_reference.
        fn insert(group: &mut MatchGroup, item: PathBuf) {
            group.duplicates.push(item);
            group.duplicates.sort_by_key(|e| e.as_os_str().len());
        }

        let mut ret: Vec<Self> = vec![];

        for cand_entry in self.duplicates() {
            let cand_stats = cache.get_stats(&cand_entry).unwrap();

            for mut affirmed_group in ret.iter_mut() {
                let mut matched = false;
                if let Some(ref affirmed_entry) = affirmed_group.duplicates().next() {
                    if cand_stats.is_match(&cache.get_stats(affirmed_entry).unwrap()) {
                        matched = true;
                    }
                }

                if matched {
                    insert(&mut affirmed_group, cand_entry.to_path_buf());
                    break;
                }
            }

            ret.push(std::iter::once(cand_entry.to_path_buf()).into());
        }

        //remove single-length groups
        let ret = ret.into_iter().filter(|group| group.len() > 1).collect();

        ret
    }

    #[cfg(feature = "gui")]
    pub fn create_resolution_thunk(&self, cache: &DupFinderCache) -> ResolutionThunk {
        let mut thunk = ResolutionThunk::new();

        //first add the reference, if it exists...
        if let Some(ref reference) = self.reference {
            let ref_stats = cache.get_stats(reference).unwrap();
            thunk.insert_reference(reference.clone(), ref_stats);
        }

        for entry in self.duplicates.iter() {
            thunk.insert_entry(entry.to_path_buf(), cache.get_stats(entry).unwrap());
        }

        thunk
    }

    pub fn dups_with_lowest_pngsize(&self, cache: &DupFinderCache) -> Vec<PathBuf> {
        //if there is a reference, then the pngsize statistic doesn't really mean very much. So for now, return nothing.
        match &self.reference {
            Some(_ref_entry) => vec![],
            None => {
                let largest_pngsize = self
                    .duplicates
                    .iter()
                    .map(|entry| (entry, cache.get_stats(entry).unwrap()))
                    .max_by_key(|(_entry, stats)| stats.png_size);

                //so return all entries that are not the best entry.
                match largest_pngsize {
                    None => vec![],
                    Some((best_entry, _best_stats)) => self
                        .duplicates
                        .iter()
                        .filter(|&entry| entry != best_entry)
                        .cloned()
                        .collect(),
                }
            }
        }
    }

    pub fn cartesian_product(&self, tolerance: Tolerance, cache: &DupFinderCache) -> Vec<Self> {
        match self.reference {
            Some(ref reference) => self
                .duplicates
                .iter()
                .map(|entry| {
                    let new_entries = [reference.to_path_buf(), entry.to_path_buf()];
                    MatchGroup::from(&new_entries)
                })
                .collect(),

            None => self
                .duplicates
                .iter()
                .combinations(2)
                .filter(|vec| {
                    let a_hash = cache.get_hash(&vec[0]).unwrap();
                    let b_hash = cache.get_hash(&vec[1]).unwrap();

                    let distance = a_hash.distance(&b_hash);
                    distance.within_tolerance((&tolerance).into())
                })
                .map(MatchGroup::from)
                .collect(),
        }
    }
}

impl<I, P> From<I> for MatchGroup
where
    I: IntoIterator<Item = P>,
    P: std::borrow::Borrow<PathBuf>,
{
    fn from(it: I) -> MatchGroup {
        let entries = it.into_iter().map(|x| x.borrow().clone()).collect();
        MatchGroup::new(entries)
    }
}
