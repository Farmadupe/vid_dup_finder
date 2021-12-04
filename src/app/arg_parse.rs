use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use vid_dup_finder_lib::*;

use crate::app::*;
use AppError::*;

// file specification
const FILE_PATHS: &str = "Directories/files to search";
const REF_PATHS: &str = "Reference file paths";
const EXCL_FILE_PATHS: &str = "Exclude file paths";
const EXCL_EXTS: &str = "Exclude file extensions";

//cache update settings
const CACHE_FILE: &str = "Cache file path";
const UPDATE_CACHE_ONLY: &str = "Update cache only. Do not perform any search";
const NO_UPDATE_CACHE: &str = "Do not update the cache. Search using alreaady-cached data";

//output settings
const JSON_OUTPUT: &str = "Json output";
const OUTPUT_THUMBS_DIR: &str = "Output thumbnails to the given directory";

//gui settings
const GUI: &str = "Run gui for deconsting duplicates";
const GUI_TRASH_PATH: &str = "Gui trash path";

//search configuration
const TOLERANCE: &str = "Comparison tolerance";
const PRINT_UNIQUE: &str = "Print unique items (default is to print duplicate items)";

const ARGS_FILE: &str = "Args file";

const VERBOSITY_QUIET: &str = "Quiet";
const VERBOSITY_VERBOSE: &str = "Verbose";

//Lifetimes in clap::App appear to be intended for various dynamically allocated help message strings.
//Since there should only ever be a single clap app in any execution of the program we avoid reasoning
//about lifetimes by leaking all dynamically allocated strings created while building the parser.
fn build_app() -> clap::App<'static, 'static> {
    let display_ordering = vec![
        //
        // file specification
        FILE_PATHS,
        REF_PATHS,
        EXCL_FILE_PATHS,
        EXCL_EXTS,
        //
        //search modifiers
        TOLERANCE,
        //
        //caching
        CACHE_FILE,
        UPDATE_CACHE_ONLY,
        NO_UPDATE_CACHE,
        //
        //outputs
        PRINT_UNIQUE,
        JSON_OUTPUT,
        OUTPUT_THUMBS_DIR,
        VERBOSITY_QUIET,
        VERBOSITY_VERBOSE,
        //
        //gui
        GUI,
        GUI_TRASH_PATH,
        //argument replacement
        ARGS_FILE,
    ];

    let get_ordering = |arg_name: &str| -> usize {
        //seems to be a bug where orderings are reversed, and higher numbers appear first.
        let last_idx = 9999;
        match display_ordering.iter().position(|x| *x == arg_name) {
            Some(idx) => idx,
            None => {
                eprintln!("argument not assigned a display order: {:?}", arg_name);
                last_idx
            }
        }
    };

    //clap requires all default values to be &'_ str. I want to provide compile-time &'static str for the below values,
    //but I couldn't find a way to turn f64 into &'static str at compile time. So the next best thing to do is to build
    //the strings at runtime.
    //Note: This is not a memory leak -- these strings need to last for the lifetime of the program.
    let tol = NormalizedTolerance::default();
    let default_tol_string: &'static str = Box::leak(format!("{}", tol.value()).into_boxed_str());

    let default_cache_file = directories_next::ProjectDirs::from("", "vid_dup_finder", "vid_dup_finder")
        .unwrap()
        .cache_dir()
        .join("vid_dup_finder_cache.bin")
        .to_str()
        .unwrap()
        .to_owned();
    let default_cache_file: &'static str = Box::leak(default_cache_file.into_boxed_str());

    //args are not added through method chaining because rustfmt struggles with very long expressions.
    let mut clap_app = clap::App::new("Video duplicate finder")
        .version("0.1")
        .about("Detect duplicate video files")
        .setting(clap::AppSettings::UnifiedHelpMessage)
        .template(include_str!("arg_parse_template.txt"));

    clap_app = clap_app.arg(
        clap::Arg::with_name(FILE_PATHS)
            .long("files")
            .required_unless(ARGS_FILE)
            .multiple(true)
            .min_values(1)
            .takes_value(true)
            .help("Paths containing new video files. These files will be checked for uniqueness against each other, or if --refs is specified, then against the files given in that argument.")
            .display_order(get_ordering(FILE_PATHS)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(REF_PATHS)
            .long("with-refs")
            .multiple(true)
            .min_values(1)
            .takes_value(true)
            .help("Paths containing reference video files. When present the files given by --files will be searched for duplicates against these files")
            .display_order(get_ordering(REF_PATHS)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(EXCL_FILE_PATHS)
            .long("exclude")
            .multiple(true)
            .min_values(1)
            .takes_value(true)
            .help("Paths to be excluded from searches")
            .display_order(get_ordering(EXCL_FILE_PATHS)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(EXCL_EXTS)
            .long("exclude-exts")
            .multiple(true)
            .min_values(1)
            .takes_value(true)
            .help("File extensions to be excluded from searches. When specified the default file exclusion extensions will be replaced with the given values. Extensions must be comma separated with no spaces, e.g '--exclude-exts ext1,ext2,ext3'")
            .require_delimiter(true)
            .default_value("png,jpg,bmp,jpeg,txt,text,db")
            .display_order(get_ordering(EXCL_EXTS)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(CACHE_FILE)
            .long("cache-file")
            .default_value(default_cache_file)
            .help("An optional custom location for the cache file (used to speed up repeated runs)")
            .display_order(get_ordering(CACHE_FILE)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(UPDATE_CACHE_ONLY)
            .long("update-cache-only")
            .help("Do not run a search. Update the cache and then exit.")
            .conflicts_with(GUI)
            .conflicts_with(NO_UPDATE_CACHE)
            .display_order(get_ordering(UPDATE_CACHE_ONLY)),
    );

    #[cfg(all(target_family = "unix", feature = "gui"))]
    let clap_app = clap_app.arg(
        clap::Arg::with_name(GUI)
            .long("gui")
            .help("Start a GUI that aids in deleting duplicate videos.")
            .display_order(get_ordering(GUI)),
    );

    #[cfg(all(target_family = "unix", feature = "gui"))]
    let mut clap_app = clap_app.arg(
        clap::Arg::with_name(GUI_TRASH_PATH)
            .long("gui-trash-path")
            .hidden(true)
            .help(
                "For use in the gui: Directory that duplicate files will be moved to when using the \"keep\" operation",
            )
            .display_order(get_ordering(GUI_TRASH_PATH)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(PRINT_UNIQUE)
            .long("search-unique")
            .help("search for unique videos (those for which no duplicate was found)")
            .display_order(get_ordering(PRINT_UNIQUE)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(JSON_OUTPUT)
            .long("json-output")
            .help("Print outputs in JSON format")
            .display_order(get_ordering(JSON_OUTPUT)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(OUTPUT_THUMBS_DIR)
            .long("match-thumbnails-dir")
            .takes_value(true)
            .help("Write thumbnails of matched images to the given directory")
            .display_order(get_ordering(OUTPUT_THUMBS_DIR)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(TOLERANCE)
            .long("tolerance")
            .help("Search tolerance. A number between 0.0 and 1.0. Low values mean videos must be very similar before they will match, high numbers will permit more differences. Suggested values are in the range 0.0 to 0.2")
            .default_value(default_tol_string)
            .display_order(get_ordering(TOLERANCE)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(NO_UPDATE_CACHE)
            .long("no-update-cache")
            .help("Do not update caches from filesystem. Search using only hashes already cached from previous runs.")
            .display_order(get_ordering(NO_UPDATE_CACHE)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(ARGS_FILE)
            .long("args-file")
            .takes_value(true)
            .help("Read command line arguments from a file. If this argument is used it must be the only argument")
            .conflicts_with_all(&[FILE_PATHS, REF_PATHS])
            .display_order(get_ordering(ARGS_FILE)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(VERBOSITY_QUIET)
            .long("quiet")
            .help("Reduced verbosity")
            .conflicts_with(VERBOSITY_VERBOSE)
            .display_order(get_ordering(VERBOSITY_QUIET)),
    );

    clap_app = clap_app.arg(
        clap::Arg::with_name(VERBOSITY_VERBOSE)
            .long("verbose")
            .help("Increased verbosity")
            .conflicts_with(VERBOSITY_QUIET)
            .display_order(get_ordering(VERBOSITY_VERBOSE)),
    );

    clap_app
}

pub(crate) fn parse_args() -> Result<AppCfg, AppError> {
    //capture the cwd once, to minimize the risk of working with two values if it is changed by the OS at runtime.
    let cwd = std::env::current_dir().expect("failed to extract cwd");

    //Start by parsing the provided arguments from the commandline. If the --args-file
    //argument is provided, then we will ignore the true command line arguments and
    //take the arguments from the file instead.
    let args = get_args_from_cmdline_or_file()?;

    let file_paths = match args.values_of_os(FILE_PATHS) {
        Some(paths) => paths.into_iter().map(|p| absolutify_path(&cwd, p.as_ref())).collect(),
        None => vec![],
    };

    let ref_file_paths = match args.values_of_os(REF_PATHS) {
        Some(ref_file_dirs) => ref_file_dirs.map(|p| absolutify_path(&cwd, p.as_ref())).collect(),
        None => vec![],
    };

    let exclude_file_paths = match args.values_of_os(EXCL_FILE_PATHS) {
        Some(exclude_file_paths) => exclude_file_paths.map(|p| absolutify_path(&cwd, p.as_ref())).collect(),
        None => vec![],
    };

    let excl_exts = args.values_of_os(EXCL_EXTS).unwrap().map(&OsStr::to_owned).collect();

    let output_thumbs_dir = args
        .value_of_os(OUTPUT_THUMBS_DIR)
        .map(|p| absolutify_path(&cwd, p.as_ref()));

    let tolerance = match args.value_of(TOLERANCE) {
        Some(value) => match value.parse() {
            Ok(value) => NormalizedTolerance::new(value),
            Err(_e) => return Err(ParseTolerance(value.to_string())),
        },
        None => NormalizedTolerance::default(),
    };

    let cache_cfg = CacheCfg {
        cache_path: args.value_of_os(CACHE_FILE).map(PathBuf::from),
        no_update_cache: args.is_present(NO_UPDATE_CACHE),
    };

    let dir_cfg = DirCfg {
        cand_dirs: file_paths,
        ref_dirs: ref_file_paths,
        excl_dirs: exclude_file_paths,
        excl_exts,
    };

    let verbosity = if args.is_present(VERBOSITY_QUIET) {
        ReportVerbosity::Quiet
    } else if args.is_present(VERBOSITY_VERBOSE) {
        ReportVerbosity::Verbose
    } else {
        ReportVerbosity::Default
    };

    let output_cfg = OutputCfg {
        print_unique: args.is_present(PRINT_UNIQUE),
        print_duplicates: !args.is_present(PRINT_UNIQUE),
        json_output: args.is_present(JSON_OUTPUT),
        output_thumbs_dir,

        verbosity,
        gui: args.is_present(GUI),
        gui_trash_path: args.value_of_os(GUI_TRASH_PATH).map(PathBuf::from),
    };

    let ret = AppCfg {
        cache_cfg,
        output_cfg,
        dir_cfg,

        update_cache_only: args.is_present(UPDATE_CACHE_ONLY),
        tolerance,
    };

    Ok(ret)
}

// Arguments are always first read from the command line, but if --args-file
// is present, then arguments are actually located in a file on disk.
// This fn obtains the args from the correct location.
fn get_args_from_cmdline_or_file() -> Result<clap::ArgMatches<'static>, AppError> {
    //first get the cmdline args. If --args-file is not present then return the cmdline args
    let cmdline_args = build_app().get_matches();
    if !cmdline_args.is_present(ARGS_FILE) {
        Ok(cmdline_args)

    //Otherwise get the args file from disk...
    } else {
        let argsfile_path = cmdline_args.value_of_os(ARGS_FILE).unwrap();

        let argsfile_text =
            std::fs::read_to_string(argsfile_path).map_err(|e| ArgsFileNotFound(PathBuf::from(argsfile_path), e))?;

        let argsfile_args = parse_argsfile_args(&argsfile_text)?;

        Ok(argsfile_args)
    }
}

fn parse_argsfile_args(argsfile_text: &str) -> Result<clap::ArgMatches<'static>, AppError> {
    //now strip comments from the args file
    let args_file_contents = match comment::shell::strip(argsfile_text) {
        Ok(args_file_contents) => args_file_contents,
        Err(e) => return Err(ArgsFileParse(PathBuf::from(ARGS_FILE), e.to_string())),
    };

    //the arguments file needs to be split into args in the same way as the shell would do it.
    //call out to an external create for this.
    let args = match shell_words::split(&args_file_contents) {
        Ok(args) => args,
        Err(e) => return Err(ArgsFileParse(PathBuf::from(ARGS_FILE), e.to_string())),
    };

    //When parsing args from file, the binary name will not be present,
    // so update the parser that we use to not expect it.
    let matches = build_app()
        .setting(clap::AppSettings::NoBinaryName)
        .get_matches_from(&args);
    Ok(matches)
}

fn absolutify_path(cwd: &Path, path: &Path) -> PathBuf {
    //get the absolute path if it is not absolute, by prepending the cwd.
    let path = if path.is_relative() {
        cwd.join(path)
    } else {
        path.to_path_buf()
    };
    //println!("absolute path: {:#?}", &p);

    //now try canonicalizing the path. If that fails then silently ignore the failure and carry on (bad idea?)
    let p = path.canonicalize().unwrap_or(path);
    //println!("canonical path: {:#?}", &p);

    p
}
