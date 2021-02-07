use std::path::Path;

use image::DynamicImage;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::library::{
    concrete_cachers::ImgOrFfmpegError,
    ffmpeg_ops::{create_images_into_memory, get_video_stats},
    img_ops::ImgOpsError,
    FfmpegCfg,
};

#[derive(Debug, Deserialize, Serialize, Clone, Error)]
pub enum StatsCalculationError {
    ImgFfmpeg(#[from] ImgOrFfmpegError),
    JsonError(String),
    ParseIntError(String),
    ParseFloatError(String),
}

impl From<serde_json::Error> for StatsCalculationError {
    fn from(e: serde_json::Error) -> Self {
        StatsCalculationError::JsonError(format!("{}", e))
    }
}

impl From<std::num::ParseIntError> for StatsCalculationError {
    fn from(e: std::num::ParseIntError) -> Self {
        StatsCalculationError::ParseIntError(format!("{}", e))
    }
}

impl From<std::num::ParseFloatError> for StatsCalculationError {
    fn from(e: std::num::ParseFloatError) -> Self {
        StatsCalculationError::ParseFloatError(format!("{}", e))
    }
}

impl std::fmt::Display for StatsCalculationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StatsCalculationError::ImgFfmpeg(e) => {
                write!(f, "Error processing video for pngsize calculation: {}", e)
            }
            StatsCalculationError::JsonError(e) => {
                write!(f, "Error parsing stats: {}", e)
            }
            StatsCalculationError::ParseIntError(e) => {
                write!(f, "Error parsing stats: {}", e)
            }
            StatsCalculationError::ParseFloatError(e) => {
                write!(f, "Error parsing stats: {}", e)
            }
        }
    }
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize, Default)]
pub struct VideoStats {
    pub duration: f64,
    pub size: u64,
    pub bit_rate: u32,
    pub resolution: (u32, u32),
    pub has_audio: bool,
    pub png_size: u32,
}

impl VideoStats {
    pub fn new<P>(src_path: P) -> Result<Self, StatsCalculationError>
    where
        P: AsRef<Path>,
    {
        use serde_json::Value;

        let stats_string = get_video_stats(&src_path).map_err(ImgOrFfmpegError::from)?;
        let stats_parsed: Value = serde_json::from_str(&stats_string)?;

        let duration = &stats_parsed["format"]["duration"];
        let duration = if let Value::String(d) = duration {
            d.parse()?
        } else {
            0.0
        };

        let size = &stats_parsed["format"]["size"];
        let size = if let Value::String(s) = size { s.parse()? } else { 0 };

        let bit_rate = &stats_parsed["format"]["bit_rate"];
        let bit_rate = if let Value::String(br) = bit_rate {
            br.parse()?
        } else {
            0
        };

        fn streams_video_iter(stats_parsed: &Value) -> Option<Vec<Value>> {
            if let Value::Array(streams) = &stats_parsed["streams"] {
                let ret = streams
                    .iter()
                    .filter(|s| match &s["codec_type"] {
                        Value::String(codec_type) => codec_type == "video",
                        _ => false,
                    })
                    .cloned()
                    .collect();

                Some(ret)
            } else {
                None
            }
        }

        let width = if let Some(streams) = streams_video_iter(&stats_parsed) {
            if let Some(width) = streams
                .iter()
                .filter_map(|stream| {
                    if let Value::Number(v) = &stream["width"] {
                        Some(v.as_u64()? as u32)
                    } else {
                        None
                    }
                })
                .next()
            {
                width
            } else {
                0
            }
        } else {
            0
        };

        let height = if let Some(streams) = streams_video_iter(&stats_parsed) {
            if let Some(height) = streams
                .iter()
                .filter_map(|stream| {
                    if let Value::Number(v) = &stream["height"] {
                        Some(v.as_u64()? as u32)
                    } else {
                        None
                    }
                })
                .next()
            {
                height
            } else {
                0
            }
        } else {
            0
        };

        let resolution = (width, height);

        let streams = &stats_parsed["streams"];
        let has_audio = if let Value::Array(streams) = streams {
            streams.iter().any(|stream| match &stream["codec_type"] {
                Value::String(codec_type) => codec_type == "audio",
                _ => false,
            })
        } else {
            false
        };

        let png_size = if let Ok(png_size) = png_size(&src_path.as_ref()) {
            png_size as u32
        } else {
            0
        };

        Ok(VideoStats {
            duration,
            size,
            bit_rate,
            resolution,
            has_audio,
            png_size,
        })
    }

    pub fn is_match(&self, other: &Self) -> bool {
        //if the durations match within 5%, then they're a match. Simple!
        let duration_ratio: f64 = other.duration / self.duration;
        (0.95..=1.05).contains(&duration_ratio)
    }
}

fn png_size(path: &Path) -> Result<usize, StatsCalculationError> {
    let cfg = &FfmpegCfg {
        dimensions_x: 1024,
        dimensions_y: 1024,
        num_frames: 10,
        framerate: "1/3".to_string(),
        cropdetect: true,
    };

    let images = create_images_into_memory(path, &cfg)?;
    let asidened = images.to_asidened_image()?;

    let row_dyn = DynamicImage::ImageRgb8(asidened);
    let mut png_encoding = vec![];
    row_dyn
        .write_to(&mut png_encoding, image::ImageFormat::Png)
        .map_err(|e| ImgOrFfmpegError::from(ImgOpsError::from(e)))?;

    Ok(png_encoding.len())
}
