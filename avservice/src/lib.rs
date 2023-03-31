mod config;
mod displays;
mod error;
mod kymux;
mod rtp;
mod utils;

use log::info;

use avservice_types::{Command, Event, VideoCodec};
use libkypc::Worker;

pub use error::{Error, Result};

pub async fn run(mut worker: Worker<Command, Event>) -> Result<()> {
    info!("AV service started");

    let (mut receiver, sender) = worker.get_sender_receiver()?;

    let mut ctx = None;
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

                ctx = Some(c);
            }
            (cmd_id, Command::RtpGenerateVideoSdp { yuv444 }) => {
                info!("Generate RTP Video SDP: yuv444: {yuv444}");
                let sdp = rtp::generate_sdp(yuv444).await?;

                receiver
                    .accept(cmd_id, Some(Event::RtpSdpGenerated { sdp }))
                    .await?;
            }
            (cmd_id, Command::RtpStartVideo { ip, port, yuv444 }) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start RTP Video: {ip}:{port} yuv444: {yuv444}");

                let c = rtp::start_video(ip, port, yuv444)?;
                ctx = Some(c);
            }
            (cmd_id, Command::RtpStartAudio { ip, port }) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start RTP Audio: {ip}:{port}");

                let c = rtp::start_audio(ip, port)?;
                ctx = Some(c);
            }
            (cmd_id, Command::KymuxStartVideo { uri, codec, yuv444 }) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start Kymux Video: URI {uri:?}, codec: {codec:?}, yuv444: {yuv444}");

                let codec = match codec {
                    VideoCodec::H264 => "h264",
                    VideoCodec::HEVC => "hevc",
                    VideoCodec::AV1 => "av1",
                };

                let c = kymux::start_video(&uri, &config.encoder, codec, yuv444)?;
                ctx = Some(c);
            }
            (cmd_id, Command::KymuxStartAudio { uri }) => {
                receiver.accept(cmd_id, None).await?;
                info!("Start Kymux Audio: URI {uri:?}");

                let c = kymux::start_audio(&uri)?;
                ctx = Some(c);
            }
            (cmd_id, Command::Stop) => {
                receiver.accept(cmd_id, None).await?;

                if let Some(ctx) = ctx.take() {
                    ctx.stop();
                }

                worker.stop().await?;
                break;
            }
        }
    }

    info!("AV service stopping");

    Ok(())
}
