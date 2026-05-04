// Project Kyber: mod.rs
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
mod displays;
mod error;
mod kymux;
mod kymux_sink;
mod metrics;
mod video;
#[cfg(feature = "watermark")]
mod watermark;

use std::sync::Arc;

use log::{debug, error, info, trace, warn};

use kyavservice_types::{
    Command, Event, IpcApi, PipelineEvent, PipelineEventType, PipelineStage, PipelineType, Response,
};
use libavconv_rs::sys;
use libkypc::{EventSender, Worker};

use crate::TargetLogLevel;
use crate::config::Config;

pub use self::error::{Error, Result};
pub(crate) use self::video::VideoConfig;

/// Terminal node of an avconv pipeline, held only to keep it alive alongside
/// the context: kymux drains a data sink.
pub(crate) enum PipelineOutput {
    Sink { _sink: Arc<libavconv_rs::DataSink> },
}

/// Context for sessions that use libavconv (kymux/RTP video).
pub(crate) struct AvconvVideoContext {
    pub(crate) ctx: Arc<libavconv_rs::AvconvContext>,
    pub(crate) _output: PipelineOutput,
    pub(crate) _pipeline: video::VideoPipeline,
    pub(crate) _watcher: Option<libavconv_rs::ErrorWatcher>,
}

impl Drop for AvconvVideoContext {
    fn drop(&mut self) {
        if let Some(w) = &self._watcher {
            w.signal_shutdown();
        }
        let _ = self.ctx.stop();
    }
}

/// Context for sessions that use libavconv (kymux audio).
pub(crate) struct AvconvAudioContext {
    pub(crate) ctx: Arc<libavconv_rs::AvconvContext>,
    pub(crate) _output: PipelineOutput,
    pub(crate) _pipeline: audio::AudioPipeline,
    pub(crate) _watcher: Option<libavconv_rs::ErrorWatcher>,
}

impl Drop for AvconvAudioContext {
    fn drop(&mut self) {
        if let Some(w) = &self._watcher {
            w.signal_shutdown();
        }
        let _ = self.ctx.stop();
    }
}

pub(crate) enum Context {
    Avconv { _ctx: AvconvVideoContext },
    AvconvAudio { _ctx: AvconvAudioContext },
}

fn forward_init_error(sender: &EventSender<IpcApi>, err: Error, type_: PipelineType) {
    error!("Failed to start {type_:?} pipeline: {err:?}");

    sender.send(Event::PipelineEvent(PipelineEvent {
        type_,
        stage: PipelineStage::Initialization,
        event: PipelineEventType::Error,
    }));
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
            let backend = Some(config.grab_backend());
            #[cfg(not(target_os = "linux"))]
            let backend = None;

            let display_list = displays::enumerate(backend)?;

            return Ok(Response::DisplayList {
                displays: display_list,
            });
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
                encoder: config.encoder(),
                bitrate: ajust_min_bitrate(config, bitrate),
                codec,
                yuv444,
                intra_refresh: intra_refresh.unwrap_or(config.intra_refresh()),
                encoder_config: config.video_encoder_config.clone(),
                clock_type: config.clock_type(),
                #[cfg(feature = "watermark")]
                watermark: config.watermark.clone(),
                #[cfg(target_os = "linux")]
                grab_backend: Some(config.grab_backend()),
            };

            let ret =
                kymux::start_video(video_config, &uri, metrics_uri.as_deref(), sender.clone());
            match ret {
                Ok(c) => *ctx = Some(c),
                Err(err) => forward_init_error(sender, err, PipelineType::Video),
            }
        }
        Command::KymuxStartAudio { uri } => {
            info!("Start Kymux Audio: URI {uri:?}");

            let ret = kymux::start_audio(&uri, config.clock_type(), sender.clone());
            match ret {
                Ok(c) => *ctx = Some(c),
                Err(err) => forward_init_error(sender, err, PipelineType::Audio),
            }
        }
        Command::Stop => {
            *stop = true;
            drop(ctx.take());
        }
        // The RTP transport and the runtime encoder commands are not wired into
        // this backend yet. Unreachable in practice: nothing dispatches to
        // `avconv::run` until the backend becomes runtime-selectable.
        cmd => {
            warn!("Command {cmd:?} not supported by the libavconv backend");
            return Err(std::io::Error::other("unsupported command").into());
        }
    }

    Ok(Response::None)
}

pub(super) async fn run(
    mut worker: Worker<IpcApi>,
    config: Config,
    _log_levels: &dyn TargetLogLevel,
) -> Result<()> {
    libavconv_rs::set_log_level(libavconv_rs::sys::AV_LOG_TRACE as i32);

    libavconv_rs::route_logs_to_sink(|level, msg| {
        if level <= sys::AV_LOG_ERROR as i32 {
            error!(target: "ffmpeg", "{msg}");
        } else if level <= sys::AV_LOG_WARNING as i32 {
            warn!(target: "ffmpeg", "{msg}");
        } else if level <= sys::AV_LOG_INFO as i32 {
            info!(target: "ffmpeg", "{msg}");
        } else if level <= sys::AV_LOG_VERBOSE as i32 {
            debug!(target: "ffmpeg", "{msg}");
        } else {
            trace!(target: "ffmpeg", "{msg}");
        }
    });

    info!("AV service started (libavconv backend)");

    let mut ctx: Option<Context> = None;

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
