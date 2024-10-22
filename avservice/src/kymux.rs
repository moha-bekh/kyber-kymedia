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
    ctx.set_epoch(0)?;

    let packet_sink = ctx.create_packet_sink(uri)?;
    let events = video::configure(&ctx, video_config, packet_sink, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        events: Some(events),
    })
}

pub(crate) fn start_audio(uri: &str, sender: EventSender<Event>) -> Result<Context> {
    let ctx = Txproto::new()?;
    ctx.set_epoch(0)?;

    let packet_sink = ctx.create_packet_sink(uri)?;
    let events = audio::configure(&ctx, packet_sink, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        events: Some(events),
    })
}
