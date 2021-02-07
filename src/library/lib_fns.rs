use std::{
    collections::{hash_map::RandomState, HashSet},
    path::{Path, PathBuf},
};

use concrete_cachers::FetchOperationError;
use itertools::Either;
use rayon::prelude::*;
use video_hashing::matches::MatchGroup;
use Either::{Left, Right};

use crate::library::*;

pub fn load_disk_caches(cache_cfg: &CacheCfg) -> Result<DupFinderCache, LibError> {
    //There are two caches here:
    // * DCT cache (precomputed temporal hashes for each video in the frame cache)
    // * Stats cache (Video metadata, and also the size of a few png-compressed frames which is used for size comparisons)
    //
    //
    // Each cache is updated in different situations:
    // * DCT cache: Only when --refresh-caches is passed.
    // * Stats cache: lazy loaded when values are read (not done here)

    let cache_path = cache_cfg.cache_dir.join("cache.bin");

    // Load up the DCT cache.
    let cache = DupFinderCache::new(100, cache_path).map_err(LibError::from)?;

    Ok(cache)
}

pub fn update_dct_cache_from_fs(dct_cache: &DupFinderCache, search_cfg: &SearchCfg) -> Result<Vec<LibError>, LibError> {
    //If asked to update the contents of the caches from the filesystem, then do so.
    let ref_and_new_paths = search_cfg.ref_dirs.iter().chain(search_cfg.cand_dirs.iter());
    let mut all_filenames_enumerator = FileSet::new(ref_and_new_paths, &search_cfg.excl_dirs);

    let errs = match dct_cache.update_from_fs(&mut all_filenames_enumerator) {
        Ok(errs) => errs.into_iter().map(LibError::CacheError).collect(),
        Err(fatal_err) => {
            return Err(LibError::CacheError(fatal_err));
        }
    };

    Ok(errs)
}

pub fn retry_load_failures(cache: &DupFinderCache) -> Vec<LibError> {
    let reload_results = cache
        .err_video_paths()
        .into_iter()
        .map(|src_path| cache.force_reload_hash(src_path));

    //now return only those errors which are a processing error
    //(as opposed to not a video or too short etc)
    reload_results
        .filter_map(Result::err)
        .filter(FetchOperationError::is_processing_error)
        .map(LibError::from)
        .collect()
}

pub fn reload_non_videos(cache: &DupFinderCache) -> Vec<LibError> {
    let reload_results = cache
        .non_video_paths()
        .into_iter()
        .map(|src_path| cache.force_reload_hash(src_path));

    //now return only those errors which are a processing error
    //(as opposed to not a video or too short etc)
    reload_results
        .filter_map(Result::err)
        .filter(FetchOperationError::is_processing_error)
        .map(LibError::from)
        .collect()
}

pub fn find_all_matches(
    cache: &DupFinderCache,
    _cache_cfg: &CacheCfg,
    search_cfg: &SearchCfg,
) -> Result<(SearchOutput, Vec<LibError>), LibError> {
    let (new_hashes, ref_hashes, errs) = populate_new_and_ref_hashes(search_cfg, cache)?;

    let search_start_time = std::time::Instant::now();
    let matches_vec = {
        if search_cfg.ref_dirs.is_empty() {
            VideoDupFinder::new().find_all(
                new_hashes.clone(),
                search_cfg.tolerance,
                search_cfg.determ,
                search_cfg.vec_search,
            )
        } else {
            VideoDupFinder::new().find_with_refs(
                ref_hashes.clone(),
                new_hashes.clone(),
                search_cfg.tolerance,
                search_cfg.determ,
                search_cfg.vec_search,
            )
        }
    };

    let dup_files: HashSet<_, RandomState> = matches_vec.iter().flat_map(MatchGroup::duplicates).collect();
    let dup_files_len = dup_files.len();

    let new_files: HashSet<_, _> = new_hashes.iter().map(|hash| hash.src_path()).collect();

    let unique_files = new_files
        .difference(&dup_files)
        .into_iter()
        .map(|path| path.to_path_buf())
        .collect::<Vec<_>>();

    let mut match_output = SearchOutput::new(matches_vec, unique_files, !search_cfg.ref_dirs.is_empty());

    //now refine the matches as asked by the user into those whose lengths match (affirmed), or
    // those whose lengths differ (falsepos), or neither.
    if search_cfg.affirm_matches {
        match_output = match_output.affirmed(cache);
    }
    if search_cfg.cartesian {
        match_output = match_output.cartesian_product(search_cfg.tolerance, cache);
    };
    let search_time = std::time::Instant::now() - search_start_time;

    trace!(target: "application", "search took {}",
        format!("{}.{} s", search_time.as_secs(), search_time.subsec_millis()),
    );

    trace!(target: "search",
        "There were {} references, {} candidates, {} matchgroups, {} duplicates",
        ref_hashes.len(),
        new_hashes.len(),
        match_output.len(),
        dup_files_len,
    );

    Ok((match_output, errs))
}

fn get_hashes_from_cache(
    cache: &DupFinderCache,
    incl_dirs: impl IntoIterator<Item = impl AsRef<Path>>,
    excl_dirs: impl IntoIterator<Item = impl AsRef<Path>>,
) -> (Vec<TemporalHash>, Vec<LibError>) {
    let mut path_source = FileSet::new(incl_dirs, excl_dirs);

    let filenames = path_source.enumerate_from_cache(cache.inner());

    let hashes = filenames
        .into_par_iter()
        .map(|filename| cache.get_hash(filename))
        //remove too-short-erros and not-video errors
        .filter(|res| match res {
            Ok(_) => true,
            Err(e) => e.is_processing_error(),
        })
        //convert errors to liberror
        .map(|res| res.map_err(LibError::from));

    let (mut good_hashes, errors): (Vec<_>, Vec<_>) = hashes.partition_map(|res| match res {
        Ok(good_hash) => Left(good_hash),
        Err(e) => Right(e),
    });

    good_hashes.sort();

    (good_hashes, errors)
}

#[allow(clippy::type_complexity)]
fn populate_new_and_ref_hashes(
    search_cfg: &SearchCfg,
    cache: &DupFinderCache,
) -> Result<(Vec<TemporalHash>, Vec<TemporalHash>, Vec<LibError>), LibError> {
    let mut ret_errs = vec![];

    //Return an error if the same path is given as a ref and a candidate.
    let ref_set: HashSet<_, RandomState> = search_cfg.ref_dirs.iter().collect();
    let cand_set: HashSet<_, RandomState> = search_cfg.cand_dirs.iter().collect();
    let dup_paths = ref_set
        .intersection(&cand_set)
        .map(|x| x.display().to_string())
        .collect::<Vec<_>>()
        .as_slice()
        .join(", ");
    if !dup_paths.is_empty() {
        return Err(LibError::SamePathInRefAndCandError(dup_paths));
    }

    // Get reference hashes from the cache.
    let ref_hashes = if !search_cfg.ref_dirs.is_empty() {
        let mut ref_extra_excls = excl_dirs_from_deeper_paths(&search_cfg.ref_dirs, &search_cfg.cand_dirs);
        ref_extra_excls.extend(search_cfg.excl_dirs.iter().map(PathBuf::as_path));
        let (ref_hashes, errs) = get_hashes_from_cache(cache, &search_cfg.ref_dirs, ref_extra_excls);
        ret_errs.extend(errs.into_iter());
        ref_hashes
    } else {
        vec![]
    };

    //tell the user if we didn't pick any files up, as they may have made a mistake.
    if !search_cfg.ref_dirs.is_empty() && ref_hashes.is_empty() {
        warn!("No reference files were found at the paths given by --with-refs. No results will be returned.")
    }

    // Get candidate hashes from the cache (Only required for golden reference mode)
    //new files must also exclude ref dirs, as that wouldn't make sense.
    let new_hashes = {
        let mut new_extra_excls = excl_dirs_from_deeper_paths(&search_cfg.cand_dirs, &search_cfg.ref_dirs);
        new_extra_excls.extend(search_cfg.excl_dirs.iter().map(PathBuf::as_path));

        let new_hashes = {
            let (new_hashes, errs) = get_hashes_from_cache(cache, &search_cfg.cand_dirs, new_extra_excls);
            ret_errs.extend(errs);
            new_hashes
        };

        new_hashes
    };
    //tell the user if we didn't pick any files up, as they may have made a mistake.
    if new_hashes.is_empty() {
        warn!("No video files found at the paths given by --files. No results will be returned.")
    }

    trace!(target: "application",
        "total cached files: {}, Files selected for deduplication: {}{}",
        cache.cached_src_paths().len(),
        new_hashes.len(),
        if !search_cfg.ref_dirs.is_empty() {
            format!(", Files selected as references: {}", ref_hashes.len())
        } else {
            "".to_string()
        }
    );

    Ok((new_hashes, ref_hashes, ret_errs))
}

fn excl_dirs_from_deeper_paths<'a>(
    dirs: &[impl AsRef<Path>],
    maybe_deeper_paths: &'a [impl AsRef<Path>],
) -> Vec<&'a Path> {
    //if a TemporalHash is both a new and ref, then...
    maybe_deeper_paths
        .iter()
        .filter(|maybe_deeper_path| {
            dirs.iter()
                .any(|cand| crate::library::file_set::is_ancestor_of(cand.as_ref(), maybe_deeper_path.as_ref()))
        })
        .map(|x| x.as_ref())
        .collect::<Vec<_>>()
}
