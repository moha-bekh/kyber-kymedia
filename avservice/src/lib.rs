mod audio;
mod clock;
mod config;
mod displays;
mod error;
mod kymux;
mod metrics;
mod rtp;
mod utils;
mod video;

use log::{error, info, warn};

use avservice_types::{
    Command, Event, PipelineEvent, PipelineEventType, PipelineStage, PipelineType, VideoCodec,
};
use libkypc::{CommandReceiver, EventSender, Worker};
use txproto_rs::Txproto;

use crate::config::Config;
pub(crate) use clock::configure_clock;
pub use error::{Error, Result};
pub(crate) use video::{VideoConfig, VideoEncoder};

#[allow(unused)]
struct Context {
    events: Option<Vec<txproto_rs::Event>>,
    encoder: Option<txproto_rs::Handle>,
    ctx: Txproto,
}

async fn forward_init_error(sender: &mut EventSender<Event>, err: Error, type_: PipelineType) {
    error!("Failed to start {type_:?} pipeline: {err:?}");

    let ret = sender
        .send(Event::PipelineEvent(PipelineEvent {
            type_,
            stage: PipelineStage::Initialization,
            event: PipelineEventType::Error,
        }))
        .await;
    if let Err(err) = ret {
        warn!("Failed to send {type_:?} init error: {err:?}");
    }
}

async fn handle_video_set_bitrate(
    ctx: &mut Option<Context>,
    receiver: &mut CommandReceiver<Command, Event>,
    cmd_id: u32,
    bitrate: i32,
) -> Result<()> {
    info!("Set bitrate to {bitrate}");

    let Some(ctx) = &ctx else {
        info!("No session started");
        receiver.reject(cmd_id, None).await?;

        return Ok(());
    };

    let Some(encoder) = &ctx.encoder else {
        info!("No active encoder");
        receiver.reject(cmd_id, None).await?;

        return Ok(());
    };

    receiver.accept(cmd_id, None).await?;

    ctx.ctx.encoder_set_bitrate(encoder, bitrate)?;
    ctx.ctx.commit()?;

    Ok(())
}

async fn handle_video_force_idr(
    ctx: &mut Option<Context>,
    receiver: &mut CommandReceiver<Command, Event>,
    cmd_id: u32,
) -> Result<()> {
    info!("Video force IDR");

    let Some(ctx) = &ctx else {
        info!("No session started");
        receiver.reject(cmd_id, None).await?;

        return Ok(());
    };

    let Some(encoder) = &ctx.encoder else {
        info!("No active encoder");
        receiver.reject(cmd_id, None).await?;

        return Ok(());
    };

    receiver.accept(cmd_id, None).await?;

    ctx.ctx.encoder_force_idr(encoder)?;
    ctx.ctx.commit()?;

    Ok(())
}

fn ajust_min_bitrate(config: &Config, bitrate: u32) -> u32 {
    let min_bitrate = config.minimum_bitrate();
    if bitrate < min_bitrate {
        warn!("Bitrate too low. Reconfigure it: {bitrate} -> {min_bitrate}");
        min_bitrate
    } else {
        bitrate
    }
}

pub async fn run(mut worker: Worker<Command, Event>) -> Result<()> {
    info!("AV service started");

    let (mut receiver, mut sender) = worker.get_sender_receiver()?;

    let mut ctx = None;
    let config = config::load().await?;

    while let Ok(msg) = receiver.recv().await {
        match msg {
            (cmd_id, Command::EnumerateDisplays) => {
                info!("Start display enumeration");

                let (c, display_list) = displays::enumerate(sender.clone())?;

                receiver
                    .accept(
                        cmd_id,
                        Some(Event::DisplayList {
                            displays: display_list,
                        }),
                    )
                    .await?;

                ctx = Some(c);
            }
            (
                cmd_id,
                Command::RtpGenerateVideoSdp {
                    display_id,
                    yuv444,
                    intra_refresh,
                },
            ) => {
                info!("Generate RTP Video SDP: yuv444: {yuv444}, intra_refresh: {intra_refresh:?}");
                let ret = rtp::generate_sdp(
                    display_id,
                    config.encoder,
                    VideoCodec::H264,
                    yuv444,
                    intra_refresh.unwrap_or(config.intra_refresh()),
                    &config.video_encoder_config,
                )
                .await;

                match ret {
                    Ok(sdp) => {
                        receiver
                            .accept(cmd_id, Some(Event::RtpSdpGenerated { sdp }))
                            .await?;
                    }
                    Err(err) => {
                        error!("Failed to generate Video SDP: {err:?}");

                        receiver.reject(cmd_id, None).await?;
                    }
                }
            }
            (
                cmd_id,
                Command::RtpStartVideo {
                    display_id,
                    ip,
                    port,
                    bitrate,
                    yuv444,
                    intra_refresh,
                },
            ) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start RTP Video: {ip}:{port}, bitrate: {bitrate}, yuv444: {yuv444}, intra_refresh: {intra_refresh:?}");

                let video_config = VideoConfig {
                    display_id,
                    encoder: config.encoder,
                    bitrate: ajust_min_bitrate(&config, bitrate),
                    codec: VideoCodec::H264,
                    yuv444,
                    intra_refresh: intra_refresh.unwrap_or(config.intra_refresh()),
                    encoder_config: config.video_encoder_config.clone(),
                    clock_type: config.clock_type(),
                    watermark: None,
                };

                let ret = rtp::start_video(video_config, ip, port, sender.clone());
                match ret {
                    Ok(c) => ctx = Some(c),
                    Err(err) => forward_init_error(&mut sender, err, PipelineType::Video).await,
                }
            }
            (cmd_id, Command::RtpStartAudio { ip, port }) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start RTP Audio: {ip}:{port}");

                let ret = rtp::start_audio(ip, port, config.clock_type(), sender.clone());
                match ret {
                    Ok(c) => ctx = Some(c),
                    Err(err) => forward_init_error(&mut sender, err, PipelineType::Video).await,
                }
            }
            (
                cmd_id,
                Command::KymuxStartVideo {
                    display_id,
                    uri,
                    bitrate,
                    codec,
                    yuv444,
                    intra_refresh,
                    metrics_uri,
                },
            ) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start Kymux Video: URI {uri:?}, bitrate: {bitrate}, codec: {codec:?}, yuv444: {yuv444}, intra_refresh: {intra_refresh:?}");

                let video_config = VideoConfig {
                    display_id,
                    encoder: config.encoder,
                    bitrate: ajust_min_bitrate(&config, bitrate),
                    codec,
                    yuv444,
                    intra_refresh: intra_refresh.unwrap_or(config.intra_refresh()),
                    encoder_config: config.video_encoder_config.clone(),
                    clock_type: config.clock_type(),
                    watermark: config.watermark.clone(),
                };

                let ret =
                    kymux::start_video(video_config, &uri, metrics_uri.as_deref(), sender.clone());
                match ret {
                    Ok(c) => ctx = Some(c),
                    Err(err) => forward_init_error(&mut sender, err, PipelineType::Video).await,
                }
            }
            (cmd_id, Command::KymuxStartAudio { uri }) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start Kymux Audio: URI {uri:?}");

                let ret = kymux::start_audio(&uri, config.clock_type(), sender.clone());
                match ret {
                    Ok(c) => ctx = Some(c),
                    Err(err) => forward_init_error(&mut sender, err, PipelineType::Audio).await,
                }
            }
            (cmd_id, Command::VideoSetBitrate { bitrate }) => {
                handle_video_set_bitrate(&mut ctx, &mut receiver, cmd_id, bitrate).await?;
            }
            (cmd_id, Command::VideoForceIdr) => {
                handle_video_force_idr(&mut ctx, &mut receiver, cmd_id).await?;
            }
            (cmd_id, Command::Stop) => {
                receiver.accept(cmd_id, None).await?;

                drop(ctx);

                worker.stop().await?;
                break;
            }
        }
    }

    info!("AV service stopping");

    Ok(())
}
