use crate::{Error, Result};

use crate::Context;

pub(crate) async fn generate_sdp(display_id: u32, yuv444: bool) -> Result<String> {
    Err(Error::Unimplemented)
}

pub(crate) fn start_video(
    display_id: u32,
    ip: String,
    port: u16,
    bitrate: u32,
    yuv444: bool,
) -> Result<Context> {
    Err(Error::Unimplemented)
}

pub(crate) fn start_audio(ip: String, port: u16) -> Result<Context> {
    Err(Error::Unimplemented)
}
