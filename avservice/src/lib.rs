mod audio;
mod config;
mod displays;
mod error;
mod kymux;
mod rtp;
mod utils;
mod video;

use log::{error, info, warn};

use avservice_types::{
    Command, Event, PipelineEvent, PipelineEventType, PipelineStage, PipelineType, VideoCodec,
};
use libkypc::{EventSender, Worker};
use txproto_rs::Txproto;

pub use error::{Error, Result};
pub(crate) use video::{VideoConfig, VideoEncoder};

#[allow(unused)]
struct Context {
    events: Option<Vec<txproto_rs::Event>>,
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

pub async fn run(mut worker: Worker<Command, Event>) -> Result<()> {
    info!("AV service started");

    let (mut receiver, mut sender) = worker.get_sender_receiver()?;

    let mut _ctx = None;
    let config = config::load().await?;

    while let Ok(msg) = receiver.recv().await {
        match msg {
            (cmd_id, Command::EnumerateDisplays) => {
                let (c, display_list) = displays::enumerate(sender.clone())?;

                receiver
                    .accept(
                        cmd_id,
                        Some(Event::DisplayList {
                            displays: display_list,
                        }),
                    )
                    .await?;

                _ctx = Some(c);
            }
            (cmd_id, Command::RtpGenerateVideoSdp { display_id, yuv444 }) => {
                info!("Generate RTP Video SDP: yuv444: {yuv444}");
                let ret = rtp::generate_sdp(
                    display_id,
                    config.encoder,
                    VideoCodec::H264,
                    yuv444,
                    config.intra_refresh(),
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
                },
            ) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start RTP Video: {ip}:{port}, bitrate: {bitrate}, yuv444: {yuv444}");

                let video_config = VideoConfig {
                    display_id,
                    encoder: config.encoder,
                    bitrate,
                    codec: VideoCodec::H264,
                    yuv444,
                    intra_refresh: config.intra_refresh(),
                    encoder_config: config.video_encoder_config.clone(),
                };

                let ret = rtp::start_video(video_config, ip, port, sender.clone());
                match ret {
                    Ok(c) => _ctx = Some(c),
                    Err(err) => forward_init_error(&mut sender, err, PipelineType::Video).await,
                }
            }
            (cmd_id, Command::RtpStartAudio { ip, port }) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start RTP Audio: {ip}:{port}");

                let ret = rtp::start_audio(ip, port, sender.clone());
                match ret {
                    Ok(c) => _ctx = Some(c),
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
                },
            ) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start Kymux Video: URI {uri:?}, bitrate: {bitrate}, codec: {codec:?}, yuv444: {yuv444}");

                let video_config = VideoConfig {
                    display_id,
                    encoder: config.encoder,
                    bitrate,
                    codec,
                    yuv444,
                    intra_refresh: config.intra_refresh(),
                    encoder_config: config.video_encoder_config.clone(),
                };

                let ret = kymux::start_video(video_config, &uri, sender.clone());
                match ret {
                    Ok(c) => _ctx = Some(c),
                    Err(err) => forward_init_error(&mut sender, err, PipelineType::Video).await,
                }
            }
            (cmd_id, Command::KymuxStartAudio { uri }) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start Kymux Audio: URI {uri:?}");

                let ret = kymux::start_audio(&uri, sender.clone());
                match ret {
                    Ok(c) => _ctx = Some(c),
                    Err(err) => forward_init_error(&mut sender, err, PipelineType::Audio).await,
                }
            }
            (cmd_id, Command::Stop) => {
                receiver.accept(cmd_id, None).await?;

                _ctx = None;

                worker.stop().await?;
                break;
            }
        }
    }

    info!("AV service stopping");

    Ok(())
}
