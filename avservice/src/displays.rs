use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use log::warn;

use crate::{Context, Result};

use avservice_types::{Display, Event};
use libkypc::EventSender;
use txproto_rs::{IOEntryType, IOSysEntry, Txproto};

pub(crate) fn enumerate(mut sender: EventSender<Event>) -> Result<(Context, Vec<Display>)> {
    let ctx = Txproto::new()?;

    let send_notifications = Arc::new(AtomicBool::new(false));
    let displays: Arc<Mutex<Vec<Display>>> = Default::default();

    // Update callback
    let send_notifications_local = send_notifications.clone();
    let displays_local = displays.clone();

    let io_cb = move |entry: IOSysEntry| {
        // Filter on displays only
        let display = match entry.entry_type {
            IOEntryType::Video {
                x,
                y,
                width,
                height,
                ..
            } => Display {
                id: entry.identifier,
                name: entry.name.clone(),
                height,
                width,
                x,
                y,
            },
            _ => {
                return;
            }
        };

        // 1. Try to find if the display is known. In this case, this is an update
        // 2. Else, store the new display
        let Ok(mut displays_locked) = displays.lock() else {
            warn!("Failed to lock displays");
            return;
        };

        let mut updated = false;
        for known_display in displays_locked.iter_mut() {
            if known_display.id == display.id {
                *known_display = display.clone();
                updated = true;
            }
        }

        if !updated {
            displays_locked.push(display.clone());
        }

        // Update notifications should only be sent when the first enumeration
        // is complete
        if send_notifications.load(Ordering::Relaxed) {
            let ret = sender.blocking_send(Event::DisplayListUpdated {
                displays: displays_locked.clone(),
            });
            if let Err(err) = ret {
                warn!("Failed to send new display list: {err:?}");
            }
        }
    };

    let event = ctx.register_io_cb(io_cb)?;

    // At this point, the enumeration is complete.
    // Any further call to io_cb will be a Display hotplug or an update about
    // a known display
    let displays = {
        let displays_locked = displays_local.lock()?;
        send_notifications_local.store(true, Ordering::Relaxed);
        displays_locked.clone()
    };

    let ctx = Context {
        ctx,
        events: Some(vec![event]),
    };

    Ok((ctx, displays))
}
