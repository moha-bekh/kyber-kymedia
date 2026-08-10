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
use libkypc::{EventSender, ReceivedCommand, Worker};
use txproto_rs::Txproto;

pub(crate) use clock::configure_clock;
pub use config::parse as parse_config;
pub use config::{Config, KyavserverLogExtras, from_table, kyavserver_log_extras};
pub use error::{Error, Result};
pub(crate) use video::{VideoConfig, VideoEncoder};

/// Per-target log level lookup, so a new pipeline backend (e.g. `avconv`,
/// alongside today's `txproto`) only needs its own `level_for` call at its
/// own `Txproto::new`/equivalent site, not a new parameter threaded through
/// every function between `run` and there.
pub trait TargetLogLevel {
    fn level_for(&self, target: &str) -> log::LevelFilter;
}

impl TargetLogLevel for log4ky::LoggerHandle {
    fn level_for(&self, target: &str) -> log::LevelFilter {
        log4ky::LoggerHandle::level_for(self, target)
    }
}

/// Opens a `Txproto` context at the `txproto` target's level.
pub(crate) fn open_txproto(log_levels: &dyn TargetLogLevel) -> Result<Txproto> {
    Ok(Txproto::new(log_levels.level_for("txproto"))?)
}

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

fn handle_video_set_bitrate(
    ctx: &mut Option<Context>,
    response: ReceivedCommand<'_, IpcApi>,
    bitrate: i32,
) -> Result<()> {
    info!("Set bitrate to {bitrate}");

    let Some(ctx) = &ctx else {
        info!("No session started");
        response.send(Err(kyavservice_types::Error("No session started".into())))?;

        return Ok(());
    };

    let Some(encoder) = &ctx.encoder else {
        info!("No active encoder");
        response.send(Err(kyavservice_types::Error("No active encoder".into())))?;

        return Ok(());
    };

    response.send(Ok(Response::None))?;

    ctx.ctx.encoder_set_bitrate(encoder, bitrate)?;
    ctx.ctx.commit()?;

    Ok(())
}

fn handle_video_force_idr(
    ctx: &mut Option<Context>,
    response: ReceivedCommand<'_, IpcApi>,
) -> Result<()> {
    info!("Video force IDR");

    let Some(ctx) = &ctx else {
        info!("No session started");
        response.send(Err(kyavservice_types::Error("No session started".into())))?;

        return Ok(());
    };

    let Some(encoder) = &ctx.encoder else {
        info!("No active encoder");
        response.send(Err(kyavservice_types::Error("No active encoder".into())))?;

        return Ok(());
    };

    response.send(Ok(Response::None))?;

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

pub async fn run(
    mut worker: Worker<IpcApi>,
    config: Config,
    log_levels: &dyn TargetLogLevel,
) -> Result<()> {
    info!("AV service started");

    let mut ctx = None;

    while let Ok((response, cmd)) = worker.commands.recv().await {
        match cmd {
            Command::EnumerateDisplays => {
                info!("Start display enumeration");

                #[cfg(target_os = "linux")]
                let api_list = video::grab_backend_api_list(config.grab_backend());
                #[cfg(not(target_os = "linux"))]
                let api_list: Vec<&str> = vec![];

                let (c, display_list) = displays::enumerate(&worker.events, &api_list, log_levels)?;

                response.send(Ok(Response::DisplayList {
                    displays: display_list,
                }))?;

                ctx = Some(c);
            }
            Command::RtpGenerateVideoSdp {
                display_id,
                yuv444,
                intra_refresh,
            } => {
                info!("Generate RTP Video SDP: yuv444: {yuv444}, intra_refresh: {intra_refresh:?}");
                let ret = rtp::generate_sdp(
                    display_id,
                    config.encoder(),
                    VideoCodec::H264,
                    yuv444,
                    intra_refresh.unwrap_or(config.intra_refresh()),
                    &config.video_encoder_config,
                    #[cfg(target_os = "linux")]
                    config.grab_backend(),
                    log_levels,
                )
                .await;

                match ret {
                    Ok(sdp) => {
                        response.send(Ok(Response::RtpSdpGenerated { sdp }))?;
                    }
                    Err(err) => {
                        error!("Failed to generate Video SDP: {err:?}");

                        response.send(Err(kyavservice_types::Error(err.to_string())))?;
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
                response.send(Ok(Response::None))?;
                info!(
                    "Start RTP Video: {ip}:{port}, bitrate: {bitrate}, yuv444: {yuv444}, intra_refresh: {intra_refresh:?}"
                );

                let video_config = VideoConfig {
                    display_id,
                    encoder: config.encoder(),
                    bitrate: ajust_min_bitrate(&config, bitrate),
                    codec: VideoCodec::H264,
                    yuv444,
                    intra_refresh: intra_refresh.unwrap_or(config.intra_refresh()),
                    encoder_config: config.video_encoder_config.clone(),
                    clock_type: config.clock_type(),
                    #[cfg(feature = "watermark")]
                    watermark: None,
                    #[cfg(target_os = "linux")]
                    grab_backend: config.grab_backend(),
                };

                let ret = rtp::start_video(video_config, ip, port, &worker.events, log_levels);
                match ret {
                    Ok(c) => ctx = Some(c),
                    Err(err) => forward_init_error(&worker.events, err, PipelineType::Video),
                }
            }
            Command::RtpStartAudio { ip, port } => {
                response.send(Ok(Response::None))?;
                info!("Start RTP Audio: {ip}:{port}");

                let ret =
                    rtp::start_audio(ip, port, config.clock_type(), &worker.events, log_levels);
                match ret {
                    Ok(c) => ctx = Some(c),
                    Err(err) => forward_init_error(&worker.events, err, PipelineType::Audio),
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
                response.send(Ok(Response::None))?;
                info!(
                    "Start Kymux Video: URI {uri:?}, bitrate: {bitrate}, codec: {codec:?}, yuv444: {yuv444}, intra_refresh: {intra_refresh:?}"
                );

                let video_config = VideoConfig {
                    display_id,
                    encoder: config.encoder(),
                    bitrate: ajust_min_bitrate(&config, bitrate),
                    codec,
                    yuv444,
                    intra_refresh: intra_refresh.unwrap_or(config.intra_refresh()),
                    encoder_config: config.video_encoder_config.clone(),
                    clock_type: config.clock_type(),
                    #[cfg(feature = "watermark")]
                    watermark: config.watermark.clone(),
                    #[cfg(target_os = "linux")]
                    grab_backend: config.grab_backend(),
                };

                let ret = kymux::start_video(
                    video_config,
                    &uri,
                    metrics_uri.as_deref(),
                    &worker.events,
                    log_levels,
                );
                match ret {
                    Ok(c) => ctx = Some(c),
                    Err(err) => forward_init_error(&worker.events, err, PipelineType::Video),
                }
            }
            Command::KymuxStartAudio { uri } => {
                response.send(Ok(Response::None))?;
                info!("Start Kymux Audio: URI {uri:?}");

                let ret = kymux::start_audio(&uri, config.clock_type(), &worker.events, log_levels);
                match ret {
                    Ok(c) => ctx = Some(c),
                    Err(err) => forward_init_error(&worker.events, err, PipelineType::Audio),
                }
            }
            Command::VideoSetBitrate { bitrate } => {
                handle_video_set_bitrate(&mut ctx, response, bitrate)?;
            }
            Command::VideoForceIdr => {
                handle_video_force_idr(&mut ctx, response)?;
            }
            Command::Stop => {
                response.send(Ok(Response::None))?;

                drop(ctx);

                break;
            }
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

    use crate::{Config, TargetLogLevel};

    /// A fixed level for every target, so tests don't need a real installed
    /// logger just to get a `TargetLogLevel`.
    struct FixedLogLevel(log::LevelFilter);

    impl TargetLogLevel for FixedLogLevel {
        fn level_for(&self, _target: &str) -> log::LevelFilter {
            self.0
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rtp_start_audio_error_reports_audio_pipeline_type() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mut commander, worker) = create_pair::<IpcApi>().unwrap();

                let worker_handle = tokio::task::spawn_local(async move {
                    super::run(
                        worker,
                        Config::default(),
                        &FixedLogLevel(log::LevelFilter::Off),
                    )
                    .await
                });

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
