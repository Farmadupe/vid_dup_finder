use image::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type RgbImgBuf = ImageBuffer<Rgb<u8>, Vec<u8>>;
pub type GrayImgBuf = ImageBuffer<Luma<u8>, Vec<u8>>;

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ImgOpsError {
    #[error("Image processing error")]
    ImgError(String),

    #[error("Internal error indexing an image")]
    IndexError,

    #[error("Failed to convert raw bytes received from FFMpeg into an image")]
    RawConversionError,
}

impl From<image::ImageError> for ImgOpsError {
    fn from(e: image::ImageError) -> Self {
        Self::ImgError(e.to_string())
    }
}

pub fn asiden_images(b1: &RgbImgBuf, b2: &RgbImgBuf) -> RgbImgBuf {
    //prepare a new buffer large enough to fit both images.
    //the width is the sum of the widths of the two images.
    //the depth is the max of the depths of the two images,
    let (b1_x, b1_y) = b1.dimensions();
    let (b2_x, b2_y) = b2.dimensions();
    let sum_x = b1_x + b2_x;
    let max_y = std::cmp::max(b1_y, b2_y);

    let mut sxs_buf: RgbImgBuf = ImageBuffer::new(sum_x, max_y);
    sxs_buf.copy_from(b1, 0, 0).unwrap();
    sxs_buf.copy_from(b2, b1_x, 0).unwrap();

    sxs_buf
}

pub fn stack_images(b1: &RgbImgBuf, b2: &RgbImgBuf) -> Result<RgbImgBuf, ImgOpsError> {
    //prepare a new buffer large enough to fit both images.
    //the width is the max of the widths of the two images.
    //the depth is the sum of the depths of the two images,
    let (b1_x, b1_y) = b1.dimensions();
    let (b2_x, b2_y) = b2.dimensions();
    let max_x = std::cmp::max(b1_x, b2_x);
    let sum_y = b1_y + b2_y;

    let mut stacked_buf: RgbImgBuf = ImageBuffer::new(max_x, sum_y);
    stacked_buf.copy_from(b1, 0, 0)?;
    stacked_buf.copy_from(b2, 0, b1_y)?;

    Ok(stacked_buf)
}

pub fn grid_images(images: Vec<Vec<RgbImgBuf>>) -> Result<RgbImgBuf, ImgOpsError> {
    let (img_x, img_y) = images
        .get(0)
        .ok_or(ImgOpsError::IndexError)?
        .get(0)
        .ok_or(ImgOpsError::IndexError)?
        .dimensions();
    let grid_num_x = images.iter().map(|i| i.len()).max().unwrap_or(0) as u32;
    let grid_num_y = images.len() as u32;

    let mut grid_buf: RgbImgBuf = ImageBuffer::new(grid_num_x * img_x, grid_num_y * img_y);

    for (col_no, row_imgs) in images.iter().enumerate() {
        for (row_no, img) in row_imgs.iter().enumerate() {
            let x_coord = row_no as u32 * img_x;
            let y_coord = col_no as u32 * img_y;
            grid_buf.copy_from(img as &RgbImgBuf, x_coord, y_coord)?;
        }
    }

    Ok(grid_buf)
}

pub fn row_images(images: Vec<&RgbImgBuf>) -> Result<RgbImgBuf, ImageError> {
    let (img_x, img_y) = images[0].dimensions();
    let row_num_images = images.len() as u32;

    let mut row_buf: RgbImgBuf = ImageBuffer::new(row_num_images * img_x, img_y);

    for (col_no, img) in images.iter().enumerate() {
        let x_coord = col_no as u32 * img_x;
        row_buf.copy_from(img as &RgbImgBuf, x_coord, 0)?;
    }

    Ok(row_buf)
}

// pub fn from_ffmpeg_frame(bytes: Vec<u8>, dimensions_x: u32, dimensions_y: u32) -> ImgBuf {
//     ImgBuf::from_raw(dimensions_x, dimensions_y, bytes).unwrap()
// }

// pub fn determine_one_crop(img: &ImgBuf) -> (u32, u32, u32, u32) {
//     let thresh = 10;

//     let raw = img.deref();
//     let (dim_x, dim_y) = img.dimensions();
//     let top_rows = raw.chunks(dim_x as usize * 3);
//     let top_crop = top_rows
//         .enumerate()
//         .find(|(_y, row)| row.iter().any(|&val| val > thresh))
//         .map(|(y, _row)| y)
//         .unwrap_or(0) as u32;

//     let bot_rows = raw.chunks(dim_x as usize * 3).rev();
//     let bot_crop = bot_rows
//         .enumerate()
//         .find(|(_y, row)| row.iter().any(|&val| val > thresh))
//         .map(|(y, _row)| y)
//         .unwrap_or(0) as u32;

//     let left_crop = (0..dim_x)
//         .find(|x| {
//             (0..dim_y).any(|y| {
//                 let pix = img[(*x, y)];

//                 pix.0[0] > thresh && pix.0[1] > thresh && pix.0[2] > thresh
//             })
//         })
//         .unwrap_or(0);

//     let right_crop = (dim_x - 1)
//         - (0..dim_x)
//             .rev()
//             .find(|x| {
//                 (0..dim_y).any(|y| {
//                     let pix = img[(*x, y)];

//                     pix.0[0] > thresh && pix.0[1] > thresh && pix.0[2] > thresh
//                 })
//             })
//             .unwrap_or(0);

//     (top_crop, bot_crop, left_crop, right_crop)
// }

// pub fn determine_crop(images: &[ImgBuf]) -> (u32, u32, u32, u32) {
//     let crops = images.iter().map(determine_one_crop).collect::<Vec<_>>();

//     let top_crop = crops.iter().map(|x| x.0).min().unwrap_or(0);
//     let bot_crop = crops.iter().map(|x| x.1).min().unwrap_or(0);
//     let left_crop = crops.iter().map(|x| x.2).min().unwrap_or(0);
//     let right_crop = crops.iter().map(|x| x.3).min().unwrap_or(0);

//     (top_crop, bot_crop, left_crop, right_crop)
// }

// pub fn crop_frame(image: ImgBuf, cropspec: (u32, u32, u32, u32)) -> ImgBuf {
//     let (dim_x, dim_y) = image.dimensions();
//     image
//         .view(
//             cropspec.2,
//             cropspec.0,
//             dim_x - (cropspec.2 + cropspec.3),
//             dim_y - (cropspec.0 + cropspec.1),
//         )
//         .to_image()
// }

// pub fn crop_seq(images: Vec<ImgBuf>, cropspec: (u32, u32, u32, u32)) -> Vec<ImgBuf> {
//     images.into_iter().map(|image| crop_frame(image, cropspec)).collect()
// }
