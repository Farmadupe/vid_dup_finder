use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use thiserror::Error;
use ResolutionError::*;
use TrashError::*;

use crate::library::*;

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

    #[error("choice pair len not 2")]
    MoreThanTwoVideos,

    #[error("could not validate resolution")]
    ValidationError,

    #[error("File to preserve does not exist: {0}")]
    MissingFileToPreserve(String),

    #[error("Could not parse name-donor video as integer from resolution string: {0}")]
    ParseNameDonorError(String),

    #[error("Could not parse location-donor video as integer from resolution string: {0}")]
    ParseLocationDonorError(String),

    #[error("Could not parse contents-donor video as integer from resolution string: {0}")]
    ParseContentsDonorError(String),

    #[error("Could not parse video as integer from resolution string: {0}")]
    ParseChosenVideoError(String),
}

#[derive(Debug, PartialEq, Default, Clone)]
struct ResolutionThunkEntry {
    filename: PathBuf,
    hash: Option<TemporalHash>,
    is_reference: bool,
    stats: VideoStats,
}

#[derive(Debug)]
enum ResolutionInstruction {
    Keep(usize),
    Move {
        location_idx: usize,
        contents_idx: usize,
    },
    MoveAndRename {
        name_idx: usize,
        location_idx: usize,
        contents_idx: usize,
    },
}

pub struct WinningStats {
    pub is_reference: bool,
    pub pngsize: bool,
    pub filesize: bool,
    pub res: bool,
    pub bitrate: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ResolutionThunk {
    entries: Vec<ResolutionThunkEntry>,
    distance: Option<Distance>,
}

impl ResolutionThunk {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn insert_entry(&mut self, filename: PathBuf, stats: VideoStats) {
        self.entries.push(ResolutionThunkEntry {
            filename,
            is_reference: false,
            hash: None,
            stats,
        });
        self.entries.sort_by_key(|x| x.filename.as_os_str().len())
    }

    pub fn insert_reference(&mut self, filename: PathBuf, stats: VideoStats) {
        self.entries.push(ResolutionThunkEntry {
            filename,
            is_reference: true,
            hash: None,
            stats,
        });
        self.entries.sort_by_key(|x| x.filename.as_os_str().len())
    }

    pub fn populate_distance(&mut self, cache: &DupFinderCache) {
        use itertools::Itertools;
        let max_distance = self
            .entries
            .iter()
            .filter_map(|e| cache.get_hash(&e.filename).ok())
            .combinations(2)
            .map(|pair| {
                let hash_a = &pair[0];
                let hash_b = &pair[1];
                let distance = hash_a.distance(hash_b);
                distance
            })
            .max();

        self.distance = max_distance;
    }

    pub fn populate_entries(&mut self, cache: &DupFinderCache) {
        self.entries
            .iter_mut()
            .for_each(|e| e.hash = cache.get_hash(&e.filename).ok())
    }

    pub fn distance(&self) -> Option<Distance> {
        self.distance.clone()
    }

    pub fn entries(&self) -> Vec<&Path> {
        self.entries.iter().map(|x| x.filename.as_path()).collect::<Vec<_>>()
    }

    pub fn hash(&self, src_path: &Path) -> TemporalHash {
        self.entries
            .iter()
            .find(|x| x.filename == src_path)
            .unwrap()
            .clone()
            .hash
            .unwrap()
    }

    pub fn calc_winning_stats(&self, filename: &Path) -> WinningStats {
        let best_pngsize = self.entries.iter().map(|e| e.stats.png_size).max().unwrap_or_default();
        let pngsize_all_eq = self.entries.iter().all(|e| e.stats.png_size == best_pngsize);

        let best_filesize = self.entries.iter().map(|e| e.stats.size).max().unwrap_or_default();
        let filesize_all_eq = self.entries.iter().all(|e| e.stats.size == best_filesize);

        let best_res = self
            .entries
            .iter()
            .map(|e| e.stats.resolution)
            .max_by_key(|res| res.0 + res.1)
            .unwrap_or_default();
        let res_all_eq = self.entries.iter().all(|e| e.stats.resolution == best_res);

        let best_bitrate = self.entries.iter().map(|e| e.stats.bit_rate).max().unwrap_or_default();
        let bitrate_all_eq = self.entries.iter().all(|e| e.stats.bit_rate == best_bitrate);

        let current_entry = self.entries.iter().find(|e| e.filename == filename).unwrap();
        let current_stats = &current_entry.stats;

        WinningStats {
            is_reference: current_entry.is_reference,
            pngsize: current_stats.png_size == best_pngsize && !pngsize_all_eq,
            filesize: current_stats.size == best_filesize && !filesize_all_eq,
            res: current_stats.resolution == best_res && !res_all_eq,
            bitrate: current_stats.bit_rate == best_bitrate && !bitrate_all_eq,
        }
    }

    pub fn render_duration(&self, filename: &Path) -> String {
        let stats = &self.entries.iter().find(|e| e.filename == filename).unwrap().stats;

        let duration = stats.duration as u64;
        format!("{}:{:02}", duration / 60, duration % 60)
    }

    pub fn stats(&self, filename: &Path) -> VideoStats {
        let stats = &self.entries.iter().find(|e| e.filename == filename).unwrap().stats;

        stats.clone()
    }

    pub fn render_details_top(&self, filename: &Path) -> String {
        let stats = &self.entries.iter().find(|e| e.filename == filename).unwrap().stats;

        let filesize = byte_unit::Byte::from_bytes(stats.size as u128);
        let filesize = filesize.get_appropriate_unit(false);

        let pngsize = byte_unit::Byte::from_bytes(stats.png_size as u128);
        let pngsize = pngsize.get_appropriate_unit(false);

        format!("f_sz: {:>9}, p_sz: {:>9}", filesize, pngsize,)
    }

    pub fn render_details_bottom(&self, filename: &Path) -> String {
        let stats = &self.entries.iter().find(|e| e.filename == filename).unwrap().stats;

        let bitrate = stats.bit_rate as f64 / 1_000_000.0;

        format!("res: {:?}, bitrt: {:>03.3} M", stats.resolution, bitrate,)
    }

    fn parse_choice(&self, choice: &str) -> Result<ResolutionInstruction, ResolutionError> {
        let choice = choice.trim();

        if choice.matches(" as ").count() == 1 && choice.matches(" at ").count() == 1 {
            let split_at_at = choice.split(" at ").map(&str::trim);

            let split_at_as = split_at_at.clone().nth(1).unwrap().split(" as ").map(&str::trim);

            let mut choice_triple = std::iter::once(split_at_at.clone().next().unwrap()).chain(split_at_as);

            let contents_idx = match choice_triple.next() {
                Some(idx_str) => match idx_str.parse::<usize>() {
                    Ok(idx) => idx,
                    Err(_e) => return Err(ParseContentsDonorError(idx_str.to_string())),
                },
                None => return Err(ParseContentsDonorError("".to_string())),
            };

            let location_idx = match choice_triple.next() {
                Some(idx_str) => match idx_str.parse::<usize>() {
                    Ok(idx) => idx,
                    Err(_e) => return Err(ParseLocationDonorError(idx_str.to_string())),
                },
                None => return Err(ParseLocationDonorError("".to_string())),
            };

            let name_idx = match choice_triple.next() {
                Some(idx_str) => match idx_str.parse::<usize>() {
                    Ok(idx) => idx,
                    Err(_e) => return Err(ParseNameDonorError(idx_str.to_string())),
                },
                None => return Err(ParseNameDonorError("".to_string())),
            };

            let ret = ResolutionInstruction::MoveAndRename {
                name_idx,
                location_idx,
                contents_idx,
            };

            //warn!("{:?}", ret);

            Ok(ret)
        } else if choice.matches(" at ").count() == 1 {
            let mut choice_pair = choice.split(" at ").map(&str::trim);

            if choice_pair.clone().count() != 2 {
                return Err(MoreThanTwoVideos);
            }

            let contents_idx = match choice_pair.next() {
                Some(idx_str) => match idx_str.parse::<usize>() {
                    Ok(idx) => idx,
                    Err(_e) => return Err(ParseContentsDonorError(idx_str.to_string())),
                },
                None => return Err(ParseContentsDonorError("".to_string())),
            };

            let location_idx = match choice_pair.next() {
                Some(idx_str) => match idx_str.parse::<usize>() {
                    Ok(idx) => idx,
                    Err(_e) => return Err(ParseLocationDonorError(idx_str.to_string())),
                },
                None => return Err(ParseLocationDonorError("".to_string())),
            };

            let ret = ResolutionInstruction::Move {
                location_idx,
                contents_idx,
            };

            Ok(ret)
        } else {
            let idx = match choice.parse::<usize>() {
                Ok(idx) => idx,
                Err(_e) => return Err(ParseChosenVideoError(choice.to_string())),
            };

            Ok(ResolutionInstruction::Keep(idx))
        }
    }

    fn validate_choice(&self, choice: &ResolutionInstruction) -> Result<(), ResolutionError> {
        //trace!("{:?}", choice);
        use ResolutionInstruction::*;
        match choice {
            Keep(idx) => {
                if self.entries.get(*idx).is_some() {
                    Ok(())
                } else {
                    Err(ValidationError)
                }
            }
            Move {
                location_idx,
                contents_idx,
            } => {
                if self.entries.get(*location_idx).is_some() && self.entries.get(*contents_idx).is_some() {
                    Ok(())
                } else {
                    Err(ValidationError)
                }
            }
            MoveAndRename {
                name_idx,
                location_idx,
                contents_idx,
            } => {
                if self.entries.get(*location_idx).is_some()
                    && self.entries.get(*contents_idx).is_some()
                    && self.entries.get(*name_idx).is_some()
                {
                    Ok(())
                } else {
                    Err(ValidationError)
                }
            }
        }
    }

    pub fn resolve(&self, choice: &str) -> Result<(), ResolutionError> {
        let choice = self.parse_choice(choice)?;
        self.validate_choice(&choice)?;

        match choice {
            //the user wants to keep one file. So delete all others.
            ResolutionInstruction::Keep(idx) => {
                let keep_entry = &self.entries[idx];

                println!("Preserving {}", keep_entry.filename.display());

                //first check that the file to keep exists. Otherwise conservatively do nothing.
                if !keep_entry.filename.exists() {
                    return Err(MissingFileToPreserve(keep_entry.filename.to_string_lossy().to_string()));
                }

                //Now trash all files except the one for preservation.
                for trash_entry in &self.entries {
                    if trash_entry.filename != keep_entry.filename {
                        trash_file(&trash_entry.filename)?;
                    }
                }
            }

            ResolutionInstruction::Move {
                location_idx,
                contents_idx,
            } => {
                let location_entry = &self.entries[location_idx];
                let contents_entry = &self.entries[contents_idx];

                let entries_to_trash = self
                    .entries
                    .iter()
                    .filter(|&entry| entry != contents_entry)
                    .collect::<Vec<_>>();

                let new_name = with_basename(&location_entry.filename, &contents_entry.filename);

                //abort early if new_name already exists and would not be deleted in the trashing phase
                if new_name.exists() && entries_to_trash.iter().all(|e| e.filename != new_name) {
                    return Err(DestFileExists(new_name.to_string_lossy().to_string()).into());
                }

                //check that the file to keep exists.
                debug!("Checking that contents exists");
                if !contents_entry.filename.exists() {
                    return Err(MissingFileToPreserve(
                        contents_entry.filename.to_string_lossy().to_string(),
                    ));
                }

                debug!("Trashing all files except contents_entry");
                //now trash all other entries (ignoring contents_entry)
                let remaining_entries = self.entries.iter().filter(|&entry| entry != contents_entry);
                for entry in remaining_entries {
                    trash_file(&entry.filename)?;
                }

                debug!("Moving contents_entry to dir of location_entry");
                //move the contents_entry into its new home.
                move_path(&contents_entry.filename, &new_name)?;
            }

            ResolutionInstruction::MoveAndRename {
                name_idx,
                location_idx,
                contents_idx,
            } => {
                let location_entry = &self.entries[location_idx];
                let contents_entry = &self.entries[contents_idx];
                let name_entry = &self.entries[name_idx];

                let entries_to_trash = self.entries.iter().filter(|&entry| entry != contents_entry);

                //first calculate the new name and check that there is nothing already there.
                let new_name_with_wrong_ext = with_basename(&location_entry.filename, &name_entry.filename);
                let new_name = with_extension(&new_name_with_wrong_ext, &contents_entry.filename);

                //abort early if new_name already exists and would not be deleted in the trashing phase
                if new_name.exists() && entries_to_trash.clone().all(|e| e.filename == new_name) {
                    return Err(DestFileExists(new_name.to_string_lossy().to_string()).into());
                }

                //check that the file to keep exists.
                debug!("Checking that contents exists");
                if !contents_entry.filename.exists() {
                    return Err(MissingFileToPreserve(
                        contents_entry.filename.to_string_lossy().to_string(),
                    ));
                }

                debug!("Trashing all files except contents_entry");
                //now trash all other entries (ignoring contents_entry)
                for entry in entries_to_trash {
                    trash_file(&entry.filename)?;
                }

                debug!("Moving contents_entry to dir of location_entry with name of name_entry");
                move_path(&contents_entry.filename, &new_name)?;
            }
        }

        Ok(())
    }
}

fn trash_file(old_path: &Path) -> Result<(), TrashError> {
    fn get_trash_path(p: &Path) -> Result<PathBuf, TrashError> {
        let new_root_dir = PathBuf::from(&"/mnt/ssd-luks/old_dups");
        let relative_filename = p.strip_prefix("/")?;
        Ok(new_root_dir.join(relative_filename))
    }

    fn is_already_trashed(old_path: &Path, trash_path: &Path) -> Result<bool, TrashError> {
        //If there is no file in the trash path, then it is not already trashed.
        if !trash_path.exists() {
            return Ok(false);
        }

        fn sha2_file(path: &Path) -> Result<[u8; 32], TrashError> {
            use sha2::Digest;

            let mut file = match std::fs::File::open(&path) {
                Ok(file) => Ok(file),
                Err(e) => Err(TrashError::FileOpenError(path.to_string_lossy().to_string(), e)),
            }?;
            let mut hasher = sha2::Sha256::new();

            match std::io::copy(&mut file, &mut hasher) {
                Ok(_) => Ok(hasher.finalize().into()),
                Err(e) => Err(TrashError::IoError(path.to_string_lossy().to_string(), e)),
            }
        }

        Ok(sha2_file(old_path)? == sha2_file(trash_path)?)
    }

    let new_path = get_trash_path(old_path)?;

    println!("trashing {}", old_path.display());

    match is_already_trashed(old_path, &new_path)? {
        true => delete_path(old_path)?,
        false => move_path(old_path, &new_path)?,
    }

    Ok(())
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

    match dest.parent() {
        Some(parent_dir) => {
            if std::fs::create_dir_all(parent_dir).is_err() {
                return Err(CreateParentDirFailure(parent_dir.to_string_lossy().to_string()));
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
                if let Err(_e) = std::fs::copy(&source, &dest) {
                    let e = CopyFailError(source.to_string_lossy().to_string(), dest.to_string_lossy().to_string());
                    return Err(e);
                };
                delete_path(&source)?;
            }
            Some(_) => {
                let e = UnhandledError(source.to_string_lossy().to_string(), dest.to_string_lossy().to_string());
                return Err(e);
            }
            None => {
                let e = RenameNoneError(source.to_string_lossy().to_string(), dest.to_string_lossy().to_string());
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
