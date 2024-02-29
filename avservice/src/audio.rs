use std::sync::{Arc, Mutex};

use log::{info, warn};

use libkypc::EventSender;
use txproto_rs::{Dictionary, IOEntryType, IOSysEntry, IOSysType, Txproto};

use crate::{Error, Event, Result};

const BITRATE: i64 = 128_000;
const FRAME_DURATION: i64 = 20;

pub(crate) fn configure(
    ctx: &Txproto,
    muxer: txproto_rs::Handle,
    sender: Option<EventSender<Event>>,
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

    let io_event = ctx.register_io_cb(io_cb)?;
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

    Ok(events)
}
