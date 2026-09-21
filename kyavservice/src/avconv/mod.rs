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
#[cfg(target_os = "windows")]
mod cursor;
mod display_watcher;
mod displays;
mod encoder_control;
mod encoder_hook;
mod error;
mod error_watcher;
mod kymux;
mod kymux_sink;
mod metrics;
mod rtp;
mod utils;
mod video;
#[cfg(feature = "watermark")]
mod watermark;

use std::sync::Arc;

use log::{debug, error, info, trace, warn};

use kyavservice_types::{
    Command, Event, IpcApi, PipelineEvent, PipelineEventType, PipelineStage, PipelineType,
    Response, VideoCodec,
};
use libavconv_rs::sys;
use libkypc::{EventSender, Worker};

use crate::TargetLogLevel;
use crate::config::Config;

use self::encoder_control::EncoderControl;
pub use self::error::{Error, Result};
pub(crate) use self::video::VideoConfig;

/// Terminal node of an avconv pipeline, held only to keep it alive alongside
/// the context: RTP writes through a real muxer, kymux drains a data sink.
pub(crate) enum PipelineOutput {
    Muxer { _muxer: libavconv_rs::Muxer },
    Sink { _sink: Arc<libavconv_rs::DataSink> },
}

/// Context for sessions that use libavconv (kymux/RTP video).
pub(crate) struct AvconvVideoContext {
    pub(crate) ctx: Arc<libavconv_rs::AvconvContext>,
    pub(crate) _output: PipelineOutput,
    pub(crate) _pipeline: video::VideoPipeline,
    /// Shared runtime-control handle for the encoder drives
    /// `VideoSetBitrate` / `VideoForceIdr`. `None` when the source is
    /// forwarded pre-encoded: there is no encoder to drive.
    pub(crate) control: Option<Arc<EncoderControl>>,
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
    Avconv(AvconvVideoContext),
    AvconvAudio { _ctx: AvconvAudioContext },
}

fn detach_teardown(old: Context) {
    tokio::task::spawn_blocking(move || drop(old));
}

fn set_ctx(ctx: &mut Option<Context>, new: Context) {
    if let Some(old) = ctx.replace(new) {
        detach_teardown(old);
    }
}

fn forward_init_error(sender: &EventSender<IpcApi>, err: Error, type_: PipelineType) {
    error!("Failed to start {type_:?} pipeline: {err:?}");

    sender.send(Event::PipelineEvent(PipelineEvent {
        type_,
        stage: PipelineStage::Initialization,
        event: PipelineEventType::Error,
    }));
}

fn encoder_control(ctx: &Option<Context>) -> Result<&Arc<EncoderControl>> {
    match ctx {
        Some(Context::Avconv(AvconvVideoContext {
            control: Some(control),
            ..
        })) => Ok(control),
        Some(Context::Avconv(AvconvVideoContext { control: None, .. })) => {
            warn!("Video source is pre-encoded, encoder controls are not available");
            Err(std::io::Error::other(
                "Video source is pre-encoded, encoder controls are not available",
            )
            .into())
        }
        _ => {
            info!("No active video encoder");
            Err(std::io::Error::other("No active video encoder").into())
        }
    }
}

fn handle_video_set_bitrate(ctx: &mut Option<Context>, bitrate: i32) -> Result<()> {
    info!("Set bitrate to {bitrate}");
    encoder_control(ctx)?.request_bitrate(bitrate as i64);
    Ok(())
}

fn handle_video_force_idr(ctx: &mut Option<Context>) -> Result<()> {
    info!("Video force IDR");
    encoder_control(ctx)?.request_force_idr();
    Ok(())
}

fn av_log_level(level: log::LevelFilter) -> i32 {
    match level {
        log::LevelFilter::Off => sys::AV_LOG_QUIET,
        log::LevelFilter::Error => sys::AV_LOG_ERROR as i32,
        log::LevelFilter::Warn => sys::AV_LOG_WARNING as i32,
        log::LevelFilter::Info => sys::AV_LOG_INFO as i32,
        log::LevelFilter::Debug => sys::AV_LOG_VERBOSE as i32,
        log::LevelFilter::Trace => sys::AV_LOG_TRACE as i32,
    }
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

fn video_config(
    config: &Config,
    display_id: u32,
    bitrate: u32,
    codec: VideoCodec,
    yuv444: bool,
    intra_refresh: Option<bool>,
) -> Result<VideoConfig> {
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut encoder = config.encoder();

    #[cfg(target_os = "linux")]
    let (source_mode, source_size, output_size, gstreamer_pipeline) = match config
        .gstreamer_video_source(display_id)?
    {
        Some(source) => {
            info!("Using GStreamer video source '{}'", source.name);
            // A pre-encoded source is forwarded as-is, so the codec the
            // client asked for must be the one the pipeline produces;
            // otherwise the client would decode a mislabeled stream.
            if source.mode == crate::config::VideoSourceMode::Encoded
                && source.codec.as_ref().is_some_and(|c| *c != codec)
            {
                return Err(Error::ConfigInvalid {
                    reason: format!(
                        "GStreamer encoded video source '{}' produces {:?} but client requested {codec:?}",
                        source.name, source.codec
                    ),
                });
            }
            encoder = source.encoder.unwrap_or(encoder);
            let source_mode = source.mode;
            let source_size = Some(source.source_size());
            let output_size = Some(source.output_size());
            let pipeline = Some(source.pipeline());
            (source_mode, source_size, output_size, pipeline)
        }
        None => (crate::config::VideoSourceMode::Raw, None, None, None),
    };

    Ok(VideoConfig {
        display_id,
        #[cfg(target_os = "linux")]
        source_mode,
        #[cfg(target_os = "linux")]
        source_size,
        #[cfg(target_os = "linux")]
        output_size,
        encoder,
        bitrate: ajust_min_bitrate(config, bitrate),
        codec,
        yuv444,
        intra_refresh: intra_refresh.unwrap_or(config.intra_refresh()),
        encoder_config: config.video_encoder_config.clone(),
        clock_type: config.clock_type(),
        #[cfg(feature = "watermark")]
        watermark: None,
        #[cfg(target_os = "linux")]
        grab_backend: config.grab_backend(),
        #[cfg(target_os = "linux")]
        gstreamer_pipeline,
    })
}

/// Same pipeline selection as `RtpStartVideo` (`video_config`) so the SDP
/// describes the stream the client will actually receive: on a headless
/// board with `grab_backend = "gstreamer"` a probe hardwired to the desktop
/// grabber + VAAPI fails, or describes a different encoder than the stream.
///
/// A pre-encoded source has no libavconv encoder to probe, so no SDP can be
/// produced for it; refuse explicitly instead of probing the wrong pipeline.
fn sdp_probe_config(
    config: &Config,
    display_id: u32,
    yuv444: bool,
    intra_refresh: Option<bool>,
) -> Result<VideoConfig> {
    let video_config = video_config(
        config,
        display_id,
        rtp::SDP_PROBE_BITRATE,
        VideoCodec::H264,
        yuv444,
        intra_refresh,
    )?;

    #[cfg(target_os = "linux")]
    if video_config.source_mode == crate::config::VideoSourceMode::Encoded {
        return Err(Error::ConfigInvalid {
            reason: format!(
                "SDP generation is not supported for pre-encoded GStreamer source (display {display_id})"
            ),
        });
    }

    Ok(video_config)
}

async fn handle_command(
    cmd: Command,
    sender: &EventSender<IpcApi>,
    stop: &mut bool,
    ctx: &mut Option<Context>,
    display_watcher: &mut Option<display_watcher::WatcherHandle>,
    config: &Config,
) -> Result<Response> {
    match cmd {
        Command::EnumerateDisplays => {
            info!("Start display enumeration");

            #[cfg(target_os = "linux")]
            let backend = Some(config.grab_backend());
            #[cfg(not(target_os = "linux"))]
            let backend = None;

            let display_list = displays::enumerate(config)?;

            // Lazily start the hot-plug watcher on first enumeration.
            // Subsequent calls just refresh the synchronous response;
            // the watcher keeps running until `Command::Stop`.
            #[cfg(target_os = "linux")]
            let display_watcher_enabled = !config.is_gstreamer();
            #[cfg(not(target_os = "linux"))]
            let display_watcher_enabled = true;

            if display_watcher.is_none() && display_watcher_enabled {
                *display_watcher = display_watcher::spawn(backend, sender.clone());
            }

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
            let ret = match sdp_probe_config(config, display_id, yuv444, intra_refresh) {
                Ok(video_config) => rtp::generate_sdp(video_config).await,
                Err(err) => Err(err),
            };

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

            let video_config = video_config(
                config,
                display_id,
                bitrate,
                VideoCodec::H264,
                yuv444,
                intra_refresh,
            )?;

            let ret = rtp::start_video(video_config, ip, port, sender.clone());
            match ret {
                Ok(c) => set_ctx(ctx, c),
                Err(err) => forward_init_error(sender, err, PipelineType::Video),
            }
        }
        Command::RtpStartAudio { ip, port } => {
            info!("Start RTP Audio: {ip}:{port}");

            let ret = rtp::start_audio(ip, port, config.clock_type(), sender.clone());
            match ret {
                Ok(c) => set_ctx(ctx, c),
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

            #[cfg_attr(not(feature = "watermark"), allow(unused_mut))]
            let mut video_config =
                video_config(config, display_id, bitrate, codec, yuv444, intra_refresh)?;
            #[cfg(feature = "watermark")]
            {
                video_config.watermark = config.watermark.clone();
            }

            let ret =
                kymux::start_video(video_config, &uri, metrics_uri.as_deref(), sender.clone());
            match ret {
                Ok(c) => set_ctx(ctx, c),
                Err(err) => forward_init_error(sender, err, PipelineType::Video),
            }
        }
        Command::KymuxStartAudio { uri } => {
            info!("Start Kymux Audio: URI {uri:?}");

            let ret = kymux::start_audio(&uri, config.clock_type(), sender.clone());
            match ret {
                Ok(c) => set_ctx(ctx, c),
                Err(err) => forward_init_error(sender, err, PipelineType::Audio),
            }
        }
        Command::VideoSetBitrate { bitrate } => handle_video_set_bitrate(ctx, bitrate)?,
        Command::VideoForceIdr => handle_video_force_idr(ctx)?,
        Command::Stop => {
            *stop = true;
            if let Some(old) = ctx.take() {
                detach_teardown(old);
            }
            if let Some(watcher) = display_watcher.take() {
                watcher.shutdown_and_join();
            }
        }
    }

    Ok(Response::None)
}

pub(super) async fn run(
    mut worker: Worker<IpcApi>,
    config: Config,
    log_levels: &dyn TargetLogLevel,
) -> Result<()> {
    libavconv_rs::set_log_level(av_log_level(log_levels.level_for("ffmpeg")));

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
    let mut display_watcher: Option<display_watcher::WatcherHandle> = None;

    loop {
        let (response, cmd) = worker.commands.recv().await?;

        let mut stop = false;
        let res = handle_command(
            cmd,
            &worker.events,
            &mut stop,
            &mut ctx,
            &mut display_watcher,
            &config,
        )
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    fn gstreamer_config(mode: &str, extra: &str) -> Config {
        let table: toml::Table = format!(
            r#"
[kyavserver]
backend = "libavconv"
grab_backend = "gstreamer"

[kyavserver.gstreamer]

[[kyavserver.gstreamer.video_sources]]
name = "cam"
source = "v4l2src device=/dev/video0"
width = 1920
height = 1080
framerate = "30/1"
mode = "{mode}"
{extra}
"#
        )
        .parse()
        .unwrap();
        crate::config::from_table(&table).unwrap()
    }

    #[test]
    fn sdp_probe_follows_gstreamer_raw_source() {
        let config = gstreamer_config("raw", r#"encoder = "v4l2m2m""#);

        let probe = sdp_probe_config(&config, 0, false, None).unwrap();

        assert_eq!(probe.grab_backend, crate::config::GrabBackend::Gstreamer);
        assert!(matches!(
            probe.encoder,
            crate::config::VideoEncoder::V4L2M2M
        ));
        assert_eq!(probe.source_mode, crate::config::VideoSourceMode::Raw);
        assert!(
            probe
                .gstreamer_pipeline
                .as_deref()
                .is_some_and(|p| p.starts_with("v4l2src device=/dev/video0 !"))
        );
    }

    #[test]
    fn sdp_probe_rejects_encoded_gstreamer_source() {
        let config = gstreamer_config("encoded", r#"codec = "H264""#);

        let Err(err) = sdp_probe_config(&config, 0, false, None) else {
            panic!("encoded source must be rejected");
        };

        assert!(matches!(err, Error::ConfigInvalid { .. }));
        assert!(err.to_string().contains("pre-encoded"));
    }
}
