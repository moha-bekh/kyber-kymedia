// Project Kyber: watch.rs
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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::context::AvconvContext;
use crate::dsink::{DataSink, SinkPacket};
use crate::error::AVERROR_EOF;
use libavconv_sys as sys;

/// 1-second polling deadline short enough for `ErrorWatcher::Drop` to observe
/// the watcher exit promptly, long enough to keep wakeup overhead negligible.
const WAIT_TICK_US: i64 = 1_000_000;

/// `avconv_wait` returns `-ETIMEDOUT` on tick.
const ETIMEDOUT_AVERROR: i32 = -libc::ETIMEDOUT;

/// Outcome reported to the [`AvconvContext::watch_errors`] callback when
/// `avconv_wait` returns something other than ETIMEDOUT.
#[derive(Debug, Clone, Copy)]
pub enum WaitOutcome {
    /// Pipeline ended cleanly (e.g. via `ctx.stop()` or natural EOF).
    Eof,
    /// Pipeline reported a runtime failure. Inner value is the raw AVERROR.
    Error(i32),
}

/// Handle to a background thread polling `avconv_wait` for pipeline outcomes.
///
/// Drops cleanly: signals cooperative shutdown and joins the thread. The
/// polling thread holds an `Arc<AvconvContext>` so the context remains alive
/// for at least as long as this watcher.
pub struct ErrorWatcher {
    join: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

impl ErrorWatcher {
    /// Signal cooperative shutdown idempotent. Call this *before* invoking
    /// `ctx.stop()` if you want to suppress the watcher callback that would
    /// otherwise fire from the forced teardown.
    pub fn signal_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

impl Drop for ErrorWatcher {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Drains whatever pending data `avconv_wait` reported. `Ok(true)` means the
/// pipeline may produce more, `Ok(false)` means end-of-stream, `Err` carries a
/// raw AVERROR from the consumer.
type Drain = Box<dyn FnMut() -> std::result::Result<bool, i32> + Send>;

impl AvconvContext {
    /// Spawn a background thread that polls `avconv_wait` and invokes
    /// `on_outcome` whenever the pipeline reports a terminal state (EOF or
    /// runtime error). The callback is invoked at most once.
    ///
    /// Takes `Arc<Self>` so the context is guaranteed to remain alive for the
    /// lifetime of the polling thread - no manual drop-ordering required.
    ///
    /// Only valid for a pipeline with no data sources and no retrieving data
    /// sinks; those need [`Self::watch_packet_sink`], which passes the progress
    /// struct `avconv_wait` demands when caller-consumable events can occur.
    pub fn watch_errors<F>(self: Arc<Self>, on_outcome: F) -> ErrorWatcher
    where
        F: FnMut(WaitOutcome) + Send + 'static,
    {
        self.spawn_watcher(None, on_outcome)
    }

    /// Spawn a background thread that polls `avconv_wait` and, whenever the
    /// pipeline signals that `sink` has data, hands each item to `on_item`.
    /// Terminal states go to `on_outcome` exactly as in [`Self::watch_errors`];
    /// an `Err(averror)` from `on_item` is reported as
    /// [`WaitOutcome::Error`] and ends the watch.
    ///
    /// `sink` must be drained by exactly one watcher: the progress struct is
    /// owned by the context and invalidated by the next call on it, so two
    /// threads polling the same context would race.
    pub fn watch_packet_sink<F, G>(
        self: Arc<Self>,
        sink: Arc<DataSink>,
        mut on_item: F,
        on_outcome: G,
    ) -> ErrorWatcher
    where
        F: FnMut(SinkPacket<'_>) -> std::result::Result<(), i32> + Send + 'static,
        G: FnMut(WaitOutcome) + Send + 'static,
    {
        let drain: Drain = Box::new(move || sink.drain_packets(&mut on_item));
        self.spawn_watcher(Some(drain), on_outcome)
    }

    fn spawn_watcher<F>(
        self: Arc<Self>,
        mut drain: Option<Drain>,
        mut on_outcome: F,
    ) -> ErrorWatcher
    where
        F: FnMut(WaitOutcome) + Send + 'static,
    {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_thread = Arc::clone(&shutdown);
        let ctx = Arc::clone(&self);

        let join = thread::spawn(move || {
            let ptr = ctx.as_ptr();
            // libavconv rejects a NULL progress pointer as soon as the graph
            // holds a node whose events the caller must service; it is
            // write-only for us, since a single sink is drained unconditionally
            // rather than by looking up its handle in the reported array.
            let mut progress: *const sys::AVConvProgress = std::ptr::null();
            let progress_ptr = if drain.is_some() {
                &mut progress as *mut _
            } else {
                std::ptr::null_mut()
            };

            loop {
                if shutdown_thread.load(Ordering::SeqCst) {
                    return;
                }

                let deadline = unsafe { sys::av_gettime() } + WAIT_TICK_US;
                let ret = unsafe { sys::avconv_wait(ptr, deadline, 0, progress_ptr) };

                if ret == ETIMEDOUT_AVERROR {
                    continue;
                }
                if shutdown_thread.load(Ordering::SeqCst) {
                    return;
                }
                if ret >= 0 {
                    // An event needing caller action; only ever reported when a
                    // retrieving sink or source exists, i.e. when `drain` is set.
                    match drain.as_mut().map(|d| d()) {
                        Some(Ok(true)) | None => continue,
                        Some(Ok(false)) => {
                            on_outcome(WaitOutcome::Eof);
                            return;
                        }
                        Some(Err(code)) => {
                            on_outcome(WaitOutcome::Error(code));
                            return;
                        }
                    }
                }
                if ret == AVERROR_EOF {
                    on_outcome(WaitOutcome::Eof);
                    return;
                }

                on_outcome(WaitOutcome::Error(ret));
                return;
            }
        });

        ErrorWatcher {
            join: Some(join),
            shutdown,
        }
    }
}
