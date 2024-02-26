use crate::{Error, Result};

use crate::{Context, VideoConfig};

pub(crate) fn start_video(video_config: VideoConfig, uri: &str) -> Result<Context> {
    Err(Error::Unimplemented)
}

pub(crate) fn start_audio(uri: &str) -> Result<Context> {
    Err(Error::Unimplemented)
}
