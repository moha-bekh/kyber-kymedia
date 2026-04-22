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

use std::sync::{Arc, Mutex};

use log::{info, warn};

use kyavservice_types::{IpcApi, PipelineEvent, PipelineEventType, PipelineStage, PipelineType};
use libkypc::EventSender;
use txproto_rs::{Dictionary, IOEntryType, IOSysEntry, IOSysType, Txproto};

use crate::{Error, Event, Result};

const BITRATE: i64 = 128_000;
const FRAME_DURATION: i64 = 20;

fn plug_error_listener(
    ctx: &Txproto,
    handle: &txproto_rs::Handle,
    stage: PipelineStage,
    sender: &EventSender<IpcApi>,
) -> Result<txproto_rs::Event> {
    let sender = sender.clone();
    let error_cb = move |event| {
        info!("Audio acquisition event: {event:?}");

        sender.send(Event::PipelineEvent(PipelineEvent {
            type_: PipelineType::Audio,
            stage,
            event: PipelineEventType::Error,
        }));
    };

    let mut error_event = txproto_rs::Event::new(txproto_rs::EventType::Error, error_cb);
    ctx.register_event(handle, &mut error_event)?;

    Ok(error_event)
}

pub(crate) fn configure(
    ctx: &Txproto,
    muxer: txproto_rs::Handle,
    sender: &EventSender<IpcApi>,
) -> Result<Vec<txproto_rs::Event>> {
    let mut events = vec![];

    // Find monitor to grab
    let monitor_id = Arc::new(Mutex::new(None));
    let monitor_id_cb = monitor_id.clone();

    let io_cb = move |entry: IOSysEntry| {
        let IOEntryType::Audio { .. } = entry.entry_type else {
            return;
        };

        if entry.sys_type != IOSysType::AudioMonitor || !entry.is_default {
            return;
        }

        let Ok(mut monitor_id) = monitor_id_cb.lock() else {
            warn!("Failed to lock monitor_id_cb");
            return;
        };

        if monitor_id.is_some() {
            warn!("Found multiple default Audio monitor");
            return;
        }

        info!(
            "Found Audio monitor {} (Id: {})",
            entry.name, entry.identifier
        );

        *monitor_id = Some(entry.identifier);
    };

    let io_event = ctx.register_io_cb(io_cb, &[])?;
    drop(io_event);

    let Some(monitor_id) = *monitor_id.lock()? else {
        warn!("No default audio monitor found");
        return Err(Error::AudioNoDefaultMonitor);
    };

    // Create audio source
    info!("Start audio grab on monitor {monitor_id}");

    let options = Dictionary::from_entries(&[("buffer_ms", FRAME_DURATION).into()])?;
    let audio_source = ctx.create_io(monitor_id, Some(options))?;

    // Configure Codec
    let options = Dictionary::from_entries(&[
        ("b", BITRATE).into(),
        ("application", "audio").into(),
        ("frame_duration", FRAME_DURATION).into(),
        ("vbr", "on").into(),
        ("flags", "+global_header").into(),
    ])?;

    // Encoder
    let config = txproto_rs::EncoderConfig {
        enc_name: "libopus",
        options: Some(options),
        ..Default::default()
    };

    let encoder = ctx.create_encoder(config)?;

    // Link everything
    ctx.link(&audio_source, &encoder, 0)?;
    ctx.link(&encoder, &muxer, 0)?;

    // Plug error listeners
    // Acquisition
    let event = plug_error_listener(ctx, &audio_source, PipelineStage::Acquisition, sender)?;
    events.push(event);

    // Encoder
    let event = plug_error_listener(ctx, &encoder, PipelineStage::Encoder, sender)?;
    events.push(event);

    Ok(events)
}
