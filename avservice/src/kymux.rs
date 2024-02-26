use crate::{Error, Result};

use crate::{Context, VideoEncoder};

pub(crate) fn start_video(
    display_id: u32,
    uri: &str,
    encoder: VideoEncoder,
    bitrate: u32,
    codec: &str,
    yuv444: bool,
    intra_refresh: bool,
) -> Result<Context> {
    Err(Error::Unimplemented)
}

pub(crate) fn start_audio(uri: &str) -> Result<Context> {
    Err(Error::Unimplemented)
}
