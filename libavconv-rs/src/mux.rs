// Project Kyber: mux.rs
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
use std::sync::Arc;

use crate::context::AvconvContext;
use crate::error::{check_ret, Result};
use crate::{Handle, OutputFormat};
use libavconv_sys as sys;

/// Convenience re-export pass to `MuxerBuilder::new` to make libavconv open
/// the underlying IO context automatically (e.g. RTP UDP socket).
pub const MUXER_FLAG_OPEN_IO: u32 = sys::AVConvMuxerFlags_AVCONV_MUXER_FLAG_OPEN_IO;

type StreamParamsFn =
    dyn FnMut(Option<&sys::AVConvMuxStream>, &mut sys::AVConvCodecParams) -> i32 + Send;
type PacketInFn =
    dyn FnMut(&mut sys::AVFormatContext, Option<&mut sys::AVPacket>, u32, &mut u32) -> i32 + Send;

/// Closure storage for muxer hooks. Heap-stable so the address handed to
/// libavconv via `AVConvMuxer::opaque` remains valid for the lifetime of the
/// owning [`Muxer`].
#[derive(Default)]
pub(crate) struct MuxerHookStorage {
    on_stream_params: Option<Box<StreamParamsFn>>,
    on_packet_in: Option<Box<PacketInFn>>,
}

/// In-construction muxer. Streams are added and closures registered on the
/// builder; `build()` calls `avconv_mux_init` and yields a runnable [`Muxer`]
/// whose `stream_handle()` is then valid.
pub struct MuxerBuilder {
    ptr: *mut sys::AVConvMuxer,
    hooks: Box<MuxerHookStorage>,
    _ctx: Arc<AvconvContext>,
}

unsafe impl Send for MuxerBuilder {}

impl MuxerBuilder {
    /// Allocate a new muxer.
    ///
    /// `fmt`   - output container format (from `OutputFormat::guess`).
    /// `url`   - output URL / file path.
    /// `flags` - bitmask of `AVCONV_MUXER_FLAG_*`; use
    ///           `MUXER_FLAG_OPEN_IO` to let the muxer open the IO context
    ///           automatically.
    pub fn new(ctx: Arc<AvconvContext>, fmt: OutputFormat, url: &str, flags: u32) -> Result<Self> {
        let url_c = CString::new(url)?;
        let mut mux: *mut sys::AVConvMuxer = std::ptr::null_mut();
        check_ret(unsafe {
            sys::avconv_mux_create(ctx.as_ptr(), &mut mux, fmt.as_ptr(), url_c.as_ptr(), flags)
        })?;
        Ok(Self {
            ptr: mux,
            hooks: Box::default(),
            _ctx: ctx,
        })
    }

    /// Add a stream of the given media type. Returns its index.
    pub fn add_stream(&mut self, media_type: sys::AVMediaType) -> Result<usize> {
        let idx = check_ret(unsafe { sys::avconv_mux_add_stream(self.ptr, media_type, 0) })?;
        Ok(idx as usize)
    }

    /// Number of streams currently registered with the muxer.
    pub fn nb_streams(&self) -> usize {
        unsafe { (*self.ptr).nb_streams as usize }
    }

    /// Set a string option on the muxer's `AVFormatContext` via `av_opt_set`
    /// (searches children), e.g. RTP `max_interleave_delta` or `low_latency`.
    /// Must be called before `build()`. Returns `Err` when the option is
    /// absent or rejected, so best-effort callers can log and continue.
    pub fn set_format_option(&self, key: &str, value: &str) -> Result<()> {
        let key = CString::new(key)?;
        let value = CString::new(value)?;
        check_ret(unsafe {
            sys::av_opt_set(
                (*self.ptr).fc as *mut _,
                key.as_ptr(),
                value.as_ptr(),
                sys::AV_OPT_SEARCH_CHILDREN as i32,
            )
        })?;
        Ok(())
    }

    /// Register a closure invoked once per stream when libavconv finalises
    /// codec params (called from `avconv_mux_init`).
    pub fn on_stream_params<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(Option<&sys::AVConvMuxStream>, &mut sys::AVConvCodecParams) -> i32
            + Send
            + 'static,
    {
        self.hooks.on_stream_params = Some(Box::new(f));
        self
    }

    /// Register a closure invoked for each packet entering the muxer.
    /// `pkt` is `None` on EOF.
    pub fn on_packet_in<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(&mut sys::AVFormatContext, Option<&mut sys::AVPacket>, u32, &mut u32) -> i32
            + Send
            + 'static,
    {
        self.hooks.on_packet_in = Some(Box::new(f));
        self
    }

    /// Finalise the muxer and produce a runnable [`Muxer`].
    pub fn build(mut self) -> Result<Muxer> {
        // Always wire all thunks - leaving a function-pointer slot NULL
        // would access-violate when libavconv invokes the hook. Stub thunks
        // exist for events we don't expose to closure users.
        let storage_ptr = &mut *self.hooks as *mut MuxerHookStorage;
        unsafe {
            (*self.ptr).opaque = storage_ptr as *mut std::ffi::c_void;
            // Copy the hooks struct through the existing mux->hooks pointer
            // (which points to MuxerPriv::hooks, the library's internal
            // storage). Do NOT replace mux->hooks itself; the library reads
            // mp->hooks directly and clears the public mux->hooks pointer in
            // init().
            let dst = (*self.ptr).hooks;
            if !dst.is_null() {
                *dst = sys::AVConvMuxerHooks {
                    on_packet_in: Some(on_packet_in_thunk),
                    on_packet_write: Some(on_packet_write_stub),
                    on_stream_params: Some(on_stream_params_thunk),
                    on_header: Some(on_header_stub),
                };
            }
        }

        check_ret(unsafe { sys::avconv_mux_init(self.ptr) })?;

        let MuxerBuilder { ptr, hooks, _ctx } = self;
        Ok(Muxer {
            ptr,
            _hooks: hooks,
            _ctx,
        })
    }
}

/// An initialised muxer node within an `AvconvContext`. Owned by the
/// context; freed when the context is dropped.
pub struct Muxer {
    ptr: *mut sys::AVConvMuxer,
    _hooks: Box<MuxerHookStorage>,
    _ctx: Arc<AvconvContext>,
}

// SAFETY: post-build the muxer exposes only read-only handle/count
// accessors. Internal mutation happens inside the scheduler's worker threads
// and is not observed through `&self`.
unsafe impl Send for Muxer {}
unsafe impl Sync for Muxer {}

impl Muxer {
    /// Scheduler input handle for muxer stream `idx`. Connect an encoder's
    /// output handle to this.
    pub fn stream_handle(&self, idx: usize) -> Handle {
        let n = self.nb_streams();
        assert!(
            idx < n,
            "Muxer::stream_handle: idx {idx} out of bounds (nb_streams = {n})"
        );
        Handle::from_raw(
            unsafe { (**(*self.ptr).streams.add(idx)).hdl },
            self._ctx.as_ptr(),
        )
    }

    pub fn nb_streams(&self) -> usize {
        unsafe { (*self.ptr).nb_streams as usize }
    }
}

/// Borrowed, read-only view over the `AVConvCodecParams` handed to the muxer
/// [`MuxerBuilder::on_stream_params`] hook. Encapsulates the `codec_par`
/// dereference so consumers never touch raw FFmpeg pointers.
pub struct CodecParams<'a> {
    raw: &'a sys::AVConvCodecParams,
}

impl<'a> CodecParams<'a> {
    /// Wrap the reference handed in by the `on_stream_params` hook.
    pub fn new(raw: &'a sys::AVConvCodecParams) -> Self {
        Self { raw }
    }

    /// Codec id from the underlying `AVCodecParameters`.
    pub fn codec_id(&self) -> sys::AVCodecID {
        // SAFETY: codec_par is a valid AVCodecParameters pointer owned by the
        // params struct for as long as `self` borrows it.
        unsafe { (*self.raw.codec_par).codec_id }
    }

    /// Out-of-band extradata (SPS/PPS, ...). Empty when none is set or the
    /// codec embeds its config in-band (AV1 / Opus).
    pub fn extradata(&self) -> &'a [u8] {
        // SAFETY: extradata / extradata_size describe a valid buffer owned by
        // the AVCodecParameters for the borrow of `self`.
        unsafe {
            let par = &*self.raw.codec_par;
            if par.extradata.is_null() || par.extradata_size <= 0 {
                &[]
            } else {
                std::slice::from_raw_parts(par.extradata, par.extradata_size as usize)
            }
        }
    }
}

/// Borrowed, read-only view over an `AVPacket` handed to the muxer
/// [`MuxerBuilder::on_packet_in`] hook. Encapsulates the payload-slice
/// construction so consumers never touch raw FFmpeg pointers.
pub struct Packet<'a> {
    raw: &'a sys::AVPacket,
}

impl<'a> Packet<'a> {
    /// Wrap the reference handed in by the `on_packet_in` hook.
    pub fn new(raw: &'a sys::AVPacket) -> Self {
        Self { raw }
    }

    /// Presentation timestamp in the packet's time base.
    pub fn pts(&self) -> i64 {
        self.raw.pts
    }

    /// Whether the packet carries `AV_PKT_FLAG_KEY` (keyframe / sync sample).
    pub fn is_key(&self) -> bool {
        (self.raw.flags & sys::AV_PKT_FLAG_KEY as i32) != 0
    }

    /// Encoded payload bytes. Empty when the packet carries no data.
    pub fn data(&self) -> &'a [u8] {
        // SAFETY: data / size describe a valid buffer owned by the packet for
        // the borrow of `self`.
        unsafe {
            if self.raw.data.is_null() || self.raw.size <= 0 {
                &[]
            } else {
                std::slice::from_raw_parts(self.raw.data, self.raw.size as usize)
            }
        }
    }
}

unsafe extern "C" fn on_stream_params_thunk(
    mux: *mut sys::AVConvMuxer,
    ms: *const sys::AVConvMuxStream,
    params: *mut sys::AVConvCodecParams,
) -> i32 {
    let storage = unsafe { &mut *((*mux).opaque as *mut MuxerHookStorage) };
    let Some(f) = storage.on_stream_params.as_mut() else {
        return 0;
    };
    let ms_ref = unsafe { ms.as_ref() };
    crate::error::catch_hook_panic(|| f(ms_ref, unsafe { &mut *params }))
}

unsafe extern "C" fn on_packet_in_thunk(
    mux: *mut sys::AVConvMuxer,
    fc: *mut sys::AVFormatContext,
    pkt: *mut sys::AVPacket,
    flags_in: u32,
    flags_out: *mut u32,
) -> i32 {
    let known_flags_in = sys::AVConvMuxHookFlagsIn_AVCONV_MUX_HOOK_FLAG_IN_EOF
        | sys::AVConvMuxHookFlagsIn_AVCONV_MUX_HOOK_FLAG_IN_REPEAT;
    if flags_in & !known_flags_in != 0 {
        return -libc::ENOSYS;
    }
    let storage = unsafe { &mut *((*mux).opaque as *mut MuxerHookStorage) };
    let Some(f) = storage.on_packet_in.as_mut() else {
        return 0;
    };
    let pkt_ref = if flags_in & sys::AVConvMuxHookFlagsIn_AVCONV_MUX_HOOK_FLAG_IN_EOF != 0 {
        None
    } else {
        Some(unsafe { &mut *pkt })
    };
    crate::error::catch_hook_panic(|| {
        f(unsafe { &mut *fc }, pkt_ref, flags_in, unsafe {
            &mut *flags_out
        })
    })
}

// Stub thunks for events not exposed via closures.
unsafe extern "C" fn on_packet_write_stub(
    _mux: *mut sys::AVConvMuxer,
    _fc: *mut sys::AVFormatContext,
    _pkt: *mut sys::AVPacket,
    _flags_in: u32,
    _flags_out: *mut u32,
) -> i32 {
    0
}

unsafe extern "C" fn on_header_stub(
    _mux: *mut sys::AVConvMuxer,
    _fc: *mut sys::AVFormatContext,
) -> i32 {
    0
}
