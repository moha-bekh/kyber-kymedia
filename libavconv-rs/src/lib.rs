// Project Kyber: lib.rs
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

use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

pub mod context;
pub mod dsink;
pub mod enc;
pub mod error;
pub mod filter;
pub mod grab;
pub mod hw;
pub mod io;
pub mod mux;
pub mod time;
pub mod watch;

pub use context::{AvconvContext, SyncQueue};
pub use dsink::{DataSink, SinkPacket};
pub use enc::{codec_context_set_option, ChannelLayout, Encoder, EncoderBuilder};
pub use error::{Error, Result};
pub use filter::{FilterGraph, FilterGraphBuilder};
pub use grab::{
    provider_by_name, providers, side_data, GrabCapture, GrabCaptureBuilder, GrabProvider,
    GrabProviderCtx, GrabSource, GrabSourceSet, GrabSourceType, StreamType,
    CAPTURE_HOOK_FLAG_IN_HEARTBEAT,
};
pub use hw::HwDeviceRef;
pub use io::AvioContext;
pub use mux::{CodecParams, Muxer, MuxerBuilder, Packet, MUXER_FLAG_OPEN_IO};
pub use watch::{ErrorWatcher, WaitOutcome};

pub use libavconv_sys as sys;

/// `avconv_connect` flag: prefer dropping older queued frames over blocking
/// the producer when the destination's input queue is full. Required for
/// live capture pipelines where stalls would back-pressure into the hw
/// frame pool. Mirrors `AVCONV_CONNECT_SEND_NONBLOCK` in `avconv.h`.
pub const CONNECT_SEND_NONBLOCK: u32 = 1;

/// Set the global FFmpeg log level.
///
/// Common values (from `libavutil/log.h`):
/// - `sys::AV_LOG_QUIET`   (-8)
/// - `sys::AV_LOG_ERROR`   (16)
/// - `sys::AV_LOG_WARNING` (24)
/// - `sys::AV_LOG_INFO`    (32)
/// - `sys::AV_LOG_VERBOSE` (40)
/// - `sys::AV_LOG_DEBUG`   (48)
pub fn set_log_level(level: i32) {
    unsafe { sys::av_log_set_level(level) }
}

/// A destination for FFmpeg/libavconv log lines.
///
/// `level` is the raw FFmpeg severity (`sys::AV_LOG_*`, lower = more severe);
/// `message` is a single, already-formatted line with its trailing newline
/// stripped. Must be a plain (non-capturing) `fn` so it can be stored behind
/// the C callback boundary, which carries no user-data pointer.
pub type LogSink = fn(level: i32, message: &str);

/// Holds the active [`LogSink`]. FFmpeg's callback is process-global with no
/// user data, so [`sink_trampoline`] reads the sink from here on every line.
static SINK: OnceLock<LogSink> = OnceLock::new();

/// C-ABI callback handed to the shim. Builds a `str` from the line + length
/// and dispatches to whatever [`LogSink`] is currently installed in [`SINK`].
///
/// # Safety
/// `line` must point to `len` valid bytes (guaranteed by the shim).
unsafe extern "C" fn sink_trampoline(level: c_int, line: *const c_char, len: usize) {
    let Some(sink) = SINK.get().copied() else {
        return;
    };
    // SAFETY: the shim guarantees `line` points to `len` initialised bytes.
    let bytes = unsafe { std::slice::from_raw_parts(line as *const u8, len) };
    let msg = String::from_utf8_lossy(bytes);
    let _ = std::panic::catch_unwind(|| sink(level, &msg));
}

/// Route all FFmpeg/libavconv log output into `sink`. Call once at startup;
/// subsequent calls keep the first installed sink. Combine with
/// [`set_log_level`] to control verbosity on the FFmpeg side before lines ever
/// reach the sink.
pub fn route_logs_to_sink(sink: LogSink) {
    let _ = SINK.set(sink);
    unsafe { sys::kyav_install_log_sink(Some(sink_trampoline)) }
}

/// Opaque scheduler handle exposed by libavconv nodes (encoder input/output,
/// filtergraph input/output, muxer input, grab capture output). Pass into
/// [`AvconvContext::connect`] to wire nodes together.
///
/// A `Handle` is logically owned by the `AvconvContext` that allocated the
/// node it points into; both remain valid for as long as that context lives.
/// Passing a handle to the wrong context is detected at the call site and
/// causes a panic (in both debug and release builds).
#[derive(Copy, Clone)]
pub struct Handle {
    ptr: *const sys::AVConvHandle,
    ctx: *mut sys::AVConvContext,
}

// SAFETY: Handle is a raw scheduler index - its validity is bound to the
// owning AvconvContext (which is Send + Sync). The Handle itself carries no
// thread-local state.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Handle {
    pub(crate) fn from_raw(ptr: *const sys::AVConvHandle, ctx: *mut sys::AVConvContext) -> Self {
        Self { ptr, ctx }
    }

    pub(crate) fn as_ptr(self) -> *const sys::AVConvHandle {
        self.ptr
    }

    pub(crate) fn ctx_ptr(self) -> *mut sys::AVConvContext {
        self.ctx
    }
}

/// Opaque handle to a registered codec (encoder or decoder) descriptor.
///
/// Codec descriptors are static C data registered at lavc init, so `Codec`
/// is `Copy` and effectively `'static`.
#[derive(Copy, Clone)]
pub struct Codec {
    ptr: *const sys::AVCodec,
}

// SAFETY: codec descriptors are static, read-only.
unsafe impl Send for Codec {}
unsafe impl Sync for Codec {}

impl Codec {
    pub(crate) fn from_raw(ptr: *const sys::AVCodec) -> Self {
        Self { ptr }
    }

    pub(crate) fn as_ptr(self) -> *const sys::AVCodec {
        self.ptr
    }

    /// Look up an encoder by its codec name (e.g. `"libx264"`, `"h264_amf"`).
    pub fn find_encoder_by_name(name: &str) -> Option<Self> {
        let cname = CString::new(name).ok()?;
        let ptr = unsafe { sys::avcodec_find_encoder_by_name(cname.as_ptr()) };
        if ptr.is_null() {
            None
        } else {
            Some(Self::from_raw(ptr))
        }
    }
}

/// Opaque handle to an `AVOutputFormat` descriptor (e.g. `"rtp"`, `"null"`).
///
/// Format descriptors are static C data registered at lavf init.
#[derive(Copy, Clone)]
pub struct OutputFormat {
    ptr: *const sys::AVOutputFormat,
}

// SAFETY: output format descriptors are static, read-only.
unsafe impl Send for OutputFormat {}
unsafe impl Sync for OutputFormat {}

impl OutputFormat {
    pub(crate) fn from_raw(ptr: *const sys::AVOutputFormat) -> Self {
        Self { ptr }
    }

    pub(crate) fn as_ptr(self) -> *const sys::AVOutputFormat {
        self.ptr
    }

    /// Guess an output format by short name (e.g. `"rtp"`, `"null"`). Returns
    /// `None` if no matching format is registered.
    pub fn guess(short_name: &str) -> Option<Self> {
        let cname = CString::new(short_name).ok()?;
        let ptr =
            unsafe { sys::av_guess_format(cname.as_ptr(), std::ptr::null(), std::ptr::null()) };
        if ptr.is_null() {
            None
        } else {
            Some(Self::from_raw(ptr))
        }
    }
}
