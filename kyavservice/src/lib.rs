// Project Kyber: lib.rs
// Copyright © 2022-2026 Kyber SAS
// SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0-or-later
//
// This file is both under dual license: AGPLv3 and a Commercial one.
//
// ----
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

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

use kyavservice_types::{
    Command, Event, IpcApi, PipelineEvent, PipelineEventType, PipelineStage, PipelineType,
    Response, VideoCodec,
};
use libkypc::{EventSender, Worker};
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

fn forward_init_error(sender: &EventSender<IpcApi>, err: Error, type_: PipelineType) {
    error!("Failed to start {type_:?} pipeline: {err:?}");

    sender.send(Event::PipelineEvent(PipelineEvent {
        type_,
        stage: PipelineStage::Initialization,
        event: PipelineEventType::Error,
    }));
}

fn handle_video_set_bitrate(ctx: &mut Option<Context>, bitrate: i32) -> Result<()> {
    info!("Set bitrate to {bitrate}");

    let ctx = ctx
        .as_ref()
        .ok_or_else(|| std::io::Error::other("No session started"))?;

    let encoder = ctx
        .encoder
        .as_ref()
        .ok_or_else(|| std::io::Error::other("No active encoder"))?;

    ctx.ctx.encoder_set_bitrate(encoder, bitrate)?;
    ctx.ctx.commit()?;

    Ok(())
}

fn handle_video_force_idr(ctx: &mut Option<Context>) -> Result<()> {
    info!("Video force IDR");

    let ctx = ctx
        .as_ref()
        .ok_or_else(|| std::io::Error::other("No session started"))?;

    let encoder = ctx
        .encoder
        .as_ref()
        .ok_or_else(|| std::io::Error::other("No active encoder"))?;

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

async fn handle_command(
    cmd: Command,
    sender: &EventSender<IpcApi>,
    stop: &mut bool,
    ctx: &mut Option<Context>,
    config: &Config,
) -> Result<Response> {
    match cmd {
        Command::EnumerateDisplays => {
            info!("Start display enumeration");

            #[cfg(target_os = "linux")]
            let api_list = video::grab_backend_api_list(config.grab_backend());
            #[cfg(not(target_os = "linux"))]
            let api_list: Vec<&str> = vec![];

            let (c, display_list) = displays::enumerate(sender, &api_list)?;

            *ctx = Some(c);

            return Ok(Response::DisplayList {
                displays: display_list,
            });
        }
        Command::RtpGenerateVideoSdp {
            display_id,
            yuv444,
            intra_refresh,
        } => {
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
                Ok(sdp) => return Ok(Response::RtpSdpGenerated { sdp }),
                Err(err) => {
                    error!("Failed to generate Video SDP: {err:?}");
                }
            }
        }
        Command::RtpStartVideo {
            display_id,
            ip,
            port,
            bitrate,
            yuv444,
            intra_refresh,
        } => {
            info!(
                "Start RTP Video: {ip}:{port}, bitrate: {bitrate}, yuv444: {yuv444}, intra_refresh: {intra_refresh:?}"
            );

            let video_config = VideoConfig {
                display_id,
                encoder: config.encoder,
                bitrate: ajust_min_bitrate(config, bitrate),
                codec: VideoCodec::H264,
                yuv444,
                intra_refresh: intra_refresh.unwrap_or(config.intra_refresh()),
                encoder_config: config.video_encoder_config.clone(),
                clock_type: config.clock_type(),
                watermark: None,
                #[cfg(target_os = "linux")]
                grab_backend: config.grab_backend(),
            };

            let ret = rtp::start_video(video_config, ip, port, sender);
            match ret {
                Ok(c) => *ctx = Some(c),
                Err(err) => forward_init_error(sender, err, PipelineType::Video),
            }
        }
        Command::RtpStartAudio { ip, port } => {
            info!("Start RTP Audio: {ip}:{port}");

            let ret = rtp::start_audio(ip, port, config.clock_type(), sender);
            match ret {
                Ok(c) => *ctx = Some(c),
                Err(err) => forward_init_error(sender, err, PipelineType::Audio),
            }
        }
        Command::KymuxStartVideo {
            display_id,
            uri,
            bitrate,
            codec,
            yuv444,
            intra_refresh,
            metrics_uri,
        } => {
            info!(
                "Start Kymux Video: URI {uri:?}, bitrate: {bitrate}, codec: {codec:?}, yuv444: {yuv444}, intra_refresh: {intra_refresh:?}"
            );

            let video_config = VideoConfig {
                display_id,
                encoder: config.encoder,
                bitrate: ajust_min_bitrate(config, bitrate),
                codec,
                yuv444,
                intra_refresh: intra_refresh.unwrap_or(config.intra_refresh()),
                encoder_config: config.video_encoder_config.clone(),
                clock_type: config.clock_type(),
                watermark: config.watermark.clone(),
                #[cfg(target_os = "linux")]
                grab_backend: config.grab_backend(),
            };

            let ret = kymux::start_video(video_config, &uri, metrics_uri.as_deref(), sender);
            match ret {
                Ok(c) => *ctx = Some(c),
                Err(err) => forward_init_error(sender, err, PipelineType::Video),
            }
        }
        Command::KymuxStartAudio { uri } => {
            info!("Start Kymux Audio: URI {uri:?}");

            let ret = kymux::start_audio(&uri, config.clock_type(), sender);
            match ret {
                Ok(c) => *ctx = Some(c),
                Err(err) => forward_init_error(sender, err, PipelineType::Audio),
            }
        }
        Command::VideoSetBitrate { bitrate } => handle_video_set_bitrate(ctx, bitrate)?,
        Command::VideoForceIdr => handle_video_force_idr(ctx)?,
        Command::Stop => {
            *stop = true;
            drop(ctx.take());
        }
    }

    Ok(Response::None)
}

pub async fn run(mut worker: Worker<IpcApi>) -> Result<()> {
    info!("AV service started");

    let mut ctx = None;
    let config = config::load().await?;

    loop {
        let (response, cmd) = worker.commands.recv().await?;

        let mut stop = false;
        let res = handle_command(cmd, &worker.events, &mut stop, &mut ctx, &config)
            .await
            .map_err(|err| kyavservice_types::Error(err.to_string()));

        response.send(res)?;

        if stop {
            break;
        }
    }

    info!("AV service stopping");

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use kyavservice_types::{Command, Event, IpcApi, PipelineStage, PipelineType, Response};
    use libkypc::channel::create_pair;

    #[tokio::test(flavor = "current_thread")]
    async fn rtp_start_audio_error_reports_audio_pipeline_type() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mut commander, worker) = create_pair::<IpcApi>().unwrap();

                let worker_handle = tokio::task::spawn_local(super::run(worker));

                let ack = commander
                    .commands
                    .send(Command::RtpStartAudio {
                        ip: "127.0.0.1".into(),
                        port: 0,
                    })
                    .await
                    .unwrap();
                assert!(
                    matches!(ack, Ok(Response::None)),
                    "RtpStartAudio accept carries no event"
                );

                let evt = tokio::time::timeout(Duration::from_secs(5), commander.events.recv())
                    .await
                    .expect("timed out waiting for pipeline error event")
                    .expect("channel closed before receiving event");

                match evt {
                    Event::PipelineEvent(pe) => {
                        assert!(
                            matches!(pe.type_, PipelineType::Audio),
                            "Expected PipelineType::Audio on RTP audio error, got {:?}",
                            pe.type_
                        );
                        assert!(matches!(pe.stage, PipelineStage::Initialization));
                    }
                    other => panic!("Expected PipelineEvent, got {other:?}"),
                }

                let _ = commander.commands.send(Command::Stop).await;
                let _ = worker_handle.await;
            })
            .await;
    }
}
