use std::{
    path::{Path, PathBuf},
    process::Command,
};

use chrono::NaiveTime;
use itertools::Itertools;
use rayon::prelude::*;

//dir where the raw fudan dataset lives.
fn test_src_dir() -> &'static Path {
    &Path::new("/mnt/ssd-luks/fudan_dataset/source")
}

//dir where extracted annotations will be placed.
fn fudan_processed_dir() -> &'static Path {
    &Path::new("/mnt/ssd-luks/fudan_dataset/processed")
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VideoClip {
    src_path: PathBuf,
    processed_path: PathBuf,
    start: NaiveTime,
    end: NaiveTime,
}

impl VideoClip {
    fn from_fields(title: &str, file_name: &Path, start: &str, end: &str) -> Self {
        let src_path = test_src_dir()
            .join(PathBuf::from("core_dataset"))
            .join(PathBuf::from(title).join(file_name));

        let file_name_string = file_name.to_string_lossy();
        let (basename, extname) = file_name_string.rsplit_once('.').unwrap();
        let file_name_with_id = format!("{}_{}-{}.{}", basename, start, end, extname);

        let processed_path = fudan_processed_dir()
            .join(PathBuf::from(title))
            .join(PathBuf::from(file_name_with_id));

        Self {
            src_path,
            processed_path,
            start: NaiveTime::parse_from_str(start, "%H:%M:%S").unwrap(),
            end: NaiveTime::parse_from_str(end, "%H:%M:%S").unwrap(),
        }
    }

    fn extract(&self) -> Result<(), std::io::Error> {
        if self.processed_path.exists() {
            return Ok(());
        }

        //now make the output directory if it does not yet exist.
        if let Some(parent_dir) = self.processed_path.parent() {
            std::fs::create_dir_all(parent_dir)?;
        }

        let cmd = self.generate_ffmpeg_command();

        //println!("{}", &cmd);

        match Command::new("sh").arg("-c").arg(&cmd).output() {
            Ok(_output) => {
                // println!(
                //     "\n\n{}\n{}",
                //     std::str::from_utf8(&output.stdout).unwrap(),
                //     std::str::from_utf8(&output.stderr).unwrap()
                // )
            }
            Err(_) => {
                panic!()
            }
        }

        Ok(())
    }

    fn generate_ffmpeg_command(&self) -> String {
        let cmd = format!(
            "ffmpeg -hide_banner -loglevel warning -nostats -i {src_path} -s {start} -to {end} -c copy {processed_path}",
            src_path = shell_words::quote(self.src_path.to_str().unwrap()),
            start = self.start,
            end = self.end,
            processed_path = shell_words::quote(self.processed_path.to_str().unwrap())
        );

        cmd
    }

    pub fn processed_path(&self) -> &Path {
        &self.processed_path
    }

    fn duration(&self) -> i64 {
        (self.end - self.start).num_seconds()
    }
}

#[derive(Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Annotation {
    title: String,
    clip_a: VideoClip,
    clip_b: VideoClip,
}

impl Annotation {
    fn from_str_and_title(title: String, s: &str) -> Self {
        let fields: Vec<&str> = s.split(',').collect();

        let video_a = *fields.get(0).unwrap();
        let video_b = *fields.get(1).unwrap();
        let a_start = *fields.get(2).unwrap();
        let a_end = *fields.get(3).unwrap();
        let b_start = *fields.get(4).unwrap();
        let b_end = *fields.get(5).unwrap();

        let title_pb = PathBuf::from(title);
        let title = match &title_pb.components().last().unwrap() {
            std::path::Component::Normal(x) => x,
            _ => panic!(),
        }
        .to_str()
        .unwrap();
        let title = title[..title.len() - 4].to_string();

        Self {
            title: title.clone(),
            clip_a: VideoClip::from_fields(&title, &PathBuf::from(video_a), a_start, a_end),
            clip_b: VideoClip::from_fields(&title, &PathBuf::from(video_b), b_start, b_end),
        }
    }

    pub fn iter_videos(&self) -> ClipIter {
        ClipIter {
            annotation: &self,
            state: ClipIterState::A,
        }
    }

    #[allow(dead_code)]
    pub fn is_match(&self, (clip_a, clip_b): (&Path, &Path)) -> bool {
        (self.clip_a.processed_path() == clip_a && self.clip_b.processed_path() == clip_b)
            || (self.clip_a.processed_path() == clip_b && self.clip_b.processed_path() == clip_a)
    }

    fn duration(&self) -> i64 {
        self.clip_a.duration().min(self.clip_b.duration())
    }
}

enum ClipIterState {
    A,
    B,
    Done,
}

pub struct ClipIter<'a> {
    annotation: &'a Annotation,
    state: ClipIterState,
}

impl<'a> Iterator for ClipIter<'a> {
    type Item = &'a VideoClip;

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            ClipIterState::A => {
                self.state = ClipIterState::B;
                Some(&self.annotation.clip_a)
            }
            ClipIterState::B => {
                self.state = ClipIterState::Done;
                Some(&self.annotation.clip_b)
            }
            ClipIterState::Done => None,
        }
    }
}

fn load_annotations() -> std::io::Result<Vec<Annotation>> {
    let annotations_dir = test_src_dir().join("annotation");

    let annotations = std::fs::read_dir(&annotations_dir)?
        .filter_map(Result::ok)
        .map(|x| x.path())
        .filter(|x| x.extension().unwrap() == "txt")
        .sorted()
        .flat_map(|annotation_file| {
            std::fs::read_to_string(&annotation_file.clone())
                .unwrap()
                .lines()
                .map(|line| Annotation::from_str_and_title(annotation_file.to_string_lossy().to_string(), &line))
                .collect::<Vec<_>>()
        })
        .dedup()
        .filter(|annotation| annotation.duration() > 30)
        .sorted_by_key(|annotation| annotation.duration())
        .collect::<Vec<_>>();

    //println!("{:#?}", annotations);

    Ok(annotations)
}

pub fn preprocess() -> std::io::Result<Vec<Annotation>> {
    let annotations = load_annotations().unwrap();

    let clips = annotations.iter().flat_map(|annotation| annotation.iter_videos());

    clips.par_bridge().for_each(|clip| {
        println!("{:#?}", clip.src_path);
        clip.extract().unwrap()
    });

    Ok(annotations)
}
