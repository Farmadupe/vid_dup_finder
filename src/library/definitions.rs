use crate::library::Tolerance;

// Frame definitions (pre hashing)
pub const RESIZE_IMAGE_X: usize = 32;
pub const RESIZE_IMAGE_Y: usize = 32;

// Hash definitions
pub const HASH_NUM_IMAGES: usize = 10;
pub const HASH_IMAGE_X: usize = 8;
pub const HASH_IMAGE_Y: usize = 8;
pub const HASH_FRAMERATE: &str = "1/3";

// To avoid floating point operations, we will scale numbers for
// distance calculations by this.
pub const HASH_DISTANCE_SCALING_FACTOR: u32 = 1 << 6;

//At user-level the tolerance parameter is specified as real between 0 and 1.
//The is the scaling factor to map into the integer-domain being used for calculations.
pub const TOLERANCE_SCALING_FACTOR: f64 =
    (HASH_IMAGE_X * HASH_IMAGE_Y * HASH_DISTANCE_SCALING_FACTOR as usize * HASH_NUM_IMAGES) as f64;

pub const DEFAULT_TOLERANCE: Tolerance = Tolerance {
    spatial: 0.05,
    temporal: 0.05,
};
