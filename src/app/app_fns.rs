use std::{
    collections::{hash_map::RandomState, HashSet},
    error::Error,
    ffi::OsString,
    io::BufWriter,
    path::{Path, PathBuf},
};

use super::app_cfg::AppCfg;
use serde::Serialize;
use serde_json::json;
use vid_dup_finder_lib::*;
use video_hash_filesystem_cache::*;

use crate::app::*;

pub fn run_app() -> i32 {
    //Parse arguments and bail early if there is an error.
    let cfg = match arg_parse::parse_args() {
        Ok(cfg) => {
            configure_logs(cfg.output_cfg.verbosity);
            cfg
        }
        Err(fatal) => {
            //Errors are reported using TermLogger, which is configured from the argument parser.
            //But if a fatal error occurred during parsing the logger would not be configured when
            //we attempt to print the fatal error. So if a fatal error occurs, start the logger
            //before returning the error.
            configure_logs(ReportVerbosity::Verbose);
            print_fatal_err(&fatal, ReportVerbosity::Verbose);
            return 1;
        }
    };

    match run_app_inner(&cfg) {
        Ok(nonfatal_errs) => {
            print_nonfatal_errs(nonfatal_errs);
            0
        }
        Err(fatal_error) => {
            print_fatal_err(&fatal_error, cfg.output_cfg.verbosity);
            1
        }
    }
}

fn run_app_inner(cfg: &AppCfg) -> Result<Vec<AppError>, AppError> {
    let mut nonfatal_errs: Vec<AppError> = vec![];

    //Check that ffmpeg and ffprobe exist on the command line, and bail if not.
    //SLightly helps usability as we can bail early here with a useful error message
    //Otherwise, the program will loop over every video printing the same
    //"failed to create hash because ffmpeg is not installed"
    //warning
    if !ffmpeg_cmdline_utils::ffmpeg_and_ffprobe_are_callable() {
        return Err(AppError::FfmpegNotFound);
    }

    //shorten some long variable names
    let cand_dirs = &cfg.dir_cfg.cand_dirs;
    let ref_dirs = &cfg.dir_cfg.ref_dirs;
    let excl_dirs = &cfg.dir_cfg.excl_dirs;
    let excl_exts = &cfg.dir_cfg.excl_exts;

    // Check that there are no shared paths in refs and cands.
    for cand_path in cand_dirs {
        for ref_path in ref_dirs {
            if cand_path == ref_path {
                return Err(AppError::PathInFilesAndRefs(cand_path.to_path_buf()));
            }
        }
    }

    // If any ref_path is a child of any cand_path, add it as an excl of cand_paths. This allows ref_paths to be located
    // in subdirs of cand_paths.
    // Also do the same the other way round
    let (cand_excls, ref_excls) =
        resolve_shadowing_paths_of_cands_and_refs(cand_dirs, ref_dirs, excl_dirs);

    //load up existing hashes from disk. If no-cache-mode is specified, then set the save threshold of the cache
    //to a very high number
    let cache_save_threshold = 100;
    let cache = VideoHashFilesystemCache::new(
        cache_save_threshold,
        cfg.cache_cfg.cache_path.as_ref().unwrap().clone(),
    )?;

    // Update the cache file with all videos specified by --files and --with-refs
    if !cfg.cache_cfg.no_update_cache {
        update_hash_cache(
            cand_dirs,
            &cand_excls,
            excl_exts,
            ref_dirs,
            &ref_excls,
            &mut nonfatal_errs,
            &cache,
        )?;
    }

    //if the app was only invoked to update the cache, then we're done at this point.
    if cfg.update_cache_only {
        return Ok(nonfatal_errs);
    }

    // Now that we have updated the caches, we can fetch hashes from the cache in preparation for a search.
    // the unwraps here are infallible, as the keys we are fetching are sourced from the cache itself.
    let all_hash_paths = cache
        .all_cached_paths()
        .iter()
        .cloned()
        .collect::<HashSet<PathBuf, RandomState>>();

    let mut cand_projection = FileProjection::new(cand_dirs, cand_excls, excl_exts.clone())
        .map_err(AppError::from_cand_exclusion_error)?;
    cand_projection.project_using_list(&all_hash_paths);
    let cand_paths = cand_projection.projected_files();
    let cand_hashes = cand_paths
        .iter()
        .map(|cand_path| cache.fetch(cand_path).unwrap())
        .collect::<Vec<_>>();

    let mut ref_projection = FileProjection::new(ref_dirs, ref_excls, excl_exts.clone())
        .map_err(AppError::from_ref_exclusion_error)?;
    ref_projection.project_using_list(&all_hash_paths);
    let ref_paths = ref_projection.projected_files();
    let ref_hashes = ref_paths
        .iter()
        .map(|ref_path| cache.fetch(ref_path).unwrap())
        .collect::<Vec<_>>();

    let matchset = obtain_thunks(cfg, cand_hashes, ref_hashes);

    if cfg.output_cfg.gui {
        #[cfg(all(target_family = "unix", feature = "gui"))]
        {
            let thunks = matchset
                .into_iter()
                .map(|match_group| {
                    ResolutionThunk::from_matchgroup(
                        &match_group,
                        &cache,
                        &cfg.output_cfg.gui_trash_path,
                    )
                })
                .collect();
            run_gui(thunks)?;
        }
    } else if let Some(output_thumbs_dir) = &cfg.output_cfg.output_thumbs_dir {
        use rayon::prelude::*;

        let font =
            rusttype::Font::try_from_bytes(include_bytes!("font/NotoSans-Regular.ttf")).unwrap();

        matchset
            .par_iter()
            .enumerate()
            .for_each(|(i, match_group)| {
                let output_path = output_thumbs_dir.join(format!("{}.png", i));

                let reference = match_group.reference();
                let duplicates = match_group.duplicates();

                write_image(reference, duplicates, &output_path, &font);
            });
    } else {
        let search_output = SearchOutput::new(matchset);

        // The user may have unique hashes to be printed. Calculate that here.
        let dup_paths = search_output
            .dup_paths()
            .map(PathBuf::from)
            .collect::<HashSet<PathBuf, RandomState>>();
        let unique_paths = cand_paths
            .difference(&dup_paths)
            .map(|x| x.as_path())
            .collect::<Vec<_>>();

        print_search_results(&search_output, &unique_paths, cfg);
    }

    Ok(nonfatal_errs)
}

fn update_hash_cache(
    cand_dirs: &[PathBuf],
    cand_excls: &[PathBuf],
    excl_exts: &[OsString],
    ref_dirs: &[PathBuf],
    ref_excls: &[PathBuf],
    nonfatal_errs: &mut Vec<AppError>,
    cache: &VideoHashFilesystemCache,
) -> Result<(), AppError> {
    let mut cands = FileProjection::new(cand_dirs, cand_excls, excl_exts)
        .map_err(AppError::from_cand_exclusion_error)?;
    let mut refs = FileProjection::new(ref_dirs, ref_excls, excl_exts)
        .map_err(AppError::from_ref_exclusion_error)?;
    match cands.project_using_fs() {
        Ok(projection_errs) => nonfatal_errs.extend(
            projection_errs
                .into_iter()
                .map(|e| AppError::FileSearchError(e.path().unwrap().to_path_buf(), e)),
        ),
        Err(fatal_err) => match fatal_err {
            FileProjectionError::PathNotFound(path) => {
                return Err(AppError::CandPathNotFoundError(path))
            }
            FileProjectionError::ExclPathNotFound(path) => {
                return Err(AppError::ExclPathNotFoundError(path))
            }
            _ => unreachable!(),
        },
    };
    match refs.project_using_fs() {
        Ok(projection_errs) => nonfatal_errs.extend(
            projection_errs
                .into_iter()
                .map(|e| AppError::FileSearchError(e.path().as_ref().unwrap().to_path_buf(), e)),
        ),
        Err(fatal_err) => match fatal_err {
            FileProjectionError::PathNotFound(path) => {
                return Err(AppError::RefPathNotFoundError(path))
            }
            _ => unreachable!(),
        },
    };
    nonfatal_errs.extend(
        cache
            .update_using_fs(&cands)?
            .into_iter()
            .map(AppError::from),
    );
    nonfatal_errs.extend(
        cache
            .update_using_fs(&refs)?
            .into_iter()
            .map(AppError::from),
    );
    cache.save()?;
    Ok(())
}

//if any of the app's starting cand paths is inside the app's ref paths, then we'll add those paths to the ref paths' excl
//paths so that those paths are cands (and vice versa).
//This function returns the shadowing_cands which 'shadow' the src_paths.
fn resolve_shadowing_paths_of_cands_and_refs(
    cand_paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ref_paths: impl IntoIterator<Item = impl AsRef<Path>>,
    excl_paths: impl IntoIterator<Item = impl AsRef<Path>>,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    fn shadowing_paths(
        src_paths: impl IntoIterator<Item = impl AsRef<Path>> + Clone,
        shadowing_cands: impl IntoIterator<Item = impl AsRef<Path>> + Clone,
    ) -> Vec<PathBuf> {
        shadowing_cands
            .into_iter()
            .filter(|shadowing_cand| {
                src_paths
                    .clone()
                    .into_iter()
                    .any(|src_path| shadowing_cand.as_ref().starts_with(src_path.as_ref()))
            })
            .map(|shadowing_path| shadowing_path.as_ref().to_path_buf())
            .collect::<Vec<_>>()
    }

    let cand_paths = cand_paths
        .into_iter()
        .map(|x| x.as_ref().to_path_buf())
        .collect::<Vec<_>>();
    let ref_paths = ref_paths
        .into_iter()
        .map(|x| x.as_ref().to_path_buf())
        .collect::<Vec<_>>();
    let excl_paths = excl_paths
        .into_iter()
        .map(|x| x.as_ref().to_path_buf())
        .collect::<Vec<_>>();
    let cand_shadows = shadowing_paths(&cand_paths, &ref_paths);
    let ref_shadows = shadowing_paths(&ref_paths, &cand_paths);

    let with_excls = |shadow_paths| {
        excl_paths
            .iter()
            .map(|excl_path| excl_path.to_path_buf())
            .chain(shadow_paths)
    };

    (
        with_excls(cand_shadows).collect(),
        with_excls(ref_shadows).collect(),
    )
}

pub fn obtain_thunks(
    cfg: &AppCfg,
    cand_hashes: Vec<VideoHash>,
    ref_hashes: Vec<VideoHash>,
) -> Vec<MatchGroup> {
    //sanity check: Warn the user if no files were selected for the search
    if cand_hashes.is_empty() {
        warn!("No files were found at the paths given by --files. No results will be returned.")
    }

    //sanity check: Warn the user if no refs were selected (but only if the user asked for refs)
    if !cfg.dir_cfg.ref_dirs.is_empty() && ref_hashes.is_empty() {
        warn!("No reference files were found at the paths given by --with-refs. No results will be returned.")
    }

    //If there are just cands, then perform a find-all search. Otherwise perform a with-refs search.
    let match_set = if ref_hashes.is_empty() {
        search(cand_hashes, cfg.tolerance)
    } else {
        search_with_references(ref_hashes, cand_hashes, cfg.tolerance)
    };

    match_set
}

fn print_fatal_err(fatal_err: &AppError, verbosity: ReportVerbosity) {
    error!(target: "app-errorlog", "{}", fatal_err);

    if verbosity == ReportVerbosity::Verbose {
        let mut source: Option<&(dyn Error + 'static)> = fatal_err.source();
        while let Some(e) = source {
            error!(target: "app-errorlog", "    caused by: {}", e);
            source = e.source();
        }
    }
}

fn print_nonfatal_errs(nonfatal_errs: Vec<AppError>) {
    for err in nonfatal_errs
        .iter()
        .filter(|err| !matches!(err, AppError::CacheErrror(_)))
    {
        warn!("{}", err);
    }
}

pub fn configure_logs(verbosity: ReportVerbosity) {
    use simplelog::*;

    //let cfg = Default::default();
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("generic_cache_insert".to_string());

    let min_loglevel = match verbosity {
        ReportVerbosity::Quiet => LevelFilter::Warn,
        ReportVerbosity::Default => LevelFilter::Info,
        ReportVerbosity::Verbose => LevelFilter::Trace,
    };

    TermLogger::init(
        min_loglevel,
        cfg.build(),
        TerminalMode::Stderr,
        ColorChoice::Auto,
    )
    .expect("TermLogger failed to initialize");
}

fn print_search_results(search_output: &SearchOutput, unique_paths: &[&Path], app_cfg: &AppCfg) {
    let output_cfg = &app_cfg.output_cfg;
    if output_cfg.print_unique {
        if output_cfg.json_output {
            let stdout = BufWriter::new(std::io::stdout());
            serde_json::to_writer_pretty(stdout, &json!(unique_paths)).unwrap_or_default();
            println!();
        } else {
            unique_paths.iter().for_each(|unique_file| {
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

            let output_vec: Vec<JsonStruct> = search_output
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
            for group in search_output.dup_groups() {
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
}

fn write_image(
    reference: Option<&Path>,
    duplicates: impl IntoIterator<Item = impl AsRef<Path>>,
    output_path: &Path,
    font: &rusttype::Font,
) {
    //use imageproc::*;
    use image::GenericImage;
    use image::ImageBuffer;
    use image::RgbImage;

    info!(
        target: "write_image",
            "Writing match image to {}", output_path.display()
    );

    pub fn grid_images(images: &[(String, Vec<RgbImage>)], font: &rusttype::Font) -> RgbImage {
        let (img_x, img_y) = images.get(0).unwrap().1.get(0).unwrap().dimensions();
        let grid_num_x = images
            .iter()
            .map(|(_src_path, imgs)| imgs.len())
            .max()
            .unwrap_or(0) as u32;
        let grid_num_y = images.len() as u32;

        let txt_y = 20;

        let grid_buf_row_y = img_y + txt_y;

        let grid_buf_x = img_x * grid_num_x;
        let grid_buf_y = grid_buf_row_y * grid_num_y;

        let mut grid_buf: RgbImage = ImageBuffer::new(grid_buf_x, grid_buf_y);

        for (col_no, (src_path, row_imgs)) in images.iter().enumerate() {
            let y_coord = col_no as u32 * grid_buf_row_y;
            for (row_no, img) in row_imgs.iter().enumerate() {
                let x_coord = row_no as u32 * img_x;

                grid_buf
                    .copy_from(img as &RgbImage, x_coord, y_coord + txt_y)
                    .unwrap();
            }
            imageproc::drawing::draw_text_mut(
                &mut grid_buf,
                image::Rgb::<u8>([255, 255, 255]),
                0,
                y_coord + 3,
                rusttype::Scale { x: 15.0, y: 15.0 },
                font,
                src_path.as_str(),
            );
        }

        grid_buf
    }

    let mut all_paths = vec![];
    if let Some(reference) = reference {
        all_paths.push(reference.to_path_buf());
    }
    for dup_path in duplicates.into_iter() {
        all_paths.push(dup_path.as_ref().to_path_buf())
    }

    let all_thumbs: Vec<(String, Vec<RgbImage>)> = all_paths
        .into_iter()
        .map(|src_path| {
            (
                src_path.to_string_lossy().to_string(),
                ffmpeg_cmdline_utils::FfmpegFrameReaderBuilder::new(src_path.to_path_buf())
                    .num_frames(7)
                    .fps("1/5")
                    .spawn()
                    .ok()
                    .map(|(frames_iter, _stats)| {
                        frames_iter
                            .map(|img| {
                                image::imageops::resize(
                                    &img,
                                    200,
                                    200,
                                    image::imageops::FilterType::Triangle,
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap(),
            )
        })
        .collect::<Vec<_>>();

    let output_buf = grid_images(&all_thumbs, font);
    std::fs::create_dir_all(output_path.parent().unwrap()).unwrap();
    output_buf.save(output_path).unwrap();
}
