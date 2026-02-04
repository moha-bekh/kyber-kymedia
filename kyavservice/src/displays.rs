// Project Kyber: displays.rs
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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use log::warn;

use crate::{Context, Result};

use kyavservice_types::{Display, Event};
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

        let Ok(mut displays_locked) = displays.lock() else {
            warn!("Failed to lock displays");
            return;
        };

        match displays_locked.iter().position(|d| d.id == display.id) {
            Some(index) if entry.is_unplugged => {
                displays_locked.remove(index);
            }
            Some(index) => {
                displays_locked[index] = display.clone();
            }
            None => {
                displays_locked.push(display.clone());
            }
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

    let event = ctx.register_io_cb(io_cb, &[])?;

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
        encoder: None,
        events: Some(vec![event]),
    };

    Ok((ctx, displays))
}
