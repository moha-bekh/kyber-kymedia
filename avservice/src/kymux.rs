// Project Kyber: kymux.rs
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

use log::info;

use libkypc::EventSender;
use txproto_rs::Txproto;

use crate::config::ClockType;
use crate::metrics;
use crate::{audio, configure_clock, video};
use crate::{Context, Event, Result, VideoConfig};

pub(crate) fn start_video(
    video_config: VideoConfig,
    uri: &str,
    metrics_uri: Option<&str>,
    sender: EventSender<Event>,
) -> Result<Context> {
    let ctx = Txproto::new()?;

    info!("Start video, using clock {:?}", video_config.clock_type);
    configure_clock(&ctx, &video_config.clock_type)?;

    if let Some(metrics_uri) = metrics_uri {
        metrics::forward(&ctx, metrics_uri)?;
    }

    let packet_sink = ctx.create_packet_sink(uri)?;
    let video_pipeline = video::configure(&ctx, video_config, packet_sink, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        encoder: video_pipeline.encoder,
        events: Some(video_pipeline.events),
    })
}

pub(crate) fn start_audio(
    uri: &str,
    clock_type: ClockType,
    sender: EventSender<Event>,
) -> Result<Context> {
    let ctx = Txproto::new()?;

    info!("Start audio, using clock {:?}", clock_type);
    configure_clock(&ctx, &clock_type)?;

    let packet_sink = ctx.create_packet_sink(uri)?;
    let events = audio::configure(&ctx, packet_sink, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        encoder: None,
        events: Some(events),
    })
}
