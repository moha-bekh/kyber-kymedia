// Project Kyber: audio.rs
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

use std::sync::Arc;

use log::info;

use libavconv_rs::{
    AvconvContext, Encoder, EncoderBuilder, FilterGraph, FilterGraphBuilder, GrabCapture,
    GrabCaptureBuilder, GrabProviderCtx, GrabSourceType, Handle, sys,
};

use super::Result;
use super::clock;
use crate::config::ClockType;

const BITRATE: i64 = 128_000;
const SAMPLE_RATE: i32 = 48_000;

#[cfg(target_os = "windows")]
fn audio_provider_name() -> &'static str {
    "wasapi"
}

#[cfg(target_os = "linux")]
fn audio_provider_name() -> &'static str {
    "pulse"
}

#[cfg(target_os = "macos")]
fn audio_provider_name() -> &'static str {
    "coreaudio"
}

pub(crate) struct AudioPipeline {
    pub(crate) _capture: GrabCapture,
    pub(crate) _fg: FilterGraph,
    pub(crate) _enc: Encoder,
}

/// Pick the system-audio loopback source preferred-flagged one if any,
/// else the first loopback, else the first source at all.
fn pick_loopback_source<'a>(
    sources: &'a libavconv_rs::GrabSourceSet,
) -> Option<libavconv_rs::GrabSource<'a>> {
    let preferred_flag = sys::AVGrabSourceFlags_AVGRAB_SOURCE_FLAG_PREFERRED;

    let mut first_loopback: Option<libavconv_rs::GrabSource<'a>> = None;
    for src in sources.iter() {
        if src.source_type() == GrabSourceType::AudioLoopback {
            if src.flags() & preferred_flag != 0 {
                return Some(src);
            }
            first_loopback.get_or_insert(src);
        }
    }

    first_loopback.or_else(|| sources.iter().next())
}

/// Set up the audio capture source.  Returns `(capture, output_handle)`.
fn create_grab_source(
    ctx: Arc<AvconvContext>,
    clock_type: &ClockType,
) -> Result<(GrabCapture, Handle)> {
    let provider = libavconv_rs::provider_by_name(audio_provider_name()).ok_or(
        libavconv_rs::Error::NotFound {
            name: "audio grab provider",
        },
    )?;

    let mut grab_ctx = GrabProviderCtx::new(provider)?;
    grab_ctx.init()?;

    let payloads: u32 = 1 << sys::AVConvHandlePayload_AVCONV_HANDLE_PAYLOAD_FRAME;
    let src_types: u64 = 1u64 << (sys::AVGrabSourceType_AVGRAB_SOURCE_TYPE_AUDIO_LOOPBACK as u64);

    let sources = grab_ctx.sources(payloads, src_types)?;

    let src = pick_loopback_source(&sources).ok_or(libavconv_rs::Error::NoSource)?;

    let name = src.name().unwrap_or_else(|| "<unnamed>".to_string());
    info!("Selected audio loopback: {name}");

    let mut capture = GrabCaptureBuilder::new(src)?;
    // Match the video epoch so audio and video PTS share a clock domain
    // mirrors the txproto-era `configure_clock` on the context.
    clock::configure_capture_clock(&mut capture, clock_type);
    let capture = capture.build(ctx.clone())?;

    let handle = capture.stream_handle(0);
    Ok((capture, handle))
}

/// Configure the Opus encoder for a stereo 48 kHz stream.
fn configure_opus_encoder(enc: &EncoderBuilder) -> Result<()> {
    enc.set_bit_rate(BITRATE);
    enc.set_sample_rate(SAMPLE_RATE);
    enc.set_sample_fmt(sys::AVSampleFormat_AV_SAMPLE_FMT_FLT);
    enc.enable_global_header();

    enc.set_channel_layout(libavconv_rs::ChannelLayout::stereo());

    enc.set_option("application", "audio")?;
    enc.set_option("frame_duration", "20")?;
    enc.set_option("vbr", "on")?;

    Ok(())
}

/// Build + wire the audio pipeline
///
/// Caller must then call `ctx.start()` to begin processing.
pub(crate) fn configure(
    ctx: Arc<AvconvContext>,
    output_hdl: Handle,
    clock_type: &ClockType,
) -> Result<AudioPipeline> {
    info!("Start audio grab (loopback), encoder libopus");

    let (capture, capture_handle) = create_grab_source(Arc::clone(&ctx), clock_type)?;

    let mut fg = FilterGraphBuilder::new(
        Arc::clone(&ctx),
        "aresample=osr=48000:ochl=stereo:osf=flt",
        None,
    )?;

    let codec = libavconv_rs::Codec::find_encoder_by_name("libopus").ok_or(
        libavconv_rs::Error::NotFound {
            name: "libopus encoder",
        },
    )?;

    let enc = EncoderBuilder::new(Arc::clone(&ctx), codec)?;
    configure_opus_encoder(&enc)?;

    // Opus has a fixed frame size (960 samples for 20 ms @ 48 kHz). libavconv
    // requires a pre-encoding sync queue for every encoder whose on_open hook
    // reports a frame size without it the scheduler asserts on
    // `enc->sq_idx[0] >= 0`.
    //
    // Attach the encoder's input handle to a freshly-created sync queue with
    // a 1 s buffer; the encoder must be the limiting stream of its own SQ.
    let sq = ctx.add_sync_queue(1_000_000)?;
    sq.add_encoder(enc.handle_in(), true, u64::MAX)?;

    fg.set_out_params_from_enc(0, &enc)?;

    ctx.connect(capture_handle, fg.input_handle(0), 0)?;
    ctx.connect(fg.output_handle(0), enc.handle_in(), 0)?;

    let fg = fg.build()?;
    let enc = enc.build()?;

    ctx.connect(enc.handle_out(), output_hdl, 0)?;

    Ok(AudioPipeline {
        _capture: capture,
        _fg: fg,
        _enc: enc,
    })
}
