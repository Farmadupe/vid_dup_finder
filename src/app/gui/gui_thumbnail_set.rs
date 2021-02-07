use std::{
    collections::HashMap,
    error::Error,
    path::{Path, PathBuf},
};

use gdk_pixbuf::Pixbuf;
use image::imageops::resize;
use rayon::prelude::*;

use crate::{
    app::gui::gui_zoom::{ZoomState, ZoomValue::*},
    library::{
        concrete_cachers::ImgOrFfmpegError,
        ffmpeg_ops::create_images_into_memory,
        img_ops::{row_images, RgbImgBuf},
        FfmpegCfg, TemporalHash, VideoStats,
    },
};

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
    thumbs: Vec<RgbImgBuf>,
}

impl ThumbRow {
    pub fn video_from_filename(filename: &Path, native_x: u32, native_y: u32, choice: ThumbChoice) -> Self {
        let mut thumb_cfg: FfmpegCfg = FfmpegCfg {
            dimensions_x: 0, //unused with create_thumbnails_unscaled
            dimensions_y: 0, //unused with create_thumbnails_unscaled
            num_frames: 10,
            framerate: "1/3".to_string(),
            cropdetect: choice == ThumbChoice::CropdetectVideo,
        };

        thumb_cfg.dimensions_x = native_x;
        thumb_cfg.dimensions_y = native_y;

        let thumbs = {
            let frames = create_images_into_memory(filename, &thumb_cfg).map_err(ImgOrFfmpegError::from);
            let thumbs = frames.map(|frames| {
                frames
                    .resize(thumb_cfg.dimensions_x, thumb_cfg.dimensions_y)
                    .into_inner()
            });
            thumbs.unwrap_or_else(|_| Self::fallback_images())
        };

        Self { thumbs }
    }

    pub fn rebuilt_from_hash(hash: &TemporalHash) -> Self {
        Self {
            thumbs: hash.reconstructed_thumbs(),
        }
    }

    pub fn spatial_from_hash(hash: &TemporalHash) -> Self {
        Self {
            thumbs: hash.spatial_thumbs(),
        }
    }

    pub fn temporal_from_hash(hash: &TemporalHash) -> Self {
        Self {
            thumbs: hash.temporal_thumbs(),
        }
    }

    pub fn zoom(&self, zoom: ZoomState) -> RgbImgBuf {
        match zoom.get() {
            User(size) => {
                let resized = self
                    .thumbs
                    .par_iter()
                    .map(|thumb| resize(thumb, size, size, image::imageops::FilterType::Nearest))
                    .collect::<Vec<_>>();

                row_images(resized.iter().collect()).unwrap_or_else(Self::fallback_image)
            }
            Native => row_images(self.thumbs.iter().collect()).unwrap_or_else(Self::fallback_image),
        }
    }

    fn fallback_image(_unused_error: impl Error) -> RgbImgBuf {
        //if an error occurs while generating thumbs, supply a default image as a placeholder

        RgbImgBuf::new(100, 100)
    }

    fn fallback_images() -> Vec<RgbImgBuf> {
        //if an error occurs while generating thumbs, supply a default image as a placeholder

        vec![RgbImgBuf::new(100, 100), RgbImgBuf::new(100, 100)]
    }
}

#[derive(Debug)]
struct GuiThumbnail {
    filename: PathBuf,
    hash: TemporalHash,
    native_x: u32,
    native_y: u32,

    base_video: Option<ThumbRow>,
    base_cropdetect: Option<ThumbRow>,
    spatial: Option<ThumbRow>,
    temporal: Option<ThumbRow>,
    rebuilt: Option<ThumbRow>,

    resized_thumb: Option<RgbImgBuf>,

    zoom: ZoomState,
    choice: ThumbChoice,

    rendered_zoom: Option<ZoomState>,
    rendered_choice: Option<ThumbChoice>,
}

impl GuiThumbnail {
    pub fn new(
        filename: &Path,
        hash: TemporalHash,
        native_x: u32,
        native_y: u32,
        zoom: ZoomState,
        choice: ThumbChoice,
    ) -> Self {
        Self {
            filename: filename.to_path_buf(),

            hash,
            native_x,
            native_y,

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

    pub fn get(&mut self) -> RgbImgBuf {
        #[allow(clippy::nonminimal_bool)]
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
                    self.base_video = Some(ThumbRow::video_from_filename(
                        &self.filename,
                        self.native_x,
                        self.native_y,
                        ThumbChoice::Video,
                    ))
                }
            }
            ThumbChoice::CropdetectVideo => {
                if self.base_cropdetect.is_none() {
                    self.base_cropdetect = Some(ThumbRow::video_from_filename(
                        &self.filename,
                        self.native_x,
                        self.native_y,
                        ThumbChoice::CropdetectVideo,
                    ))
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
    pub fn new(info: Vec<(&Path, VideoStats, TemporalHash)>, zoom: ZoomState, choice: ThumbChoice) -> Self {
        let mut thumbs = HashMap::new();
        info.into_par_iter()
            .map(|(src_path, stats, hash)| {
                (
                    src_path.to_path_buf(),
                    GuiThumbnail::new(src_path, hash, stats.resolution.0, stats.resolution.1, zoom, choice),
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

    fn image_to_gdk_pixbuf(img: RgbImgBuf) -> Pixbuf {
        let (width, height) = img.dimensions();
        let bytes = glib::Bytes::from_owned(img.into_raw());
        let gdk_pixels = Pixbuf::from_bytes(
            &bytes,
            gdk_pixbuf::Colorspace::Rgb,
            false,
            8,
            width as i32,
            height as i32,
            width as i32 * 3,
        );

        gdk_pixels
    }
}
