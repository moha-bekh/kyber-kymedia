// Project Kyber: display_watcher.rs
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use log::{info, warn};

use kyavservice_types::{Event, IpcApi};
use libkypc::EventSender;

use super::displays;
use crate::config::GrabBackend;

/// Cadence for `avgrab_sources_refresh` polls.  Display hot-plug is a
/// human-timescale event, so 1 s gives prompt UX without burning CPU.
const POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// Owns the watcher thread plus a cooperative shutdown flag.  The
/// owner sets `shutdown` to `true` before dropping the handle; the
/// watcher checks it after each sleep.
pub(crate) struct WatcherHandle {
    pub(crate) join: JoinHandle<()>,
    pub(crate) shutdown: Arc<AtomicBool>,
}

impl WatcherHandle {
    pub(crate) fn shutdown_and_join(self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = self.join.join();
    }
}

/// Spawn the display hot-plug watcher.  Builds its own
/// `GrabProviderCtx` (so it does not depend on any session) and emits
/// `Event::DisplayListUpdated` whenever the source set changes.
///
/// Returns `None` if the provider could not be built the caller
/// should log and proceed without hot-plug events.
pub(crate) fn spawn(
    backend: Option<GrabBackend>,
    sender: EventSender<IpcApi>,
) -> Option<WatcherHandle> {
    let provider_ctx = match displays::build_provider_ctx(backend) {
        Ok(ctx) => ctx,
        Err(err) => {
            warn!("Display hot-plug watcher: failed to build provider: {err:?}");
            return None;
        }
    };
    let initial_sources = match displays::fetch_screen_sources(&provider_ctx) {
        Ok(srcs) => srcs,
        Err(err) => {
            warn!("Display hot-plug watcher: failed to fetch sources: {err:?}");
            return None;
        }
    };

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_thread = Arc::clone(&shutdown);

    let join = thread::spawn(move || {
        // Provider must outlive the loop moved in here so the watcher owns it.
        let _provider_ctx = provider_ctx;
        let mut sources = initial_sources;

        loop {
            thread::sleep(POLL_INTERVAL);
            if shutdown_thread.load(Ordering::SeqCst) {
                return;
            }

            match sources.refresh() {
                Ok(false) => continue,
                Ok(true) => {}
                Err(err) => {
                    warn!("avgrab_sources_refresh failed: {err:?}");
                    return;
                }
            }

            let displays = displays::collect_displays(&sources);
            info!(
                "Display set changed; {} display(s) now connected",
                displays.len()
            );

            sender.send(Event::DisplayListUpdated { displays });
        }
    });

    Some(WatcherHandle { join, shutdown })
}
