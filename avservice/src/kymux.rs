use txproto_rs::Txproto;

use crate::{audio, video};
use crate::{Context, Result, VideoConfig};

pub(crate) fn start_video(video_config: VideoConfig, uri: &str) -> Result<Context> {
    let ctx = Txproto::new()?;
    ctx.set_epoch(0)?;

    let packet_sink = ctx.create_packet_sink(uri)?;
    video::configure(&ctx, video_config, packet_sink)?;

    ctx.commit()?;

    Ok(Context { ctx, events: None })
}

pub(crate) fn start_audio(uri: &str) -> Result<Context> {
    let ctx = Txproto::new()?;
    ctx.set_epoch(0)?;

    let packet_sink = ctx.create_packet_sink(uri)?;
    audio::configure(&ctx, packet_sink)?;

    ctx.commit()?;

    Ok(Context { ctx, events: None })
}
