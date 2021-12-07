use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use ffmpeg_cmdline_utils::*;
use gdk_pixbuf::Pixbuf;
use image::{imageops::resize, RgbImage};
use rayon::prelude::*;
use vid_dup_finder_lib::*;

use super::{gui_zoom::ZoomState, img_ops::*};
use crate::app::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbChoice {
    Video,
    CropdetectVideo,
    Spatial,
    Temporal,
    Rebuilt,
}

#[derive(Debug)]
struct ThumbRow {
    thumbs: Vec<RgbImage>,
}

impl ThumbRow {
    pub fn video_from_filename(src_path: &Path) -> Self {
        let thumbs_10sec =
            ffmpeg_cmdline_utils::FfmpegFrameReaderBuilder::new(src_path.to_path_buf())
                .num_frames(7)
                .fps("1/10")
                .spawn()
                .ok()
                .and_then(|(frames_iter, _stats)| {
                    let frames_vec = frames_iter.collect::<Vec<_>>();
                    if frames_vec.len() < 5 {
                        None
                    } else {
                        Some(frames_vec)
                    }
                });

        if let Some(thumbs) = thumbs_10sec {
            return Self { thumbs };
        }

        // if that didn't work then maybe it's because the video is too short for a 1/30 second
        // framerate, so try again with 1/5 second framerate instead.
        let thumbs_5sec =
            ffmpeg_cmdline_utils::FfmpegFrameReaderBuilder::new(src_path.to_path_buf())
                .num_frames(7)
                .fps("1/5")
                .spawn()
                .ok()
                .and_then(|(frames_iter, _stats)| {
                    let frames_vec = frames_iter.collect::<Vec<_>>();
                    if frames_vec.is_empty() {
                        None
                    } else {
                        Some(frames_vec)
                    }
                });

        if let Some(thumbs) = thumbs_5sec {
            return Self { thumbs };
        }

        // try 0.5 second interval.
        let thumbs_halfsec =
            ffmpeg_cmdline_utils::FfmpegFrameReaderBuilder::new(src_path.to_path_buf())
                .num_frames(7)
                .fps("2")
                .spawn()
                .ok()
                .and_then(|(frames_iter, _stats)| {
                    let frames_vec = frames_iter.collect::<Vec<_>>();
                    if frames_vec.is_empty() {
                        None
                    } else {
                        Some(frames_vec)
                    }
                });

        if let Some(thumbs) = thumbs_halfsec {
            return Self { thumbs };
        }

        //otherwise, give up and return the fallback images (black square)
        Self {
            thumbs: Self::fallback_images(),
        }
    }

    pub fn rebuilt_from_hash(hash: &VideoHash) -> Self {
        Self {
            thumbs: hash.reconstructed_thumbs(),
        }
    }

    pub fn spatial_from_hash(hash: &VideoHash) -> Self {
        Self {
            thumbs: hash.spatial_thumbs(),
        }
    }

    pub fn temporal_from_hash(hash: &VideoHash) -> Self {
        Self {
            thumbs: hash.temporal_thumbs(),
        }
    }

    pub fn zoom(&self, zoom: ZoomState) -> RgbImage {
        use gui::gui_zoom::ZoomValue::*;
        match zoom.get() {
            User(size) => {
                let resized = self
                    .thumbs
                    .par_iter()
                    .map(|thumb| resize(thumb, size, size, image::imageops::FilterType::Nearest))
                    .collect::<Vec<_>>();

                row_images(resized.iter().collect())
            }
            Native => row_images(self.thumbs.iter().collect()),
        }
    }

    //if an error occurs while generating thumbs, supply a default image as a placeholder
    fn fallback_images() -> Vec<RgbImage> {
        vec![RgbImage::new(100, 100), RgbImage::new(100, 100)]
    }

    fn without_letterbox(&self) -> ThumbRow {
        Self {
            thumbs: VideoFrames::from_images(&self.thumbs)
                .without_letterbox()
                .into_inner(),
        }
    }
}

#[derive(Debug)]
struct GuiThumbnail {
    filename: PathBuf,
    hash: VideoHash,

    base_video: Option<ThumbRow>,
    base_cropdetect: Option<ThumbRow>,
    spatial: Option<ThumbRow>,
    temporal: Option<ThumbRow>,
    rebuilt: Option<ThumbRow>,

    resized_thumb: Option<RgbImage>,

    zoom: ZoomState,
    choice: ThumbChoice,

    rendered_zoom: Option<ZoomState>,
    rendered_choice: Option<ThumbChoice>,
}

impl GuiThumbnail {
    pub fn new(filename: &Path, hash: VideoHash, zoom: ZoomState, choice: ThumbChoice) -> Self {
        Self {
            filename: filename.to_path_buf(),

            hash,

            base_video: None,
            base_cropdetect: None,
            spatial: None,
            temporal: None,
            resized_thumb: None,
            rebuilt: None,

            zoom,
            choice,

            rendered_zoom: None,
            rendered_choice: None,
        }
    }

    pub fn get(&mut self) -> RgbImage {
        let should_rerender = self.rendered_zoom.is_none()
            || self.rendered_choice.is_none()
            || self.rendered_zoom.unwrap() != self.zoom
            || self.rendered_choice.unwrap() != self.choice;

        self.rendered_zoom = Some(self.zoom);
        self.rendered_choice = Some(self.choice);

        if !should_rerender {
            return self.resized_thumb.as_ref().unwrap().clone();
        }

        match self.choice {
            ThumbChoice::Video => {
                if self.base_video.is_none() {
                    self.base_video = Some(ThumbRow::video_from_filename(&self.filename))
                }
            }
            ThumbChoice::CropdetectVideo => {
                if self.base_video.is_none() {
                    self.base_video = Some(ThumbRow::video_from_filename(&self.filename))
                }

                if self.base_cropdetect.is_none() {
                    self.base_cropdetect =
                        Some(self.base_video.as_ref().unwrap().without_letterbox())
                }
            }
            ThumbChoice::Spatial => {
                if self.spatial.is_none() {
                    self.spatial = Some(ThumbRow::spatial_from_hash(&self.hash))
                }
            }
            ThumbChoice::Temporal => {
                if self.temporal.is_none() {
                    self.temporal = Some(ThumbRow::temporal_from_hash(&self.hash))
                }
            }
            ThumbChoice::Rebuilt => {
                if self.spatial.is_none() {
                    self.rebuilt = Some(ThumbRow::rebuilt_from_hash(&self.hash))
                }
            }
        }

        match self.choice {
            ThumbChoice::Video => {
                self.resized_thumb = Some(self.base_video.as_ref().unwrap().zoom(self.zoom));
            }
            ThumbChoice::CropdetectVideo => {
                self.resized_thumb = Some(self.base_cropdetect.as_ref().unwrap().zoom(self.zoom));
            }
            ThumbChoice::Spatial => {
                self.resized_thumb = Some(self.spatial.as_ref().unwrap().zoom(self.zoom));
            }
            ThumbChoice::Temporal => {
                self.resized_thumb = Some(self.temporal.as_ref().unwrap().zoom(self.zoom));
            }
            ThumbChoice::Rebuilt => {
                self.resized_thumb = Some(self.rebuilt.as_ref().unwrap().zoom(self.zoom));
            }
        }

        self.resized_thumb.as_ref().unwrap().clone()
    }

    pub fn set_zoom(&mut self, zoom: ZoomState) {
        self.zoom = zoom;
    }

    pub fn set_choice(&mut self, choice: ThumbChoice) {
        self.choice = choice;
    }

    // fn zoom_thumb(base_thumb: &ImgBuf, num_frames: u32, zoom: ZoomState) -> ImgBuf {
    //     //debug!("resizing to {:?}", zoom);
    //     match zoom.get() {
    //         User(size) => resize(
    //             base_thumb,
    //             size * num_frames,
    //             size,
    //             image::imageops::FilterType::Nearest,
    //         ),
    //         Native => base_thumb.clone(),
    //     }
    // }
}

#[derive(Debug)]
pub struct GuiThumbnailSet {
    thumbs: HashMap<PathBuf, GuiThumbnail>,
}

impl GuiThumbnailSet {
    pub fn new(info: Vec<(&Path, VideoHash)>, zoom: ZoomState, choice: ThumbChoice) -> Self {
        let mut thumbs = HashMap::new();
        info.into_par_iter()
            .map(|(src_path, hash)| {
                (
                    src_path.to_path_buf(),
                    GuiThumbnail::new(src_path, hash, zoom, choice),
                )
            })
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|(src_path, thumb)| {
                thumbs.insert(src_path, thumb);
            });

        Self { thumbs }
    }

    pub fn set_zoom(&mut self, val: ZoomState) {
        self.thumbs
            .par_iter_mut()
            .for_each(|(_src_path, thumb)| thumb.set_zoom(val))
    }

    pub fn set_choice(&mut self, val: ThumbChoice) {
        self.thumbs
            .par_iter_mut()
            .for_each(|(_src_path, thumb)| thumb.set_choice(val))
    }

    pub fn get_pixbufs(&mut self) -> HashMap<PathBuf, Pixbuf> {
        let mut ret = HashMap::new();
        for (src_path, thumb) in self.thumbs.iter_mut() {
            let x = thumb.get();
            ret.insert(src_path.clone(), Self::image_to_gdk_pixbuf(x));
        }

        ret
    }

    fn image_to_gdk_pixbuf(img: RgbImage) -> Pixbuf {
        let (width, height) = img.dimensions();
        let bytes = glib::Bytes::from_owned(img.into_raw());

        Pixbuf::from_bytes(
            &bytes,
            gdk_pixbuf::Colorspace::Rgb,
            false,
            8,
            width as i32,
            height as i32,
            width as i32 * 3,
        )
    }
}
