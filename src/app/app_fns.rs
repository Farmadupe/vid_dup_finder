use std::io::BufWriter;
use std::{error::Error, path::Path};

use concrete_cachers::{FetchOperationError, HashStatsCreationError};
use serde::Serialize;
use serde_json::json;

use HashCreationErrorKind::VideoTooShortError;

use crate::{
    app::{self, *},
    library::{self, *},
};

pub fn run_app() {
    match obtain_thunks() {
        Ok(_nonfatal_errs) => {
            //print_nonfatal_errs(nonfatal_errs);
        }
        Err(fatal_err) => {
            error!(target: "app-errorlog", "fatal error: {}", fatal_err);

            let mut source: Option<&(dyn Error + 'static)> = fatal_err.source();
            while let Some(e) = source {
                error!(target: "app-errorlog", "    caused by: {}", e);
                source = e.source();
            }
            std::process::exit(1)
        }
    }
}

pub fn obtain_thunks() -> Result<Vec<AppError>, AppError> {
    let mut errs_ret: Vec<AppError> = vec![];

    let cfg = match app::parse_args() {
        Ok(args) => args,

        //Errors are reported using TermLogger, which is configured from the argument parser.
        //But if a fatal error occurred during parsing the logger would not be configured when
        //we attempt to print the fatal error. So if a fatal error occurs, start the logger
        //before returning the error.
        Err(fatal_err) => {
            configure_logs(false, false);
            return Err(fatal_err);
        }
    };

    configure_logs(cfg.output_cfg.quiet, cfg.output_cfg.very_quiet);

    //first load up existing hashes from disk.
    let cache = library::load_disk_caches(&cfg.cache_cfg)?;

    //There are some debug functions that do not require a search to be done. If so perform then and return.
    if cfg.debug_print_bad_hashes {
        app::print_bad_hashes(&cache);
        return Ok(vec![]);
    }

    //now update the hashes if requested
    if !cfg.cache_cfg.no_refresh_caches {
        let errs = library::update_dct_cache_from_fs(&cache, &cfg.search_cfg)?;
        errs_ret.extend(errs.into_iter().map(AppError::from));
    }

    if cfg.cache_cfg.debug_reload_errors {
        let errs = library::retry_load_failures(&cache);
        errs_ret.extend(errs.into_iter().map(AppError::from));
    }

    if cfg.cache_cfg.debug_reload_non_videos {
        let errs = library::reload_non_videos(&cache);
        errs_ret.extend(errs.into_iter().map(AppError::from))
    }

    //Now perform the search
    let matchset = {
        let (matchset, errs) = library::find_all_matches(&cache, &cfg.cache_cfg, &cfg.search_cfg)?;
        errs_ret.extend(errs.into_iter().map(AppError::from));
        matchset
    };

    //save remaining changes to caches here (possibly before running the gui), otherwise they will not get saved
    //until after the gui is closed, which could take a long time.
    save_caches_to_disk(&cache);

    app::print_outputs(&matchset, &cache, &cfg.output_cfg);

    #[cfg(feature = "gui")]
    if cfg.output_cfg.gui || cfg.debug_falsepos {
        let matchset = if cfg.debug_falsepos {
            matchset.false_positives(&cache)
        } else {
            matchset
        };

        let mut resolution_thunks = matchset.create_resolution_thunks(&cache);

        populate_distance_and_entries(&mut resolution_thunks, &cache);
        crate::app::run_gui(resolution_thunks);
        save_caches_to_disk(&cache);
    }

    Ok(errs_ret)
}

#[cfg(feature = "gui")]
pub fn populate_distance_and_entries(thunks: &mut Vec<ResolutionThunk>, cache: &DupFinderCache) {
    for thunk in thunks.iter_mut() {
        thunk.populate_distance(&cache);
        thunk.populate_entries(&cache);
    }

    // thunks.sort_by_key(|thunk| {
    //     let key = (
    //         match thunk.distance() {
    //             Some(distance) => distance,
    //             None => library::Distance::MAX_DISTANCE,
    //         },
    //         thunk.entries().len(),
    //         thunk.entries().iter().map(|e| e.as_os_str().len()).sum::<usize>(),
    //     );
    //     key
    // });

    thunks.sort_by_key(|thunk| {
        let key = std::cmp::Reverse((
            thunk.entries().len(),
            thunk.entries().iter().map(|e| e.as_os_str().len()).sum::<usize>(),
        ));
        key
    });
}

fn print_nonfatal_errs(nonfatal_errs: Vec<AppError>) {
    for err in nonfatal_errs {
        //println!("{}", err);
        warn!("{}", err);
        let mut source = err.source();
        while let Some(e) = source {
            warn!("    caused by: {}", e);
            source = e.source();
        }
    }
}

pub fn configure_logs(quiet: bool, very_quiet: bool) {
    use simplelog::*;

    let default_cfg = Default::default();

    let min_loglevel = if quiet {
        LevelFilter::Warn
    } else if very_quiet {
        LevelFilter::Error
    } else {
        LevelFilter::Info
    };

    TermLogger::init(min_loglevel, default_cfg, TerminalMode::Stderr).unwrap();
}

fn save_caches_to_disk(cache: &DupFinderCache) {
    cache.save().expect("failed to save cache to disk");
}

pub fn print_bad_hashes(cache: &DupFinderCache) {
    let mut non_videos = vec![];
    let mut too_short = vec![];
    let mut processing_errs = vec![];
    let mut cache_errs = vec![];
    let mut empty_t_paths = vec![];
    let mut empty_s_paths = vec![];

    for src_path in cache.keys().into_iter() {
        let get_result = cache.get_hash(&src_path);

        match get_result {
            Err(processing_error) => match processing_error {
                FetchOperationError::NotVideo => non_videos.push(src_path),
                FetchOperationError::ShortVideo => too_short.push(src_path),
                FetchOperationError::ProcessingError(HashStatsCreationError::Hash(VideoTooShortError(_))) => {
                    too_short.push(src_path)
                }
                FetchOperationError::ProcessingError(e) => processing_errs.push((src_path, e)),
                FetchOperationError::CacheError(e) => cache_errs.push((src_path, e)),
            },
            Ok(hash) => {
                if hash.thash_is_all_zeroes() {
                    empty_t_paths.push(src_path)
                } else if hash.shash_is_all_zeroes() {
                    empty_s_paths.push(src_path)
                }
            }
        }
    }

    // if !non_videos.is_empty() {
    //     for src_path in &non_videos {
    //         println!("Not a video: {}", src_path.display(),);
    //     }
    // }

    // if !too_short.is_empty() {
    //     for src_path in &non_videos {
    //         println!("Video is too short: {}", src_path.display(),);
    //     }
    // }

    if !cache_errs.is_empty() {
        for (src_path, err) in &cache_errs {
            println!("Cache lookup errors: path: {}, err: {}", src_path.display(), err);
        }
    }

    if !processing_errs.is_empty() {
        for (src_path, err) in &processing_errs {
            println!(
                "Hash creation/processing error: path: {}, err: {}",
                src_path.display(),
                err
            );
        }
    }

    if !empty_t_paths.is_empty() {
        for err in &empty_t_paths {
            println!("empty temporal component: {}", err.display());
        }
    }

    if !empty_s_paths.is_empty() {
        for err in &empty_s_paths {
            println!("empty spatial component: {}", err.display());
        }
    }

    debug!(target: "run_summary",
        "non videos: {}, too short: {},  cache errors: {}, processing errors: {},  empty temporal: {}, empty_spatial: {}",
        non_videos.len(),
        too_short.len(),
        cache_errs.len(),
        processing_errs.len(),
        empty_t_paths.len(),
        empty_s_paths.len()
    );
}

pub fn print_outputs(matchset: &SearchOutput, cache: &DupFinderCache, output_cfg: &OutputCfg) {
    if output_cfg.print_unique {
        let unique_files = matchset.unique_hashes().collect::<Vec<_>>();

        if output_cfg.json_output {
            let stdout = BufWriter::new(std::io::stdout());
            serde_json::to_writer_pretty(stdout, &json!(unique_files)).unwrap_or_default();
            println!();
        } else {
            unique_files.into_iter().for_each(|unique_file| {
                println!("{}", unique_file.display());
            });
        }
    }

    if output_cfg.print_duplicates {
        if output_cfg.json_output {
            #[derive(Serialize)]
            struct JsonStruct<'a> {
                reference: Option<&'a Path>,
                duplicates: Vec<&'a Path>,
            }

            let output_vec: Vec<JsonStruct> = matchset
                .dup_groups()
                .map(|group| JsonStruct {
                    reference: group.reference(),
                    duplicates: group.duplicates().collect(),
                })
                .collect();

            let stdout = BufWriter::new(std::io::stdout());
            serde_json::to_writer_pretty(stdout, &json!(output_vec)).unwrap_or_default();
            println!();
        } else {
            for group in matchset.dup_groups() {
                if let Some(video) = group.reference() {
                    println!("{}", video.display());
                }
                for video in group.duplicates() {
                    println!("{}", video.display());
                }
                println!();
            }
        }
    }

    if output_cfg.print_worst_entries {
        let worst_entries = matchset.dups_with_lowest_pngsize(cache);

        if output_cfg.json_output {
            let stdout = BufWriter::new(std::io::stdout());
            serde_json::to_writer_pretty(stdout, &json!(worst_entries)).unwrap_or_default();
            println!();
        } else {
            for entry in worst_entries {
                println!("{}", entry.display());
            }
        }
    }
}
