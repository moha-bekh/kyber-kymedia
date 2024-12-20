use libkypc::EventSender;
use txproto_rs::Txproto;

use crate::{audio, video};
use crate::{Context, Event, Result, VideoConfig};

pub(crate) fn start_video(
    video_config: VideoConfig,
    uri: &str,
    sender: EventSender<Event>,
) -> Result<Context> {
    let ctx = Txproto::new()?;
    ctx.set_epoch_offset(0)?;

    let packet_sink = ctx.create_packet_sink(uri)?;
    let video_pipeline = video::configure(&ctx, video_config, packet_sink, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        encoder: video_pipeline.encoder,
        events: Some(video_pipeline.events),
    })
}

pub(crate) fn start_audio(uri: &str, sender: EventSender<Event>) -> Result<Context> {
    let ctx = Txproto::new()?;
    ctx.set_epoch_offset(0)?;

    let packet_sink = ctx.create_packet_sink(uri)?;
    let events = audio::configure(&ctx, packet_sink, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        encoder: None,
        events: Some(events),
    })
}
