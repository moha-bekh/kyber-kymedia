use log::info;

use libkypc::EventSender;
use txproto_rs::Txproto;

use crate::config::ClockType;
use crate::metrics;
use crate::{audio, configure_clock, video};
use crate::{Context, Event, Result, VideoConfig};

pub(crate) fn start_video(
    video_config: VideoConfig,
    uri: &str,
    metrics_uri: Option<&str>,
    sender: EventSender<Event>,
) -> Result<Context> {
    let ctx = Txproto::new()?;

    info!("Start video, using clock {:?}", video_config.clock_type);
    configure_clock(&ctx, &video_config.clock_type)?;

    if let Some(metrics_uri) = metrics_uri {
        metrics::forward(&ctx, metrics_uri)?;
    }

    let packet_sink = ctx.create_packet_sink(uri)?;
    let video_pipeline = video::configure(&ctx, video_config, packet_sink, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        encoder: video_pipeline.encoder,
        events: Some(video_pipeline.events),
    })
}

pub(crate) fn start_audio(
    uri: &str,
    clock_type: ClockType,
    sender: EventSender<Event>,
) -> Result<Context> {
    let ctx = Txproto::new()?;

    info!("Start audio, using clock {:?}", clock_type);
    configure_clock(&ctx, &clock_type)?;

    let packet_sink = ctx.create_packet_sink(uri)?;
    let events = audio::configure(&ctx, packet_sink, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        encoder: None,
        events: Some(events),
    })
}
