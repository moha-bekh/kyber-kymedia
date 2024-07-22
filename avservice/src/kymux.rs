use libkypc::EventSender;
use txproto_rs::Txproto;

use crate::{audio, video};
use crate::{Context, Event, Result, VideoConfig};

fn set_epoch(ctx: &Txproto) -> Result<()> {
    const OFFSET_DAYS: i64 = 1;

    let offset_micro = OFFSET_DAYS * 24 * 60 * 60 * 1_000_000;
    ctx.set_epoch(-offset_micro)?;

    Ok(())
}

pub(crate) fn start_video(
    video_config: VideoConfig,
    uri: &str,
    sender: EventSender<Event>,
) -> Result<Context> {
    let ctx = Txproto::new()?;
    set_epoch(&ctx)?;

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
    set_epoch(&ctx)?;

    let packet_sink = ctx.create_packet_sink(uri)?;
    let events = audio::configure(&ctx, packet_sink, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        events: Some(events),
    })
}
