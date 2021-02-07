use crate::library::{utils::framified_video::FramifiedVideo, *};

pub fn load(video_frames: &FramifiedVideo) -> Result<TemporalHash, HashCreationErrorKind> {
    let file_path = &video_frames.name();
    dct_hasher::video_dct_hash(&file_path, &video_frames)
}
