use std::{ffi::OsString, path::Path, process::Command};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use FfmpegErrorKind::*;

use super::{
    framified_video::FramifiedVideo,
    img_ops::{ImgOpsError, RgbImgBuf},
};
use crate::library::{concrete_cachers::ImgOrFfmpegError, FfmpegCfg};

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum FfmpegErrorKind {
    //these names are rubbish...
    #[error("Error internal to Ffmpeg: {0}")]
    FfmpegFailure(String),

    #[error("Error external to Ffmpeg: {0}")]
    OtherFailure(String),

    #[error("Ffmmpeg decoded no frames from the video")]
    NoFrames,
}

pub fn create_images_into_memory<P: AsRef<Path>>(
    src_path: P,
    cfg: &FfmpegCfg,
) -> Result<FramifiedVideo, ImgOrFfmpegError> {
    if cfg.cropdetect {
        //try loading the video with cropdetection. If that fails (sometimes ffmpeg's cropdetect produces nonsensical results...)
        //then retry without.

        match create_images_into_memory_cropdetect(&src_path, cfg) {
            Ok(images) => Ok(images),
            Err(_crop_error) => match create_images_into_memory_nocrop(&src_path, cfg) {
                Ok(images) => Ok(images),
                Err(nocrop_error) => Err(nocrop_error),
            },
        }
    } else {
        create_images_into_memory_nocrop(src_path, cfg)
    }
}

fn create_images_into_memory_cropdetect<P: AsRef<Path>>(
    src_path: P,
    cfg: &FfmpegCfg,
) -> Result<FramifiedVideo, ImgOrFfmpegError> {
    //first detect if the video is letterboxed with ffmpeg. We will use this to
    //crop the letterboxes out of the spatial hash, which introduces lots of false positives.

    #[rustfmt::skip]
    let output = Command::new("ffmpeg")
        .args(&[
            "-i", &escaped_path(src_path.as_ref()).to_str().unwrap(),
            "-vf", &format!("cropdetect=24:2:0,fps={}", cfg.framerate),
            "-f", "null",
            "-t", "1",
            "-"
        ])
        .output()
        .unwrap();

    let crop_detect_result = std::str::from_utf8(&output.stderr)
        .map_err(|_| OtherFailure("Failed to parse ffmpeg output as utf8".to_string()))?;

    let crops = crop_detect_result.lines().filter_map(|line| line.split("crop=").nth(1));
    let most_pessimistic_crop = crops.max_by_key(|crop| {
        let fields = crop.split(':').collect::<Vec<_>>();
        //warn!("{:?}: {:?}", fields, src_path.as_ref());
        let x_dim = fields.get(0).unwrap_or(&"").parse::<i64>().unwrap_or(i64::MIN);
        let y_dim = fields.get(1).unwrap_or(&"").parse::<i64>().unwrap_or(i64::MIN);
        x_dim.saturating_add(y_dim)
    });

    let cropdetect_string = match most_pessimistic_crop {
        Some(crop) => format!(",crop={}", crop.trim_end()),
        None => "".to_string(),
    };

    create_images_into_memory_inner(src_path, cfg, &cropdetect_string)
}

fn create_images_into_memory_nocrop<P: AsRef<Path>>(
    src_path: P,
    cfg: &FfmpegCfg,
) -> Result<FramifiedVideo, ImgOrFfmpegError> {
    create_images_into_memory_inner(src_path, cfg, &"")
}

fn create_images_into_memory_inner<P: AsRef<Path>>(
    src_path: P,
    cfg: &FfmpegCfg,
    cropdetect_string: &str,
) -> Result<FramifiedVideo, ImgOrFfmpegError> {
    #[rustfmt::skip]
    let output_result = Command::new("ffmpeg")
        .args(&[
            "-hide_banner",
            "-loglevel", "warning",
            "-nostats",
            "-i", &escaped_path(src_path.as_ref()).to_str().unwrap(),
            "-vf", &format!("fps={}{},scale={}x{}", cfg.framerate, cropdetect_string, cfg.dimensions_x, cfg.dimensions_y),
            "-vframes", &cfg.num_frames.to_string(),
            "-pix_fmt", "rgb24",
            "-c:v", "rawvideo",
            "-f", "image2pipe",
            "-"])
        .output();

    if let Ok(output) = output_result {
        if output.status.success() {
            //There is an error case where ffmpeg will report success but actually decode nothing.
            //(I guess valid videos can have no frames?)
            // So capture this here...
            if output.stdout.is_empty() {
                Err(ImgOrFfmpegError::from(NoFrames))
            } else {
                match bytes_to_images(output.stdout, cfg.dimensions_x, cfg.dimensions_y) {
                    Ok(images) => Ok(FramifiedVideo::new(src_path.as_ref(), images)),
                    Err(e) => Err(e.into()),
                }
            }
        } else {
            Err(ImgOrFfmpegError::from(make_ffmpeg_failure(
                String::from_utf8_lossy(&output.stderr).to_string(),
            )))
        }
    } else {
        Err(ImgOrFfmpegError::from(OtherFailure("no path?".to_owned())))
    }
}

pub fn get_video_stats<P: AsRef<Path>>(src_path: P) -> Result<String, FfmpegErrorKind> {
    use FfmpegErrorKind::*;

    #[rustfmt::skip]
    let output_result = Command::new("ffprobe")
        .args(&["-v", "quiet",
        "-show_format",
        "-show_streams",
        "-print_format", "json"])
        .arg(escaped_path(src_path))
        .output();

    if let Ok(output) = output_result {
        if output.status.success() {
            Ok(String::from_utf8(output.stdout)
                .map_err(|_| OtherFailure("Failed to process ffmpeg output as utf8".to_string()))?)
        } else {
            Err(make_ffmpeg_failure(String::from_utf8_lossy(&output.stderr).to_string()))
        }
    } else {
        Err(OtherFailure("no path?".to_owned()))
    }
}

pub fn is_video_file<P: AsRef<Path>>(src_path: P) -> Result<bool, FfmpegErrorKind> {
    let streams_string = is_video_file_output(src_path.as_ref())?;

    let mut fields_iter = streams_string.split('|');

    let codec_name = fields_iter.next().unwrap_or("");
    let codec_type = fields_iter.next().unwrap_or("");
    let duration = fields_iter.next().unwrap_or("").trim().parse::<f64>().unwrap_or(999.0);

    if codec_type != "video" {
        return Ok(false);
    }

    if ["mjpeg", "png", "text", "txt"]
        .iter()
        .any(|&nonvideo_codec| nonvideo_codec == codec_name)
    {
        return Ok(false);
    }

    if duration < 1.0 {
        return Ok(false);
    }

    // println!(
    //     "codec_type: {}, codec_name: {}, duration: {} file: {}",
    //     codec_type,
    //     codec_name,
    //     duration,
    //     src_path.as_ref().display()
    // );

    Ok(true)
}

fn is_video_file_output<P: AsRef<Path>>(src_path: P) -> Result<String, FfmpegErrorKind> {
    //"ffprobe -v error -select_streams v -show_entries stream=codec_type,codec_name,duration -of compact=p=0:nk=1 {}"
    #[rustfmt::skip]
    let output_result = Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-select_streams", "v",
            "-show_entries", "stream=codec_type,codec_name,duration",
            "-of", "compact=p=0:nk=1",
        ])
        .arg(escaped_path(src_path.as_ref()))
        .output();

    if let Ok(output) = output_result {
        if output.status.success() {
            let streams_string = String::from_utf8(output.stdout)
                .map_err(|_| OtherFailure("Failed to process ffmpeg output as utf8".to_string()))?
                .trim()
                .to_string();

            Ok(streams_string)
        } else {
            Err(make_ffmpeg_failure(String::from_utf8_lossy(&output.stderr).to_string()))
        }
    } else {
        Err(OtherFailure("no path?".to_owned()))
    }
}

//sometimes ffmpeg creates very long error messages. Limit them to the first 500 characters
fn make_ffmpeg_failure(msg: String) -> FfmpegErrorKind {
    FfmpegErrorKind::FfmpegFailure(msg.chars().take(500).collect::<String>())
}

fn bytes_to_images(bytes: Vec<u8>, dimensions_x: u32, dimensions_y: u32) -> Result<Vec<RgbImgBuf>, ImgOpsError> {
    let img_size = (dimensions_x * dimensions_y * 3) as usize;
    let chunks = bytes.chunks_exact(img_size);

    chunks
        .map(|chunk| {
            let temp_vec = chunk.into();
            RgbImgBuf::from_raw(dimensions_x, dimensions_y, temp_vec).ok_or(ImgOpsError::RawConversionError)
        })
        .collect()
}

fn escaped_path(path: impl AsRef<Path>) -> OsString {
    let temp = std::borrow::Cow::from(path.as_ref().to_string_lossy().to_string());
    //let escaped = shell_escape::escape(temp.clone()).into_owned();

    OsString::from(temp.into_owned())
}
