// Project Kyber: rtp.rs
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

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, info, warn};

use kyavservice_types::{IpcApi, PipelineType, VideoCodec};
use libavconv_rs::{
    AvconvContext, AvioContext, Handle, HwDeviceRef, MUXER_FLAG_OPEN_IO, Muxer, MuxerBuilder, sys,
};
use libkypc::EventSender;

use super::error_watcher;
use super::utils;
use super::video::VideoConfig;
use super::{
    AvconvAudioContext, AvconvVideoContext, Context, Error, PipelineOutput, Result, audio, video,
};
use crate::config;
use crate::config::{ClockType, VideoEncoder};

const SDP_PROBE_PORT: u16 = 30_000;
const SDP_POLL_INTERVAL: Duration = Duration::from_millis(50);
const SDP_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Best-effort muxer option setter logs and ignores failures (e.g. an option
/// the active output format doesn't expose).
fn try_set_format_opt(muxer: &MuxerBuilder, key: &str, value: &str) {
    if let Err(err) = muxer.set_format_option(key, value) {
        debug!("RTP muxer set_format_option({key}={value}) failed: {err:?} (option likely absent)");
    }
}

/// Build and initialise an RTP muxer pointing at `rtp://ip:port`.
///
/// Returns the muxer (which must outlive its scheduler handles) and the
/// scheduler input handle for stream 0.
fn create_rtp_muxer(
    ctx: Arc<AvconvContext>,
    ip: &str,
    port: u16,
    media: sys::AVMediaType,
) -> Result<(Muxer, Handle)> {
    let url = format!("rtp://{ip}:{port}");

    let fmt = libavconv_rs::OutputFormat::guess("rtp").ok_or(libavconv_rs::Error::NotFound {
        name: "rtp output format",
    })?;

    let mut muxer = MuxerBuilder::new(ctx, fmt, &url, MUXER_FLAG_OPEN_IO)?;

    // Mirror txproto's RTP defaults: ship every packet immediately.
    try_set_format_opt(&muxer, "max_interleave_delta", "0");
    try_set_format_opt(&muxer, "low_latency", "1");

    muxer.add_stream(media)?;
    let muxer = muxer.build()?;

    let stream_hdl = muxer.stream_handle(0);
    Ok((muxer, stream_hdl))
}

pub(crate) async fn generate_sdp(
    display_id: u32,
    encoder: VideoEncoder,
    codec: VideoCodec,
    yuv444: bool,
    intra_refresh: bool,
    encoder_config: &Option<config::EncoderConfig>,
) -> Result<String> {
    info!("Generate RTP Video SDP via libavconv probe");

    // Path the probe pipeline writes the SDP to.  Removed up-front so a stale
    // file from a previous run can't satisfy the polling loop early.
    let sdp_path = std::env::temp_dir().join("video.sdp");
    if sdp_path.exists()
        && let Err(err) = std::fs::remove_file(&sdp_path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        warn!("Failed to clear existing SDP file: {err:?}");
        return Err(Error::RtpVideoSdpFileDeleteFailed { reason: err });
    }
    let sdp_path_str = utils::path_to_string(&sdp_path)?;

    let encoder_config = encoder_config.clone();
    let probe_path = sdp_path_str.clone();

    // The probe runs on a detached blocking thread:
    //   1. Build pipeline, point libavconv at the SDP file.
    //   2. Start the pipeline; poll the file until libavconv flushes the SDP.
    //   3. Send the parsed `a=fmtp` line back over a oneshot.
    //   4. **Then** call ctx.stop() and remove the file.
    //
    // Step 4 must happen after step 3 because libavconv's `avconv_stop`
    // does a `pthread_join` on every pipeline thread with no timeout. The
    // JoinHandle on the spawn_blocking task is dropped (detached), so that
    // cleanup runs in the background while the caller already has its
    // response.
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<String>>();
    let _detached: tokio::task::JoinHandle<()> = tokio::task::spawn_blocking(move || {
        let probe = build_and_capture_sdp(
            display_id,
            encoder,
            codec,
            yuv444,
            intra_refresh,
            encoder_config,
            &probe_path,
        );
        let (parsed, anchor) = match probe {
            Ok((parsed, anchor)) => (parsed, Some(anchor)),
            Err(err) => (Err(err), None),
        };

        // Reply *before* the slow teardown.
        let _ = tx.send(parsed);

        // Cleanup may block inside avconv_stop's pthread_join.
        // The SDP file at `probe_path` is intentionally left on disk: kyclient
        // reads it directly out of `%TEMP%\video.sdp` after the probe, matching
        // the original txproto behaviour.  It is overwritten on the next probe.
        if let Some(anchor) = anchor {
            let _ = anchor.ctx.stop();
        }
    });

    rx.await.map_err(|_| Error::ThreadPanicked)?
}

/// Anchor that keeps the SDP probe's libavconv nodes alive after
/// `build_and_capture_sdp` returns.
struct ProbeAnchor {
    ctx: Arc<AvconvContext>,
    _muxer: libavconv_rs::Muxer,
    _pipeline: super::video::VideoPipeline,
}

/// Build the probe pipeline, run it, and return the parsed `a=fmtp` line
/// alongside the still-running `AvconvContext` (so the caller can stop it
/// after replying to the client).
fn build_and_capture_sdp(
    display_id: u32,
    encoder: VideoEncoder,
    codec: VideoCodec,
    yuv444: bool,
    intra_refresh: bool,
    encoder_config: Option<config::EncoderConfig>,
    probe_path: &str,
) -> Result<(Result<String>, ProbeAnchor)> {
    let video_config = VideoConfig {
        display_id,
        encoder,
        bitrate: 20_000_000,
        codec,
        yuv444,
        intra_refresh,
        encoder_config,
        clock_type: config::ClockType::ZeroedOrigin,
        #[cfg(feature = "watermark")]
        watermark: None,
        #[cfg(target_os = "linux")]
        grab_backend: config::GrabBackend::NvFBC,
    };

    let ctx = Arc::new(AvconvContext::new()?);

    #[cfg(target_os = "windows")]
    let hw_device = HwDeviceRef::new(sys::AVHWDeviceType_AV_HWDEVICE_TYPE_D3D11VA)?;
    #[cfg(target_os = "linux")]
    let hw_device = HwDeviceRef::new(sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VAAPI)?;
    #[cfg(target_os = "macos")]
    let hw_device = HwDeviceRef::new(sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VIDEOTOOLBOX)?;

    let (muxer, stream_hdl) = create_rtp_muxer(
        Arc::clone(&ctx),
        "127.0.0.1",
        SDP_PROBE_PORT,
        sys::AVMediaType_AVMEDIA_TYPE_VIDEO,
    )?;

    let (pipeline, _control) =
        video::configure(Arc::clone(&ctx), &hw_device, video_config, stream_hdl, None)?;

    // Hand libavconv a file-backed AVIOContext. `avconv_sdp` takes
    // ownership; libavconv `avio_closep`s it once the SDP is written,
    // which flushes & closes the file.
    let sdp_io = AvioContext::open_for_write(Path::new(probe_path))?;
    ctx.request_sdp(sdp_io)?;

    ctx.start()?;

    // Poll the file synchronously break out the moment libavconv has
    // flushed the SDP to disk (typically within a few hundred ms once the
    // first frame reaches the encoder).
    let deadline = Instant::now() + SDP_PROBE_TIMEOUT;
    let mut sdp_text = String::new();
    while Instant::now() < deadline {
        if let Ok(content) = std::fs::read_to_string(probe_path)
            && !content.is_empty()
        {
            sdp_text = content;
            break;
        }
        std::thread::sleep(SDP_POLL_INTERVAL);
    }

    let parsed = if sdp_text.is_empty() {
        warn!("Timed out waiting for libavconv to emit SDP");
        Err(Error::RtpVideoSdpFileNotFound)
    } else {
        sdp_text
            .lines()
            .find(|l| l.starts_with("a=fmtp"))
            .map(|l| l.to_owned())
            .ok_or_else(|| {
                warn!("Line starting with 'a=fmtp' not found in SDP");
                Error::RtpVideoSdpParseFailed
            })
    };

    Ok((
        parsed,
        ProbeAnchor {
            ctx,
            _muxer: muxer,
            _pipeline: pipeline,
        },
    ))
}

pub(crate) fn start_video(
    video_config: VideoConfig,
    ip: String,
    port: u16,
    sender: EventSender<IpcApi>,
) -> Result<Context> {
    info!(
        "Start RTP video (avconv), display {}, encoder {:?}, target {ip}:{port}",
        video_config.display_id, video_config.encoder
    );

    let ctx = Arc::new(AvconvContext::new()?);

    #[cfg(target_os = "windows")]
    let hw_device = HwDeviceRef::new(sys::AVHWDeviceType_AV_HWDEVICE_TYPE_D3D11VA)?;
    #[cfg(target_os = "linux")]
    let hw_device = HwDeviceRef::new(sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VAAPI)?;
    #[cfg(target_os = "macos")]
    let hw_device = HwDeviceRef::new(sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VIDEOTOOLBOX)?;

    let (muxer, stream_hdl) = create_rtp_muxer(
        Arc::clone(&ctx),
        &ip,
        port,
        sys::AVMediaType_AVMEDIA_TYPE_VIDEO,
    )?;

    let (pipeline, control) =
        video::configure(Arc::clone(&ctx), &hw_device, video_config, stream_hdl, None)?;

    ctx.start()?;
    info!("RTP video context started");

    let watcher = error_watcher::spawn(Arc::clone(&ctx), sender, PipelineType::Video);

    Ok(Context::Avconv(AvconvVideoContext {
        ctx,
        _output: PipelineOutput::Muxer { _muxer: muxer },
        _pipeline: pipeline,
        control,
        _watcher: Some(watcher),
    }))
}

pub(crate) fn start_audio(
    ip: String,
    port: u16,
    clock_type: ClockType,
    sender: EventSender<IpcApi>,
) -> Result<Context> {
    info!("Start RTP audio (avconv), target {ip}:{port}");

    let ctx = Arc::new(AvconvContext::new()?);

    let (muxer, stream_hdl) = create_rtp_muxer(
        Arc::clone(&ctx),
        &ip,
        port,
        sys::AVMediaType_AVMEDIA_TYPE_AUDIO,
    )?;

    let pipeline = audio::configure(Arc::clone(&ctx), stream_hdl, &clock_type)?;

    ctx.start()?;
    info!("RTP audio context started");

    let watcher = error_watcher::spawn(Arc::clone(&ctx), sender, PipelineType::Audio);

    Ok(Context::AvconvAudio {
        _ctx: AvconvAudioContext {
            ctx,
            _output: PipelineOutput::Muxer { _muxer: muxer },
            _pipeline: pipeline,
            _watcher: Some(watcher),
        },
    })
}
