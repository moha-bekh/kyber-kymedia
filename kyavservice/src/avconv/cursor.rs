// Project Kyber: cursor.rs
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

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use log::{info, warn};

use libavconv_rs::{side_data, sys};

const PIPE_NAME: &str = r"\\.\pipe\KyberInputServer";

const CHANNEL_CAP: usize = 8;

const RECONNECT_BACKOFF: Duration = Duration::from_millis(500);

pub(crate) struct CursorSink {
    tx: SyncSender<Vec<u8>>,
    stop: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl CursorSink {
    pub(crate) fn new(display_id: u32) -> Arc<Self> {
        let (tx, rx) = sync_channel::<Vec<u8>>(CHANNEL_CAP);
        let stop = Arc::new(AtomicBool::new(false));
        let connected = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let connected_thread = Arc::clone(&connected);
        let worker = thread::Builder::new()
            .name(format!("cursor-pipe-{display_id}"))
            .spawn(move || run_worker(rx, stop_thread, connected_thread))
            .ok();
        Arc::new(Self {
            tx,
            stop,
            connected,
            worker,
        })
    }

    fn try_send(&self, bytes: Vec<u8>) -> bool {
        match self.tx.try_send(bytes) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
        }
    }

    fn connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn clear_connected(&self) {
        self.connected.store(false, Ordering::Release);
    }

    fn replay_on_connect(&self, packet: Option<&[u8]>) {
        if !self.connected() {
            return;
        }
        match packet {
            None => self.clear_connected(),
            Some(p) if self.try_send(p.to_vec()) => self.clear_connected(),
            Some(_) => {}
        }
    }
}

impl Drop for CursorSink {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let (closed_tx, _) = sync_channel::<Vec<u8>>(1);
        let _ = std::mem::replace(&mut self.tx, closed_tx);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

fn try_open_pipe() -> Option<File> {
    OpenOptions::new().write(true).open(PIPE_NAME).ok()
}

fn run_worker(rx: Receiver<Vec<u8>>, stop: Arc<AtomicBool>, connected: Arc<AtomicBool>) {
    let mut pipe: Option<File> = None;

    while !stop.load(Ordering::Relaxed) {
        if pipe.is_none() {
            if let Some(f) = try_open_pipe() {
                info!("cursor-pipe: connected to {PIPE_NAME}");
                connected.store(true, Ordering::Release);
                pipe = Some(f);
            } else {
                match rx.recv_timeout(RECONNECT_BACKOFF) {
                    Ok(_) | Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }

        let bytes = match rx.recv() {
            Ok(b) => b,
            Err(_) => return,
        };

        let f = pipe.as_mut().unwrap();
        if f.write_all(&bytes).is_err() {
            warn!("cursor-pipe: write failed, will reconnect");
            pipe = None;
        }
    }
}

pub(crate) fn cursor_hook(
    sink: Arc<CursorSink>,
    display_id: u32,
    cursor_capture_idx: u32,
    mut video_hook: impl FnMut(&mut u32, &mut sys::AVFrame, u32, &mut u32) -> i32 + Send + 'static,
) -> impl FnMut(&mut u32, &mut sys::AVFrame, u32, &mut u32) -> i32 + Send + 'static {
    let mut state = CursorState::default();

    move |stream_idx, frame, flags_in, flags_out| {
        if *stream_idx == cursor_capture_idx {
            sink.replay_on_connect(state.last_packet.as_deref());

            if let Some(bytes) = state.update(display_id, frame) {
                sink.try_send(bytes);
            }
            -libc::EAGAIN
        } else {
            video_hook(stream_idx, frame, flags_in, flags_out)
        }
    }
}

#[derive(Default)]
struct CursorState {
    started: bool,
    visible: bool,
    last_pos: (i32, i32),
    last_hotspot: (i32, i32),
    last_dims: (u32, u32),
    last_packet: Option<Vec<u8>>,
}

impl CursorState {
    fn update(&mut self, display_id: u32, frame: &sys::AVFrame) -> Option<Vec<u8>> {
        let packet = self.next_packet(display_id, frame)?;
        self.last_packet = Some(packet.clone());
        Some(packet)
    }

    fn next_packet(&mut self, display_id: u32, frame: &sys::AVFrame) -> Option<Vec<u8>> {
        let coords = side_data::coords(frame);
        let first = !std::mem::replace(&mut self.started, true);

        if coords.map_or(first, |c| c.invisible != 0) {
            let announce = self.visible || first;
            self.visible = false;
            return announce.then(|| hide_packet(display_id));
        }

        if let Some(c) = coords {
            self.last_pos = (c.x, c.y);
        }

        if let Some((dims, hotspot, rows)) = shape_of(frame) {
            self.visible = true;
            self.last_dims = dims;
            self.last_hotspot = hotspot;
            return Some(self.show_packet(display_id, Some(rows)));
        }

        if self.visible || coords.is_none() {
            return None;
        }
        self.visible = true;
        Some(self.show_packet(display_id, None))
    }

    fn show_packet(&self, display_id: u32, pixels: Option<side_data::RgbaRows<'_>>) -> Vec<u8> {
        let (x, y) = self.last_pos;
        let (xhot, yhot) = self.last_hotspot;
        let (width, height) = self.last_dims;

        let mut out: Vec<u8> = Vec::with_capacity(33);
        out.extend_from_slice(&display_id.to_be_bytes());
        out.push(1);
        out.extend_from_slice(&(x.saturating_add(xhot) as u32).to_be_bytes());
        out.extend_from_slice(&(y.saturating_add(yhot) as u32).to_be_bytes());
        out.extend_from_slice(&xhot.to_be_bytes());
        out.extend_from_slice(&yhot.to_be_bytes());
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&height.to_be_bytes());

        let Some(rows) = pixels else {
            out.extend_from_slice(&0u32.to_be_bytes());
            return out;
        };

        let count_at = out.len();
        let mut count: usize = 0;
        out.extend_from_slice(&0u32.to_be_bytes());
        out.reserve(width as usize * height as usize * 4);
        for row in rows {
            for px in row {
                count += 1;
                out.extend_from_slice(&px.to_be_bytes());
            }
        }
        out[count_at..count_at + 4].copy_from_slice(&(count as u32).to_be_bytes());

        out
    }
}

fn hide_packet(display_id: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    out.extend_from_slice(&display_id.to_be_bytes());
    out.push(0);
    out
}

type ShapeOf<'a> = ((u32, u32), (i32, i32), side_data::RgbaRows<'a>);

fn shape_of(frame: &sys::AVFrame) -> Option<ShapeOf<'_>> {
    let width = u32::try_from(frame.width).ok().filter(|w| *w > 0)?;
    let height = u32::try_from(frame.height).ok().filter(|h| *h > 0)?;
    let rows = side_data::rgba_rows(frame)?;

    let hotspot = side_data::cursor_info(frame)
        .map(|c| (c.hotspot_x(), c.hotspot_y()))
        .unwrap_or((0, 0));

    Some(((width, height), hotspot, rows))
}
