// Project Kyber: dsink.rs
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

use crate::context::AvconvContext;
use crate::error::{check_ret, Result, AVERROR_EAGAIN, AVERROR_EOF};
use crate::mux::{CodecParams, Packet};
use crate::Handle;
use libavconv_sys as sys;

/// One item retrieved from a coded-payload [`DataSink`]. Mirrors the
/// `AVConvDSinkDataPacket` tagged union.
pub enum SinkPacket<'a> {
    /// Coded stream parameters, delivered before the first packet and again
    /// whenever they change. The libavconv equivalent of a muxer's
    /// `on_stream_params` hook.
    Params(CodecParams<'a>),
    /// An encoded packet.
    Packet(Packet<'a>),
    /// An assertion that no new data exists up to the carried pts. Carries no
    /// payload.
    Heartbeat,
}

/// A data sink node: the pure-sink terminator libavconv requires at the end of
/// every branch of the processing graph.
///
/// Two flavours:
/// - [`DataSink::null`] discards its input, giving a hook-consumed branch an
///   endpoint without producing any events;
/// - [`DataSink::packets`] hands encoded packets back to the caller, which must
///   drain it - see [`AvconvContext::watch_packet_sink`].
///
/// Owned by the [`AvconvContext`] it is created in; freed with the context.
pub struct DataSink {
    ptr: *mut sys::AVConvDSink,
    _ctx: Arc<AvconvContext>,
}

// SAFETY: a null sink exposes only its (immutable) input handle. All state
// mutation happens on the scheduler's sink thread and is never observed
// through `&self`.
unsafe impl Send for DataSink {}
unsafe impl Sync for DataSink {}

impl DataSink {
    /// Create a sink that discards everything it receives.
    ///
    /// Use it to terminate a node output whose data is consumed elsewhere -
    /// typically by a hook that returns `AVERROR(EAGAIN)` - but which still
    /// needs a destination for `avconv_start()`'s topology check to pass.
    ///
    /// `payload` selects frames (`AVCONV_HANDLE_PAYLOAD_FRAME`) or packets
    /// (`AVCONV_HANDLE_PAYLOAD_PACKET`) and must match the upstream output.
    ///
    /// A null sink reports no events through `avconv_wait()`, so it is safe to
    /// combine with [`AvconvContext::watch_errors`].
    pub fn null(
        ctx: Arc<AvconvContext>,
        payload: sys::AVConvHandlePayload,
        media_type: sys::AVMediaType,
    ) -> Result<Self> {
        Self::create(
            ctx,
            payload,
            media_type,
            sys::AVConvDSinkFlags_AVCONV_DSINK_FLAG_NULL,
        )
    }

    /// Create a sink that hands encoded packets back to the caller.
    ///
    /// The sink holds one item at a time and blocks its producer until that
    /// item is taken, so a slow consumer back-pressures the pipeline rather
    /// than queueing without bound. It must therefore be drained, which
    /// [`AvconvContext::watch_packet_sink`] does; leaving it unread stalls
    /// everything upstream.
    pub fn packets(ctx: Arc<AvconvContext>, media_type: sys::AVMediaType) -> Result<Self> {
        Self::create(
            ctx,
            sys::AVConvHandlePayload_AVCONV_HANDLE_PAYLOAD_PACKET,
            media_type,
            0,
        )
    }

    fn create(
        ctx: Arc<AvconvContext>,
        payload: sys::AVConvHandlePayload,
        media_type: sys::AVMediaType,
        flags: u32,
    ) -> Result<Self> {
        let mut sink: *mut sys::AVConvDSink = std::ptr::null_mut();
        check_ret(unsafe {
            sys::avconv_dsink_create(ctx.as_ptr(), &mut sink, payload, media_type, flags)
        })?;
        check_ret(unsafe { sys::avconv_dsink_init(sink) })?;
        Ok(Self {
            ptr: sink,
            _ctx: ctx,
        })
    }

    /// Scheduler input handle. Connect the producing node's output to this.
    pub fn handle(&self) -> Handle {
        Handle::from_raw(unsafe { (*self.ptr).hdl }, self._ctx.as_ptr())
    }

    /// Take every item currently available, passing each to `on_item`.
    ///
    /// Returns `Ok(false)` once the sink has reached end-of-stream (no further
    /// data will ever arrive), `Ok(true)` when it merely ran dry for now. An
    /// `Err` from `on_item` carries a raw AVERROR code and aborts the drain.
    ///
    /// Only meaningful for a sink built by [`DataSink::packets`]; a null sink
    /// always reports EOF-less emptiness.
    pub(crate) fn drain_packets<F>(&self, mut on_item: F) -> std::result::Result<bool, i32>
    where
        F: FnMut(SinkPacket<'_>) -> std::result::Result<(), i32>,
    {
        loop {
            let mut data: *mut sys::AVConvDSinkDataPacket = std::ptr::null_mut();
            let ret = unsafe { sys::avconv_dsink_receive_packet(self.ptr, &mut data, 0) };
            match ret {
                AVERROR_EAGAIN => return Ok(true),
                AVERROR_EOF => return Ok(false),
                r if r < 0 => return Err(r),
                _ => {}
            }

            // SAFETY: a successful receive yields a refstruct reference we now
            // own; the borrowed view handed to `on_item` cannot outlive it
            // because the callback takes it by value for the call only.
            let res = unsafe { on_item(sink_packet(&*data)) };
            unsafe {
                sys::av_refstruct_unref(&mut data as *mut *mut _ as *mut libc::c_void);
            }
            res?;
        }
    }
}

/// Project the tagged union onto a borrowed view.
///
/// # Safety
/// `data`'s active union member must match its `type_`, which libavconv
/// guarantees for anything returned by `avconv_dsink_receive_packet()`.
unsafe fn sink_packet(data: &sys::AVConvDSinkDataPacket) -> SinkPacket<'_> {
    match data.type_ {
        sys::AVConvDSinkTypePacket_AVCONV_DSINK_PACKET => {
            SinkPacket::Packet(Packet::new(unsafe { &*data.__bindgen_anon_1.pkt }))
        }
        sys::AVConvDSinkTypePacket_AVCONV_DSINK_PACKET_PARAMS => {
            SinkPacket::Params(CodecParams::new(unsafe { &*data.__bindgen_anon_1.par }))
        }
        // AVCONV_DSINK_PACKET_HEARTBEAT, plus any type added in the future:
        // both carry nothing a consumer of this crate can act on.
        _ => SinkPacket::Heartbeat,
    }
}
