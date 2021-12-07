use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use lazy_static::lazy_static;
use thiserror::Error;
use vid_dup_finder_lib::*;
use video_hash_filesystem_cache::*;
use ResolutionError::*;
use TrashError::*;

fn with_extension(recipient: &Path, donor: &Path) -> PathBuf {
    match donor.extension() {
        None => recipient.to_path_buf(),
        Some(ext) => recipient.with_extension(ext),
    }
}

fn with_basename(recipient: &Path, donor: &Path) -> PathBuf {
    let new_basename = donor.file_name().unwrap();
    recipient.with_file_name(new_basename)
}

#[derive(Error, Debug)]

pub enum TrashError {
    #[error("Gui Trash Path not supplied in command line arguments")]
    NoTrashPathError,

    #[error("Failed to open file at path path {0}: {1}")]
    FileOpenError(String, #[source] std::io::Error),

    #[error("Failed to strip prefix '/' from path: {0}")]
    StripPrefixError(#[from] std::path::StripPrefixError),

    #[error("I/O Error at path {0}: {1}")]
    IoError(String, #[source] std::io::Error),

    #[error("Failed to delete file: {0}")]
    DeleteFileFailure(String, #[source] std::io::Error),

    #[error("Source file does not exist: {0}")]
    SourceFileMissing(String),

    #[error("Destination already exists: {0}")]
    DestFileExists(String),

    #[error("Failed to create parent directory for trash file: {0}")]
    CreateParentDirFailure(String),

    #[error("Coudn't extract parent directory from string: {0}")]
    ExtractParentDirFailure(String),

    #[error("move_path: Failed to copy file {0} to {1}")]
    CopyFailError(String, String),

    #[error("move_path: Unhandled error copying {0} to {1}")]
    UnhandledError(String, String),

    #[error("move_path: std::fs::rename returned None for moving {0} to {1}")]
    RenameNoneError(String, String),
}

#[derive(Error, Debug)]
pub enum ResolutionError {
    #[error("Failed to perform trash operation: {0}")]
    TrashFailed(#[from] TrashError),

    #[error("could not validate resolution")]
    ValidationError(String),

    #[error("File to preserve does not exist: {0}")]
    MissingContentsFile(String),

    #[error("Could not parse filename-donor video as integer from resolution string: {0}")]
    ParseBasenameError(String),

    #[error("Could not parse directory-donor video as integer from resolution string: {0}")]
    ParseDirnameError(String),

    #[error("Could not parse contents-donor video as integer from resolution string: {0}")]
    ParseContentsError(String),

    #[error("Could not parse resolution string: {0}")]
    ParseError(String),
}

#[derive(Debug, PartialEq, Default, Clone)]
struct ResolutionThunkEntry {
    filename: PathBuf,
    hash: Option<VideoHash>,
    is_reference: bool,
    stats: VideoStats,
}

#[derive(Debug)]
struct ResolutionInstruction {
    basename_idx: usize,
    dirname_idx: usize,
    contents_idx: usize,
}

pub struct WinningStats {
    pub is_reference: bool,
    pub pngsize: bool,
    pub filesize: bool,
    pub res: bool,
    pub bitrate: bool,
    pub has_audio: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ResolutionThunk {
    entries: Vec<ResolutionThunkEntry>,
    distance: Option<f64>,
    gui_trash_path: Option<PathBuf>,
}

impl ResolutionThunk {
    #[cfg(all(target_family = "unix", feature = "gui"))]
    pub fn from_matchgroup(
        match_group: &MatchGroup,
        cache: &VideoHashFilesystemCache,
        gui_trash_path: &Option<PathBuf>,
    ) -> Self {
        let mut thunk = Self {
            entries: Default::default(),
            distance: Default::default(),
            gui_trash_path: gui_trash_path.clone(),
        };

        //first add the reference, if it exists...
        if let Some(ref reference) = match_group.reference() {
            let ref_stats = cache.fetch_stats(reference).unwrap();
            thunk.insert_reference(reference.to_path_buf(), ref_stats);
        }

        for entry in match_group.duplicates() {
            thunk.insert_entry(entry.to_path_buf(), cache.fetch_stats(entry).unwrap());
        }

        thunk.populate_distance(cache);
        thunk.populate_entries(cache);

        thunk
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn insert_entry(&mut self, filename: PathBuf, stats: VideoStats) {
        self.entries.push(ResolutionThunkEntry {
            filename,
            is_reference: false,
            hash: None,
            stats,
        });
        self.entries
            .sort_by_key(|x| (!x.is_reference, x.filename.as_os_str().len()));
    }

    fn insert_reference(&mut self, filename: PathBuf, stats: VideoStats) {
        self.entries.push(ResolutionThunkEntry {
            filename,
            is_reference: true,
            hash: None,
            stats,
        });
        self.entries
            .sort_by_key(|x| (!x.is_reference, x.filename.as_os_str().len()));
    }

    fn populate_distance(&mut self, cache: &VideoHashFilesystemCache) {
        use itertools::Itertools;
        let max_distance = self
            .entries
            .iter()
            .filter_map(|e| cache.fetch(&e.filename).ok())
            .combinations(2)
            .fold(0.0, |max_distance, pair| {
                let hash_a = &pair[0];
                let hash_b = &pair[1];

                let current_distance = hash_a.normalized_levenshtein_distance(hash_b).value();
                current_distance.max(max_distance)
            });

        self.distance = Some(max_distance);
    }

    fn populate_entries(&mut self, cache: &VideoHashFilesystemCache) {
        self.entries
            .iter_mut()
            .for_each(|e| e.hash = cache.fetch(&e.filename).ok())
    }

    pub fn distance(&self) -> Option<f64> {
        self.distance
    }

    pub fn entries(&self) -> Vec<&Path> {
        self.entries
            .iter()
            .map(|x| x.filename.as_path())
            .collect::<Vec<_>>()
    }

    pub fn hash(&self, src_path: &Path) -> VideoHash {
        self.entries
            .iter()
            .find(|x| x.filename == src_path)
            .unwrap()
            .clone()
            .hash
            .unwrap()
    }

    pub fn calc_winning_stats(&self, filename: &Path) -> WinningStats {
        let best_pngsize = self
            .entries
            .iter()
            .map(|e| e.stats.png_size)
            .max()
            .unwrap_or_default();
        let pngsize_all_eq = self
            .entries
            .iter()
            .all(|e| e.stats.png_size == best_pngsize);

        let best_filesize = self
            .entries
            .iter()
            .map(|e| e.stats.size())
            .max()
            .unwrap_or_default();
        let filesize_all_eq = self.entries.iter().all(|e| e.stats.size() == best_filesize);

        let best_res = self
            .entries
            .iter()
            .map(|e| e.stats.resolution())
            .max_by_key(|(x, y)| x * y)
            .unwrap_or_default();
        let res_all_eq = self
            .entries
            .iter()
            .all(|e| e.stats.resolution() == best_res);

        let best_bitrate = self
            .entries
            .iter()
            .map(|e| e.stats.bit_rate())
            .max()
            .unwrap_or_default();
        let bitrate_all_eq = self
            .entries
            .iter()
            .all(|e| e.stats.bit_rate() == best_bitrate);

        let best_has_audio = self.entries.iter().any(|e| e.stats.has_audio());
        let has_audio_all_eq = self
            .entries
            .iter()
            .all(|e| e.stats.has_audio() == best_has_audio);

        let current_entry = self
            .entries
            .iter()
            .find(|e| e.filename == filename)
            .unwrap();
        let current_stats = &current_entry.stats;

        WinningStats {
            is_reference: current_entry.is_reference,
            pngsize: current_stats.png_size == best_pngsize && !pngsize_all_eq,
            filesize: current_stats.size() == best_filesize && !filesize_all_eq,
            res: current_stats.resolution() == best_res && !res_all_eq,
            bitrate: current_stats.bit_rate() == best_bitrate && !bitrate_all_eq,
            has_audio: current_stats.has_audio() == best_has_audio && !has_audio_all_eq,
        }
    }

    pub fn render_duration(&self, filename: &Path) -> String {
        let stats = &self
            .entries
            .iter()
            .find(|e| e.filename == filename)
            .unwrap()
            .stats;

        let duration = stats.duration() as u64;
        format!("{}:{:02}", duration / 60, duration % 60)
    }

    // pub fn stats(&self, filename: &Path) -> VideoStats {
    //     let stats = &self.entries.iter().find(|e| e.filename == filename).unwrap().stats;

    //     stats.clone()
    // }

    pub fn render_details_top(&self, filename: &Path) -> String {
        let stats = &self
            .entries
            .iter()
            .find(|e| e.filename == filename)
            .unwrap()
            .stats;

        let filesize = byte_unit::Byte::from_bytes(stats.size() as u128);
        let filesize = filesize.get_appropriate_unit(false);

        let pngsize = stats.png_size();
        let pngsize = byte_unit::Byte::from_bytes(pngsize as u128);
        let pngsize = pngsize.get_appropriate_unit(false);

        format!("f_sz: {:>9}, p_sz: {:>9}", filesize, pngsize,)
    }

    pub fn render_details_bottom(&self, filename: &Path) -> String {
        let stats = &self
            .entries
            .iter()
            .find(|e| e.filename == filename)
            .unwrap()
            .stats;

        let bitrate = stats.bit_rate() as f64 / 1_000_000.0;

        format!("res: {:?}, bitrt: {:>03.3} M", stats.resolution(), bitrate,)
    }

    fn parse_choice(&self, choice: &str) -> Result<ResolutionInstruction, ResolutionError> {
        use regex::Regex;

        lazy_static! {
            // example: "k 1 as 2 at 3". Keep video 1's content, video 2's name, at video 3's path.
            static ref RENAME_MOVE_REGEX1: Regex =
                Regex::new(r"^\s*(?P<contents>\d+)\s*as\s*(?P<basename>\d+)\s*at\s*(?P<dirname>\d+)\s*$").unwrap();
            static ref RENAME_MOVE_REGEX2: Regex =
                Regex::new(r"^\s*(?P<contents>\d+)\s*at\s*(?P<dirname>\d+)\s*as\s*(?P<basename>\d+)\s*$").unwrap();

            // example: "k 1 at 2". Keep video 1's content, video 1's name, at video 2's path.
            static ref  MOVE_REGEX: Regex =
                Regex::new(r"^\s*(?P<contentsbasename>\d+)\s*at\s*(?P<dirname>\d+)\s*$").unwrap();

            //example "k 1 as 2". Keep video 1's content, video 2's name, at video 2's path.
            static ref RENAME_REGEX: Regex =
                Regex::new(r"^\s*(?P<contents>\d+)\s*as\s*(?P<basenamedirname>\d+)\s*$").unwrap();

            //example: "k 1". keep video 1's content, video 1's name, at video 1's path.
            static ref KEEP_REGEX: Regex =
                Regex::new(r"^\s*(?P<contentsbasenamedirname>\d+)\s*$").unwrap();
        }

        //The matches indices as strings.
        let contents_str;
        let basename_str;
        let dirname_str;

        if let Some(caps) = RENAME_MOVE_REGEX1.captures(choice) {
            contents_str = caps["contents"].to_string();
            basename_str = caps["basename"].to_string();
            dirname_str = caps["dirname"].to_string();
        } else if let Some(caps) = RENAME_MOVE_REGEX2.captures(choice) {
            contents_str = caps["contents"].to_string();
            basename_str = caps["basename"].to_string();
            dirname_str = caps["dirname"].to_string();
        } else if let Some(caps) = MOVE_REGEX.captures(choice) {
            contents_str = caps["contentsbasename"].to_string();
            basename_str = caps["contentsbasename"].to_string();
            dirname_str = caps["dirname"].to_string();
        } else if let Some(caps) = RENAME_REGEX.captures(choice) {
            contents_str = caps["contents"].to_string();
            basename_str = caps["basenamedirname"].to_string();
            dirname_str = caps["basenamedirname"].to_string();
        } else if let Some(caps) = KEEP_REGEX.captures(choice) {
            contents_str = caps["contentsbasenamedirname"].to_string();
            basename_str = caps["contentsbasenamedirname"].to_string();
            dirname_str = caps["contentsbasenamedirname"].to_string();
        } else {
            return Err(ParseError(choice.to_string()));
        }

        //now make sure the indices parse as integers.
        let contents_idx = contents_str
            .parse::<usize>()
            .map_err(|_| ParseContentsError(contents_str.to_string()))?;
        let basename_idx = basename_str
            .parse::<usize>()
            .map_err(|_| ParseBasenameError(basename_str.to_string()))?;
        let dirname_idx = dirname_str
            .parse::<usize>()
            .map_err(|_| ParseDirnameError(dirname_str.to_string()))?;

        Ok(ResolutionInstruction {
            basename_idx,
            dirname_idx,
            contents_idx,
        })
    }

    fn validate_choice(&self, choice: &ResolutionInstruction) -> Result<(), ResolutionError> {
        //trace!("{:?}", choice);
        let ResolutionInstruction {
            basename_idx,
            dirname_idx,
            contents_idx,
        } = choice;

        let basename_valid = self.entries.get(*basename_idx).is_some();
        let dirname_valid = self.entries.get(*dirname_idx).is_some();
        let contents_valid = self.entries.get(*contents_idx).is_some();

        if basename_valid && dirname_valid && contents_valid {
            Ok(())
        } else {
            let mut err_string = String::new();
            if !basename_valid {
                err_string += &format!("basename index not valid: {}. ", basename_idx);
            }

            if !dirname_valid {
                err_string += &format!("dirname index not valid: {}. ", dirname_idx);
            }

            if !contents_valid {
                err_string += &format!("contents index not valid: {}.", contents_idx);
            }

            //remove trailing space.
            err_string = err_string.trim().to_string();

            Err(ValidationError(err_string))
        }
    }

    pub fn resolve(&self, choice: &str) -> Result<(), ResolutionError> {
        let choice = self.parse_choice(choice)?;
        self.validate_choice(&choice)?;

        let ResolutionInstruction {
            basename_idx,
            dirname_idx,
            contents_idx,
        } = choice;

        let dirname_entry = &self.entries[dirname_idx];
        let contents_entry = &self.entries[contents_idx];
        let basename_entry = &self.entries[basename_idx];

        let entries_to_trash = self.entries.iter().filter(|&entry| entry != contents_entry);

        //If the contents_entry is to be renamed, get the new name.
        let new_name;
        let need_to_move_contents;
        if (contents_entry == basename_entry) && (contents_entry == dirname_entry) {
            need_to_move_contents = false;
            new_name = contents_entry.filename.clone();
        } else {
            need_to_move_contents = true;
            let new_name_with_wrong_ext =
                with_basename(&dirname_entry.filename, &basename_entry.filename);
            new_name = with_extension(&new_name_with_wrong_ext, &contents_entry.filename);

            //abort early if new_name already exists and would not be deleted in the trashing phase
            if new_name.exists() && entries_to_trash.clone().all(|e| e.filename != new_name) {
                return Err(DestFileExists(new_name.to_string_lossy().to_string()).into());
            }
        }

        //check that the file to keep exists.
        debug!("Checking that contents exists");
        if !contents_entry.filename.exists() {
            return Err(MissingContentsFile(
                contents_entry.filename.to_string_lossy().to_string(),
            ));
        }

        //now trash all other entries (ignoring contents_entry)
        debug!("Trashing all files except contents_entry");
        for entry in entries_to_trash {
            self.trash_file(&entry.filename)?;
        }

        if need_to_move_contents {
            debug!("Moving contents_entry to dir of dirname_entry with name of basename_entry");
            move_path(&contents_entry.filename, &new_name)?;
        }

        Ok(())
    }

    fn get_trash_path(&self, p: &Path) -> Result<PathBuf, TrashError> {
        let relative_filename = p.strip_prefix("/")?;
        self.gui_trash_path
            .as_ref()
            .map(|p| p.join(relative_filename))
            .ok_or(NoTrashPathError)
    }

    fn trash_file(&self, old_path: &Path) -> Result<(), TrashError> {
        fn is_already_trashed(old_path: &Path, trash_path: &Path) -> Result<bool, TrashError> {
            //If there is no file in the trash path, then it is not already trashed.
            if !trash_path.exists() {
                return Ok(false);
            }

            fn sha2_file(path: &Path) -> Result<[u8; 32], TrashError> {
                use sha2::Digest;

                let mut file = match std::fs::File::open(&path) {
                    Ok(file) => Ok(file),
                    Err(e) => Err(TrashError::FileOpenError(
                        path.to_string_lossy().to_string(),
                        e,
                    )),
                }?;
                let mut hasher = sha2::Sha256::new();

                match std::io::copy(&mut file, &mut hasher) {
                    Ok(_) => Ok(hasher.finalize().into()),
                    Err(e) => Err(TrashError::IoError(path.to_string_lossy().to_string(), e)),
                }
            }

            Ok(sha2_file(old_path)? == sha2_file(trash_path)?)
        }

        let new_path = self.get_trash_path(old_path)?;

        println!("trashing {}", old_path.display());

        match is_already_trashed(old_path, &new_path)? {
            true => delete_path(old_path)?,
            false => move_path(old_path, &new_path)?,
        }

        Ok(())
    }
}

fn delete_path(path: &Path) -> Result<(), TrashError> {
    println!("Deleting {}", path.display());

    if let Err(e) = std::fs::remove_file(&path) {
        let e = DeleteFileFailure(path.to_string_lossy().to_string(), e);
        return Err(e);
    };

    Ok(())
}

fn move_path(source: &Path, dest: &Path) -> Result<(), TrashError> {
    println!("Moving {} ------> {}", source.display(), dest.display());

    if !source.exists() {
        return Err(SourceFileMissing(source.to_string_lossy().to_string()));
    }

    let dest = get_new_name_if_path_already_exists(dest);

    #[allow(clippy::question_mark)] //spurious
    match dest.parent() {
        Some(parent_dir) => {
            if std::fs::create_dir_all(parent_dir).is_err() {
                return Err(CreateParentDirFailure(
                    parent_dir.to_string_lossy().to_string(),
                ));
            }
        }
        None => {
            return Err(ExtractParentDirFailure(dest.to_string_lossy().to_string()));
        }
    };

    if let Err(e) = std::fs::rename(&source, &dest) {
        match e.raw_os_error() {
            Some(libc::EPERM) | Some(libc::EXDEV) => {
                //try copy and delete.
                println!("Unable to move. Performing copy and delete instead.");
                if let Err(_e) = std::fs::copy(&source, &dest) {
                    let e = CopyFailError(
                        source.to_string_lossy().to_string(),
                        dest.to_string_lossy().to_string(),
                    );
                    return Err(e);
                };
                delete_path(source)?;
            }
            Some(_) => {
                let e = UnhandledError(
                    source.to_string_lossy().to_string(),
                    dest.to_string_lossy().to_string(),
                );
                return Err(e);
            }
            None => {
                let e = RenameNoneError(
                    source.to_string_lossy().to_string(),
                    dest.to_string_lossy().to_string(),
                );
                return Err(e);
            }
        }
    }

    Ok(())
}

//with a given path, check if it already exists on the filesystem.
//If it does, append a suffix that does not exist (in the form "(1)" or "(2)" etc..)
//until a filename is found that does exist. Then return the new name.
fn get_new_name_if_path_already_exists(p: &Path) -> PathBuf {
    let original_stem = p.file_stem().unwrap();
    let extension = p.extension();

    let mut ret = p.to_path_buf();
    let mut counter = 1u64;
    while ret.exists() {
        let mut new_file_stem = original_stem.to_os_string();
        new_file_stem.push(OsString::from(format!(" ({})", counter)));
        ret.set_file_name(new_file_stem);
        if let Some(ref extension) = extension {
            ret.set_extension(extension);
        }

        counter += 1;
    }

    ret
}
