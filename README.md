# Video Duplicate Finder
Video Duplicate Finder is a tool to search for duplicate and near-duplicate video files. It is capable of detecting duplicates even when the videos have been:
 * Resized (including changes of aspect ratio)
 * Watermarked
 * Letterboxed
 * Cropped (TBD: Quantify amount of cropping that can be detected)


Video duplicate finder contains:
* A Rust library for detecting duplicates.
* An optional command line program for listing unique/dupliacte files in a filesystem.
* An optional GUI (written in GTK) to allow users to examine duplicates and mark them for deletion (currently Linux-only)


## How it works
Video Duplicate finder extracts several frames from the first minute of each video. It creates a "perceptual hash" from these frames using 'Spatial' and 'Temporal' information from those frames:
* The spatial component describes the parts of each frame that are bright and dark. It is generated using the pHash algorithm described in [here](http://hackerfactor.com/blog/index.php%3F/archives/432-Looks-Like-It.html)
* The temporal component describes the parts of each frame that are brighter/darker than the previous frame. (It is calculated directly from the bits of the spatial hash)

The resulting hashes can then be compared according to their hamming distance. Shorter distances represent similar videos.
 

## Requirements
Ffmpeg must be installed on your system and be accessible on the command line.


## Speed
Excluding generation of hashes (which is dependent on the speed at which video files can be decoded), Video Duplicate Finder can deduplicate a set of 50,000 videos in under 3 seconds using a single CPU core.



## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

