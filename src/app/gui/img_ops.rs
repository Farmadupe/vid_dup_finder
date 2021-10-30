use image::*;

pub fn row_images(images: Vec<&RgbImage>) -> RgbImage {
    let (img_x, img_y) = images[0].dimensions();
    let row_num_images = images.len() as u32;

    let mut row_buf: RgbImage = ImageBuffer::new(row_num_images * img_x, img_y);

    for (col_no, img) in images.iter().enumerate() {
        let x_coord = col_no as u32 * img_x;
        row_buf.copy_from(img as &RgbImage, x_coord, 0).unwrap();
    }

    row_buf
}
