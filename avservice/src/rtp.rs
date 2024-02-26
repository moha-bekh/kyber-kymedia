use crate::{Error, Result};

use crate::{Context, VideoCodec, VideoConfig, VideoEncoder};

pub(crate) async fn generate_sdp(
    display_id: u32,
    encoder: VideoEncoder,
    codec: VideoCodec,
    yuv444: bool,
    intra_refresh: bool,
) -> Result<String> {
    Err(Error::Unimplemented)
}

pub(crate) fn start_video(video_config: VideoConfig, ip: String, port: u16) -> Result<Context> {
    Err(Error::Unimplemented)
}

pub(crate) fn start_audio(ip: String, port: u16) -> Result<Context> {
    Err(Error::Unimplemented)
}
