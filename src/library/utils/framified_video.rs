use std::path::{Path, PathBuf};

use image::imageops::{resize, FilterType::Lanczos3};

use super::img_ops::{grid_images, GrayImgBuf, RgbImgBuf};
use crate::library::concrete_cachers::ImgOrFfmpegError;
// #[derive(Debug, Clone)]
// pub struct ResizedVideoFrame(pub Vec<u8>);

#[derive(Debug, Clone)]
pub struct FramifiedVideo {
    name: PathBuf,
    frames: Vec<RgbImgBuf>,
}

impl FramifiedVideo {
    // pub fn try_from_filesystem(file_path: &Path, cfg: &FfmpegCfg) -> Result<Self, ImgOrFfmpegError> {
    //     let images = create_images_into_memory(file_path, &cfg)?;

    //     let frames = images_grey;
    //     Ok(FramifiedVideo {
    //         name: file_path.to_owned(),
    //         frames,
    //     })
    // }

    pub fn new(file_path: &Path, images: Vec<RgbImgBuf>) -> Self {
        Self {
            name: file_path.to_path_buf(),
            frames: images,
        }
    }

    pub fn name(&self) -> &Path {
        &self.name
    }

    pub fn resize(&self, width: u32, height: u32) -> Self {
        let resized_frames = self
            .frames
            .iter()
            .map(|frame| resize(frame, width, height, Lanczos3))
            .collect();

        Self {
            name: self.name.to_path_buf(),
            frames: resized_frames,
        }
    }

    pub fn to_asidened_image(&self) -> Result<RgbImgBuf, ImgOrFfmpegError> {
        let image_grid = vec![self.frames.clone()];

        let asidened = grid_images(image_grid).map_err(ImgOrFfmpegError::Img)?;

        Ok(asidened)
    }

    pub fn into_inner(self) -> Vec<RgbImgBuf> {
        self.frames
    }
}

pub struct GrayFramifiedVideo {
    name: PathBuf,
    frames: Vec<GrayImgBuf>,
}

impl From<FramifiedVideo> for GrayFramifiedVideo {
    fn from(rgb: FramifiedVideo) -> Self {
        let images_gray = rgb
            .frames
            .into_iter()
            .map(|img| {
                let grey_buf: GrayImgBuf = image::buffer::ConvertBuffer::convert(&img);
                grey_buf
            })
            .collect::<Vec<_>>();

        Self {
            name: rgb.name,
            frames: images_gray,
        }
    }
}

impl GrayFramifiedVideo {
    pub fn into_inner(self) -> Vec<GrayImgBuf> {
        self.frames
    }
}
