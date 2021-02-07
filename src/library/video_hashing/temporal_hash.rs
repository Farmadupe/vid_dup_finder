use std::{
    cmp::min,
    hash::Hash,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::library::{
    concrete_cachers::ImgOrFfmpegError,
    definitions::{HASH_DISTANCE_SCALING_FACTOR, HASH_NUM_IMAGES},
    img_ops::RgbImgBuf,
    search_structures::ScaledTolerance,
};

const HASH_FRAME_QWORDS: usize =
    (crate::library::definitions::HASH_IMAGE_X * crate::library::definitions::HASH_IMAGE_Y) / 64;
const SPATIAL_HASH_QWORDS: usize = HASH_FRAME_QWORDS * crate::library::definitions::HASH_NUM_IMAGES;
const TEMPORAL_HASH_QWORDS: usize = HASH_FRAME_QWORDS * (crate::library::definitions::HASH_NUM_IMAGES - 1);

big_array! { BigArray; TEMPORAL_HASH_QWORDS as usize, SPATIAL_HASH_QWORDS as usize,}

#[derive(Serialize, Deserialize, PartialEq, Clone, Hash, Eq, Default, Ord, PartialOrd)]
pub struct TemporalHash {
    #[serde(with = "BigArray")]
    thash: [u64; TEMPORAL_HASH_QWORDS as usize],
    #[serde(with = "BigArray")]
    shash: [u64; SPATIAL_HASH_QWORDS as usize],
    num_frames: u32,
    src_path: PathBuf,
}

impl TemporalHash {
    pub fn new<P: AsRef<Path>>(
        src_path: P,
        spatial_hash: Vec<Vec<u64>>,
        temporal_hash: Vec<Vec<u64>>,
    ) -> Result<Self, HashCreationErrorKind> {
        let spatial_len = spatial_hash.len() as u32;

        if spatial_len < 2 {
            return Err(HashCreationErrorKind::VideoTooShortError(
                src_path.as_ref().to_path_buf(),
            ));
        }

        let flattened_spatial_hash = spatial_hash.into_iter().flatten().collect::<Vec<_>>();
        let mut spatial_arr: [u64; SPATIAL_HASH_QWORDS as usize] = [0; SPATIAL_HASH_QWORDS as usize];
        spatial_arr[..flattened_spatial_hash.len()].copy_from_slice(&flattened_spatial_hash);

        let flattened_temporal_hash = temporal_hash.into_iter().flatten().collect::<Vec<_>>();
        let mut temporal_arr: [u64; TEMPORAL_HASH_QWORDS as usize] = [0; TEMPORAL_HASH_QWORDS as usize];
        temporal_arr[..flattened_temporal_hash.len()].copy_from_slice(&flattened_temporal_hash);

        let ret = Self {
            thash: temporal_arr,
            shash: spatial_arr,
            num_frames: spatial_len,
            src_path: src_path.as_ref().to_owned(),
        };

        //raise an error if the hash is empty
        if ret.shash_is_all_zeroes() && ret.thash_is_all_zeroes() {
            return Err(HashCreationErrorKind::EmptyHashError(src_path.as_ref().to_path_buf()));
        }

        Ok(ret)
    }

    pub fn src_path(&self) -> &Path {
        &self.src_path
    }

    const fn calc_lut_entry(frame_no: u32, max_frames: u32) -> u32 {
        let f64_scaling = HASH_DISTANCE_SCALING_FACTOR as f64;
        let f64_frame_no = frame_no as f64;
        let f64_max_frames = max_frames as f64;

        let f64_ret = f64_scaling * (1.0 / (f64_frame_no / f64_max_frames));

        f64_ret as u32
    }

    const fn calc_temporal_lut() -> [u32; HASH_NUM_IMAGES as usize] {
        //note that the 0th entry of the LUT is never read. Its presence is to offset the start of the LUT to
        //eliminate the addition of a constant value (1) when indexing into it.
        let mut i: usize = 1;
        let mut ret: [u32; HASH_NUM_IMAGES as usize] = [0; HASH_NUM_IMAGES as usize];
        while i < HASH_NUM_IMAGES as usize {
            ret[i] = Self::calc_lut_entry(i as u32, HASH_NUM_IMAGES as u32);
            i += 1;
        }

        ret
    }
    const TEMPORAL_LUT: [u32; HASH_NUM_IMAGES as usize] = Self::calc_temporal_lut();

    pub fn temporal_distance(&self, other: &TemporalHash) -> u32 {
        //note: Each frame is 64 bits, and there is always 1 less temporal frame than spatial frames.
        // So num_qwords == num_frames - 1.
        let num_qwords = min(self.num_frames, other.num_frames) - 1;
        let raw_dist = raw_distance(&self.thash, &other.thash);

        //unsafe code to eliminate bounds check in the hottest hot path of the entire codebase.
        //TEMPORAL_LUT is HASH_NUM_IMAGES length, and num_qwords is the max of the number of
        //frames in the hash - 1, which is guaranteed upon creation to be within this range.
        //Seems to give a single digit percent performance bump.
        unsafe { raw_dist * Self::TEMPORAL_LUT.get_unchecked(num_qwords as usize) }
    }

    const fn calc_spatial_lut() -> [u32; (HASH_NUM_IMAGES + 1) as usize] {
        //note that the 0th entry of the LUT is never read. Its presence is to offset the start of the LUT to
        //eliminate the addition of a constant value (1) when indexing into it.
        let mut i: usize = 1;
        let mut ret: [u32; (HASH_NUM_IMAGES + 1) as usize] = [0; (HASH_NUM_IMAGES + 1) as usize];

        while i < (HASH_NUM_IMAGES + 1) as usize {
            ret[i] = Self::calc_lut_entry(i as u32, HASH_NUM_IMAGES as u32);
            i += 1;
        }

        ret
    }
    const SPATIAL_LUT: [u32; (HASH_NUM_IMAGES + 1) as usize] = Self::calc_spatial_lut();

    pub fn spatial_distance(&self, other: &TemporalHash) -> u32 {
        let num_qwords = min(self.num_frames, other.num_frames);
        let raw_dist = raw_distance(&self.shash, &other.shash);

        //unsafe code to eliminate bounds check in the hottest hot path of the entire codebase.
        //SPATIAL_LUT is (HASH_NUM_IMAGES + 1) length, and num_qwords is the max of the number of
        //frames in the hash, which is guaranteed upon creation to be within this range.
        //Seems to give a single digit percent performance bump.
        unsafe { raw_dist * Self::SPATIAL_LUT.get_unchecked(num_qwords as usize) }
    }

    pub fn hash_is_all_zeroes(&self) -> bool {
        self.thash_is_all_zeroes() && self.shash_is_all_zeroes()
    }

    pub fn thash_is_all_zeroes(&self) -> bool {
        self.thash.iter().all(|val| *val == 0)
    }

    pub fn shash_is_all_zeroes(&self) -> bool {
        self.shash.iter().all(|val| *val == 0)
    }

    pub fn distance(&self, other: &Self) -> Distance {
        Distance {
            spatial: self.spatial_distance(other),
            temporal: self.temporal_distance(other),
        }
    }

    pub fn spatial_thumbs(&self) -> Vec<RgbImgBuf> {
        (0..self.num_frames)
            .map(|frame_no| {
                let mut frame_bits = self.shash[frame_no as usize];

                let mut frame = RgbImgBuf::new(8, 8);
                for y in 0..8 {
                    for x in 0..8 {
                        if (frame_bits % 2) == 0 {
                            frame.get_pixel_mut(x, y).0 = [u8::MIN, u8::MIN, u8::MIN];
                        } else {
                            frame.get_pixel_mut(x, y).0 = [u8::MAX, u8::MAX, u8::MAX];
                        }
                        frame_bits = frame_bits.rotate_right(1);
                    }
                }

                frame
            })
            .collect()
    }

    pub fn reconstructed_thumbs(&self) -> Vec<RgbImgBuf> {
        (0..self.num_frames)
            .map(|frame_no| {
                let mut frame_bits = self.shash[frame_no as usize];
                let mut frame = vec![0f64; 64];

                for y in 0..8 {
                    for x in 0..8 {
                        *frame.get_mut(x * y).unwrap() = (frame_bits % 2) as f64;

                        frame_bits = frame_bits.rotate_left(1);
                    }
                }

                frame
            })
            .map(|x| crate::library::utils::dct_ops::inverse_dct(&x))
            .map(|dynamic_image| dynamic_image.to_rgb8())
            .collect()
    }

    pub fn temporal_thumbs(&self) -> Vec<RgbImgBuf> {
        (0..self.num_frames - 1)
            .map(|frame_no| {
                let mut frame_bits = self.thash[frame_no as usize];

                let mut frame = RgbImgBuf::new(8, 8);
                for y in 0..8 {
                    for x in 0..8 {
                        if (frame_bits % 2) == 0 {
                            frame.get_pixel_mut(x, y).0 = [u8::MIN, u8::MIN, u8::MIN];
                        } else {
                            frame.get_pixel_mut(x, y).0 = [u8::MAX, u8::MAX, u8::MAX];
                        }
                        frame_bits = frame_bits.rotate_right(1);
                    }
                }

                frame
            })
            .collect::<Vec<_>>()
    }
}

impl std::fmt::Debug for TemporalHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(&self.src_path, f)
    }
}

impl AsRef<TemporalHash> for TemporalHash {
    fn as_ref(&self) -> &TemporalHash {
        &self
    }
}

fn raw_distance<const N: usize>(x: &[u64; N], y: &[u64; N]) -> u32 {
    x.iter().zip(y.iter()).fold(0, |acc, (x, y)| {
        let difference = x ^ y;
        let set_bits = difference.count_ones();
        acc + set_bits
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Distance {
    pub temporal: u32,
    pub spatial: u32,
}

impl Distance {
    pub fn within_tolerance(&self, tolerance: ScaledTolerance) -> bool {
        self.spatial <= tolerance.spatial && self.temporal <= tolerance.temporal
    }
    pub const MAX_DISTANCE: Self = Self {
        spatial: u32::MAX,
        temporal: u32::MAX,
    };

    pub fn u32_value(&self) -> u32 {
        self.temporal + self.spatial
    }
}

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum HashCreationErrorKind {
    #[error("Video file is too short: {0}")]
    VideoTooShortError(PathBuf),

    #[error("image/ffmpeg error at {path}: {error}")]
    ImgOrFfmpegError { path: PathBuf, error: ImgOrFfmpegError },

    #[error("hash is empty: {0}")]
    EmptyHashError(PathBuf),
}
