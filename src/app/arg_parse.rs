use std::{
    collections::{hash_map::RandomState, HashSet},
    iter::once,
    path::PathBuf,
};

use crate::{
    app::*,
    library::{CacheCfg, SearchCfg, DEFAULT_TOLERANCE},
};

pub fn parse_args() -> Result<AppCfg, crate::app::AppError> {
    let file_paths = "New file paths";
    let ref_paths = "Reference file paths";
    let excl_file_paths = "Exclude file paths";
    let cache_dir_path = "Cache dir path";
    let affirm_by_length = "Confirm matches with video length";
    let gui = "Run gui for deleting duplicates";
    let print_unique = "Print unique items";
    let print_dup = "Print duplicate items";
    let print_worst_entries = "Print worst entries";
    let json_output = "Json output";
    let debug_print_bad_hashes = "Debug print bad hashes";
    let debug_print_none = "Debug print none";
    let debug_falsepos = "Debug false positives";
    let debug_reload_cache_errors = "Debug Reload cache errors";
    let debug_reload_non_videos = "Debug Reload non vdieos";
    let args_file = "Args file";
    let tolerance_spatial = "Comparison tolerance (spatial)";
    let tolerance_temporal = "Comparison tolerance (temporal)";
    let vec_search = "Vec search";
    let deterministic_search = "Deterministic search";
    let no_update_caches = "No update caches";
    let cartesian = "Cartesian";
    let quiet = "Quiet";
    let very_quiet = "Very quiet";
    let generate_bash_completions = "Generate bash completions";

    let default_tolerance_spatial_string = format!("{}", DEFAULT_TOLERANCE.spatial);
    let default_tolerance_temporal_string = format!("{}", DEFAULT_TOLERANCE.temporal);

    let conflicting_args: Vec<&str> = vec![
        gui,
        print_unique,
        print_dup,
        print_worst_entries,
        debug_print_bad_hashes,
        debug_print_none,
    ]
    .into_iter()
    .collect();
    let conflicting_args: HashSet<&str, RandomState> = conflicting_args.into_iter().collect();

    //args are not added through method chaining because this appears to break rustfmt.
    let mut clap_app = clap::App::new("Video duplicate finder")
        .version("0.1")
        .author("me")
        .about("Detects duplicate video files");

    clap_app = clap_app.arg(
            clap::Arg::with_name(file_paths)
                .long("files")
                .required_unless(args_file)
                .required_unless(generate_bash_completions)
                .multiple(true)
                .min_values(1)
                .takes_value(true)
                .help("Paths containing new video files. These files will be checked for uniqueness against each other, or if --refs is specified, then against the files given in that argument.")
                .display_order(1)
        );

    clap_app = clap_app.arg(
        clap::Arg::with_name(ref_paths)
            .long("with-refs")
            .multiple(true)
            .min_values(1)
            .takes_value(true)
            .help("Paths containing reference video files.")
            .display_order(2),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(excl_file_paths)
            .short("x")
            .long("excl")
            .multiple(true)
            .min_values(1)
            .takes_value(true)
            .help("Paths to be ignored")
            .display_order(3),
    );

    #[cfg(target_os = "linux")]
    let default_dir = "/tmp/video_dup_finder_cache";

    #[cfg(target_os = "windows")]
    let default_dir = {
        todo!();
        //note the value  below doesn't work, as %temp% does not get expanded, thus
        //it points to a literal "%Temp%" directory.
        #[allow(unreachable_code)]
        "%Temp%/video_dup_finder_cache"
    };

    clap_app = clap_app.arg(
        clap::Arg::with_name(cache_dir_path)
            .short("c")
            .long("cache-dir")
            .default_value(default_dir)
            .help("Caches will be stored in this directory to speed up repeated runs."),
    );

    clap_app = clap_app.arg(
            clap::Arg::with_name(affirm_by_length)
                .long("affirm-length")
                .help("Confirm matches using the length of matched video files (somewhat helpful for separating out truncations, and long idents at the beginning)")
        );

    #[cfg(feature = "gui")]
    let gui_conflicts: Vec<&str> = conflicting_args.difference(&once(gui).collect()).cloned().collect();

    #[cfg(feature = "gui")]
    let mut clap_app = clap_app.arg(
        clap::Arg::with_name(gui)
            .long("gui")
            .help("Start a GUI that aids in deleting duplicate videos.")
            .conflicts_with_all(&gui_conflicts),
    );

    let print_unique_conflicts: Vec<&str> = conflicting_args
        .difference(&once(print_unique).collect())
        .cloned()
        .collect();
    clap_app = clap_app.arg(
        clap::Arg::with_name(print_unique)
            .long("print-unique")
            .help("Print unique videos (those for which no duplicate was found)")
            .conflicts_with_all(&print_unique_conflicts),
    );

    let print_dup_conflicts: Vec<&str> = conflicting_args
        .difference(&once(print_dup).collect())
        .cloned()
        .collect();
    clap_app = clap_app.arg(
        clap::Arg::with_name(print_dup)
            .long("print-dup")
            .help("Print all duplicate videos")
            .conflicts_with_all(&print_dup_conflicts),
    );

    let print_worst_entries_conflicts: Vec<&str> = conflicting_args
        .difference(&once(print_worst_entries).collect())
        .cloned()
        .collect();
    clap_app = clap_app.arg(
        clap::Arg::with_name(print_worst_entries)
            .long("print-worst-entries")
            .help("For each set of matching videos, prints the videos with the lowest 'quailty'")
            .long_help(concat!(
                "The quality metric is subjective and very rough. It is calculated as the sum of the sizes ",
                "of 10 frames in the video, equally spaced across the first minute, once they have been ",
                "compressed into PNG images. Be careful if using this to blindly delete videos."
            ))
            .conflicts_with_all(&print_worst_entries_conflicts),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(json_output)
            .long("json-output")
            .help("Print outputs in json format"),
    );

    let debug_print_bad_hashes_conflicts: Vec<&str> = conflicting_args
        .difference(
            &once(debug_print_bad_hashes)
                .chain(once(debug_reload_cache_errors))
                .collect(),
        )
        .cloned()
        .collect();
    clap_app = clap_app.arg(
        clap::Arg::with_name(debug_print_bad_hashes)
            .long("debug-print-bad-hashes")
            .hidden(true)
            .conflicts_with_all(&debug_print_bad_hashes_conflicts),
    );

    let debug_print_none_conflicts: Vec<&str> = conflicting_args
        .difference(&once(debug_print_none).chain(once(debug_reload_cache_errors)).collect())
        .cloned()
        .collect();
    clap_app = clap_app.arg(
        clap::Arg::with_name(debug_print_none)
            .long("debug-print-none")
            .hidden(true)
            .conflicts_with_all(&debug_print_none_conflicts),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(debug_falsepos)
            .long("debug-falsepos")
            .hidden(true)
            .conflicts_with(&affirm_by_length),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(debug_reload_cache_errors)
            .long("debug-reload-cache-errors")
            .hidden(true)
            .conflicts_with(no_update_caches)
            .help("For any video in the cache where an error occurred during processing, reload the video"),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(debug_reload_non_videos)
            .long("debug-reload-non-videos")
            .hidden(true)
            .conflicts_with(no_update_caches)
            .help("For any entry in the cache which is recorded as a non-video, reload the entry."),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(args_file)
            .long("args-file")
            .takes_value(true)
            .help("Read command line arguments from a file")
            .conflicts_with_all(&[file_paths, ref_paths]),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(tolerance_spatial)
            .long("tolerance-spatial")
            .default_value(&default_tolerance_spatial_string),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(tolerance_temporal)
            .long("tolerance-temporal")
            .default_value(&default_tolerance_temporal_string),
    );

    clap_app = clap_app.arg(clap::Arg::with_name(vec_search).long("vec-search"));

    clap_app = clap_app.arg(clap::Arg::with_name(deterministic_search).long("deterministic"));

    //clap_app = clap_app.arg(clap::Arg::with_name(framerate).long(framerate).default_value("1/3"));

    clap_app = clap_app.arg(
        clap::Arg::with_name(no_update_caches)
            .long("no-update-caches")
            .help("Do not update caches from filesystem. Use only those hashes in the given caches."),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(cartesian)
            .long("cartesian")
            .help("If a group of matches has more than two entries, split the group into many groups of length-two"),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(quiet)
            .long("quiet")
            .help("Quiet verbosity: Only print errors, warnings and output")
            .conflicts_with(very_quiet),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(very_quiet)
            .long("quiet-quiet")
            .help("Very quiet verbosity: Only print errors and output"),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(generate_bash_completions)
            .long("generate-bash-completions")
            .help("Generate bash completions"),
    );

    let mut clap_app3 = clap_app.clone();

    let clap_app2 = clap_app.clone();
    let mut matches = clap_app2.get_matches();

    //first check if a Args file is present. If so, then read it and use the arguments from within.
    if let Some(args_fname) = matches.value_of_os(args_file) {
        let args = match std::fs::read_to_string(args_fname) {
            Ok(args) => args,
            Err(e) => {
                return Err(crate::app::AppError::ArgsFileNotFoundError(
                    PathBuf::from(args_fname),
                    e,
                ));
            }
        };

        //now strip comments from the args file
        let args_file_contents = match comment::shell::strip(args) {
            Ok(args_file_contents) => args_file_contents,
            Err(e) => {
                return Err(crate::app::AppError::ArgsFileParseError(
                    PathBuf::from(args_file),
                    e.to_string(),
                ))
            }
        };

        //the arguments file needs to be split into args in the same way as the shell would do it.
        //call out to an external create for this.
        let args = match shell_words::split(&args_file_contents) {
            Ok(args) => args,
            Err(e) => {
                return Err(crate::app::AppError::ArgsFileParseError(
                    PathBuf::from(args_file),
                    e.to_string(),
                ))
            }
        };

        //clap works directly on OsString values, but the arguments file is read as String.
        //So convert to OsString here. Would be nicer to directly read into OsString instead.
        let args = args.into_iter();

        //slight bodge: need to prepend program name to as this is what would be seen in a direct shell invocation.
        let program_name = "dup_finder".to_string();
        let args = once(program_name).chain(args);

        matches = clap_app.get_matches_from(args);
    }

    if matches.is_present(generate_bash_completions) {
        clap_app3.gen_completions("dup_finder", clap::Shell::Zsh, ".");
        std::process::exit(0);
    }

    let file_paths = matches
        .values_of_os(file_paths)
        .unwrap_or_else(|| unreachable!())
        .map(PathBuf::from)
        .collect();

    let ref_file_paths = match matches.values_of_os(ref_paths) {
        Some(ref_file_dirs) => ref_file_dirs.map(PathBuf::from).collect(),
        None => vec![],
    };

    let exclude_file_paths = match matches.values_of_os(excl_file_paths) {
        Some(exclude_file_paths) => exclude_file_paths.map(PathBuf::from).collect(),
        None => vec![],
    };

    let tolerance = {
        let spatial = match matches.value_of(tolerance_spatial) {
            Some(spatial) => match spatial.parse() {
                Ok(spatial) => spatial,
                Err(_e) => return Err(AppError::ParseSpatialToleranceError(spatial.to_string())),
            },
            None => DEFAULT_TOLERANCE.spatial,
        };

        let temporal = match matches.value_of(tolerance_temporal) {
            Some(temporal) => match temporal.parse() {
                Ok(temporal) => temporal,
                Err(_e) => return Err(AppError::ParseSpatialToleranceError(temporal.to_string())),
            },
            None => DEFAULT_TOLERANCE.spatial,
        };

        crate::library::Tolerance { spatial, temporal }
    };

    let cache_cfg = CacheCfg {
        cache_dir: PathBuf::from(matches.value_of_os(cache_dir_path).unwrap_or_else(|| unreachable!())),
        no_refresh_caches: matches.is_present(no_update_caches),
        debug_reload_errors: matches.is_present(debug_reload_cache_errors),
        debug_reload_non_videos: matches.is_present(debug_reload_non_videos),
    };

    let search_cfg = SearchCfg {
        cand_dirs: file_paths,
        ref_dirs: ref_file_paths,
        excl_dirs: exclude_file_paths,
        vec_search: matches.is_present(vec_search),
        determ: matches.is_present(deterministic_search),
        affirm_matches: matches.is_present(affirm_by_length),
        tolerance,
        cartesian: matches.is_present(cartesian),
    };

    let output_cfg = OutputCfg {
        print_unique: matches.is_present(print_unique),
        print_duplicates: (matches.is_present(print_dup)
            || conflicting_args.iter().all(|arg| !matches.is_present(arg)))
            && !matches.is_present(debug_print_none),
        print_worst_entries: matches.is_present(print_worst_entries),
        json_output: matches.is_present(json_output),

        quiet: matches.is_present(quiet),
        very_quiet: matches.is_present(very_quiet),

        gui: matches.is_present(gui),
    };

    let ret = AppCfg {
        cache_cfg,
        search_cfg,
        output_cfg,

        debug_falsepos: matches.is_present(debug_falsepos),
        debug_print_bad_hashes: matches.is_present(debug_print_bad_hashes),
    };

    Ok(ret)
}
