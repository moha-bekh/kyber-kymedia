// Project Kyber: rtp.rs
// Copyright © 2022-2026 Kyber SAS
// SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0
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

use std::env;
use std::io;
use std::time::Duration;

use log::{info, warn};
use tokio::{fs, time};

use kyavservice_types::VideoCodec;
use libkypc::EventSender;
use txproto_rs::{Dictionary, Txproto};

use crate::config::{self, ClockType};
use crate::video::{VideoConfig, VideoEncoder};
use crate::{audio, configure_clock, utils, video};
use crate::{Context, Error, Event, Result};

fn create_muxer(
    ctx: &Txproto,
    ip: &str,
    port: u16,
    sdp_file: Option<&str>,
) -> Result<txproto_rs::Handle> {
    let out_url = format!("rtp://{ip}:{port}");

    let options = Dictionary::from_entries(&[("max_interleave_delta", 0).into()])?;

    let mut init_opts = vec![("dump_info", true).into(), ("low_latency", true).into()];

    if let Some(sdp_file) = sdp_file {
        init_opts.push(("sdp_file", sdp_file).into());
    }

    let init_opts = Dictionary::from_entries(&init_opts)?;

    let muxer = ctx.create_muxer(&out_url, "rtp", Some(options), Some(init_opts))?;
    Ok(muxer)
}

pub(crate) async fn generate_sdp(
    display_id: u32,
    encoder: VideoEncoder,
    codec: VideoCodec,
    yuv444: bool,
    intra_refresh: bool,
    encoder_config: &Option<config::EncoderConfig>,
) -> Result<String> {
    // Prepare target file
    let sdp_path = env::temp_dir().join("video.sdp");

    let ret = fs::remove_file(&sdp_path).await;
    if let Err(err) = ret {
        if err.kind() != io::ErrorKind::NotFound {
            warn!("Fail to delete SDP file");
            return Err(Error::RtpVideoSdpFileDeleteFailed { reason: err });
        }
    }

    // Run txproto
    let ctx = Txproto::new()?;
    ctx.set_epoch_offset(0)?;

    let sdp_path_str = utils::path_to_string(&sdp_path)?;
    let muxer = create_muxer(&ctx, "localhost", 30_000, Some(&sdp_path_str))?;

    let video_config = VideoConfig {
        display_id,
        encoder,
        bitrate: 20_000_000,
        codec,
        yuv444,
        intra_refresh,
        encoder_config: encoder_config.clone(),
        clock_type: config::ClockType::ZeroedOrigin,
        watermark: None,
        #[cfg(target_os = "linux")]
        grab_backend: Some(config::GrabBackend::NvFBC),
    };
    video::configure(&ctx, video_config, muxer, None)?;

    ctx.commit()?;

    // Wait for sdp file to be available
    let mut sdp_content = None;

    for _ in 0..50 {
        let ret = fs::read_to_string(&sdp_path).await;
        match ret {
            Ok(file_content) => {
                info!("Got SDP file");
                sdp_content = Some(file_content);
                break;
            }
            Err(_) => {
                info!("Waiting for SDP file");
                time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
    }

    drop(ctx);

    // Parse SDP content
    let Some(sdp_content) = sdp_content else {
        warn!("Couldn't find SDP file {sdp_path:?}");
        return Err(Error::RtpVideoSdpFileNotFound);
    };

    for l in sdp_content.lines() {
        if l.starts_with("a=fmtp") {
            return Ok(l.to_string());
        }
    }

    warn!("Line starting with 'a=fmtp' not found in {sdp_path:?}");
    Err(Error::RtpVideoSdpParseFailed)
}

pub(crate) fn start_video(
    video_config: VideoConfig,
    ip: String,
    port: u16,
    sender: EventSender<Event>,
) -> Result<Context> {
    let ctx = Txproto::new()?;

    info!("Start video, using clock {:?}", video_config.clock_type);
    configure_clock(&ctx, &video_config.clock_type)?;

    let muxer = create_muxer(&ctx, &ip, port, None)?;
    let video_pipeline = video::configure(&ctx, video_config, muxer, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        encoder: video_pipeline.encoder,
        events: Some(video_pipeline.events),
    })
}

pub(crate) fn start_audio(
    ip: String,
    port: u16,
    clock_type: ClockType,
    sender: EventSender<Event>,
) -> Result<Context> {
    let ctx = Txproto::new()?;

    info!("Start audio, using clock {:?}", clock_type);
    configure_clock(&ctx, &clock_type)?;

    let muxer = create_muxer(&ctx, &ip, port, None)?;
    let events = audio::configure(&ctx, muxer, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        encoder: None,
        events: Some(events),
    })
}
