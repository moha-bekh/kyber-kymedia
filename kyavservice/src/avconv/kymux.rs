// Project Kyber: kymux.rs
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

use libavconv_rs::{AvconvContext, DataSink, HwDeviceRef, sys};
use libkypc::EventSender;

use kyavservice_types::IpcApi;

use super::kymux_sink::{self, StreamKind};
use super::{
    AvconvAudioContext, AvconvVideoContext, Context, PipelineOutput, Result, VideoConfig, audio,
    video,
};
use crate::config::ClockType;

pub(crate) fn start_video(
    video_config: VideoConfig,
    uri: &str,
    metrics_uri: Option<&str>,
    _sender: EventSender<IpcApi>,
) -> Result<Context> {
    let metrics = match metrics_uri {
        Some(uri) => Some(super::metrics::forward(uri)?),
        None => None,
    };

    info!(
        "Start kymux video (avconv), display {}, encoder {:?}",
        video_config.display_id, video_config.encoder
    );

    let ctx = Arc::new(AvconvContext::new()?);

    #[cfg(target_os = "windows")]
    let hw_device = HwDeviceRef::new(sys::AVHWDeviceType_AV_HWDEVICE_TYPE_D3D11VA)?;
    #[cfg(target_os = "linux")]
    let hw_device = HwDeviceRef::new(sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VAAPI)?;
    #[cfg(target_os = "macos")]
    let hw_device = HwDeviceRef::new(sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VIDEOTOOLBOX)?;

    // The encoder feeds a data sink rather than a muxer: kymux wants the raw
    // encoded packets, and the sink hands them over without a container in the
    // middle. Draining happens on the watcher thread spawned below.
    let sink = Arc::new(DataSink::packets(
        Arc::clone(&ctx),
        sys::AVMediaType_AVMEDIA_TYPE_VIDEO,
    )?);
    let on_item = kymux_sink::connect(uri, StreamKind::Video, metrics.clone())?;

    let (pipeline, control) = video::configure(
        Arc::clone(&ctx),
        &hw_device,
        video_config,
        sink.handle(),
        metrics,
    )?;
    info!("Pipeline configured, starting context");

    ctx.start()?;
    info!("Context started");

    // Drain the sink from libavconv's wait thread, handing each packet to the
    // kymux writer. Runtime wait outcomes are discarded here.
    let watcher = Arc::clone(&ctx).watch_packet_sink(Arc::clone(&sink), on_item, |_| {});

    Ok(Context::Avconv(AvconvVideoContext {
        ctx,
        _output: PipelineOutput::Sink { _sink: sink },
        _pipeline: pipeline,
        control,
        _watcher: Some(watcher),
    }))
}

/// Start a kymux audio stream.
pub(crate) fn start_audio(
    uri: &str,
    clock_type: ClockType,
    _sender: EventSender<IpcApi>,
) -> Result<Context> {
    info!("Start kymux audio (avconv)");

    let ctx = Arc::new(AvconvContext::new()?);

    let sink = Arc::new(DataSink::packets(
        Arc::clone(&ctx),
        sys::AVMediaType_AVMEDIA_TYPE_AUDIO,
    )?);
    let on_item = kymux_sink::connect(uri, StreamKind::Audio, None)?;

    let pipeline = audio::configure(Arc::clone(&ctx), sink.handle(), &clock_type)?;
    info!("Audio pipeline configured, starting context");

    ctx.start()?;
    info!("Audio context started");

    let watcher = Arc::clone(&ctx).watch_packet_sink(Arc::clone(&sink), on_item, |_| {});

    Ok(Context::AvconvAudio {
        _ctx: AvconvAudioContext {
            ctx,
            _output: PipelineOutput::Sink { _sink: sink },
            _pipeline: pipeline,
            _watcher: Some(watcher),
        },
    })
}
