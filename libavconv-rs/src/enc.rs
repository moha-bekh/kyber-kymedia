// Project Kyber: enc.rs
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
use crate::{Codec, Handle};
use libavconv_sys as sys;

/// Constructors for `AVChannelLayout` values, so callers don't have to
/// hand-zero and fill the FFI struct themselves.
pub struct ChannelLayout;

impl ChannelLayout {
    /// Native-order stereo (`AV_CHANNEL_LAYOUT_STEREO`): 2 channels, mask
    /// `FL|FR`. Pass to [`EncoderBuilder::set_channel_layout`].
    pub fn stereo() -> sys::AVChannelLayout {
        // SAFETY: AVChannelLayout is plain data; zeroing then filling the
        // NATIVE-order fields reproduces FFmpeg's AV_CHANNEL_LAYOUT_STEREO
        // initializer. Writing the union field `u.mask` is sound.
        let mut layout = unsafe { std::mem::zeroed::<sys::AVChannelLayout>() };
        layout.order = sys::AVChannelOrder_AV_CHANNEL_ORDER_NATIVE;
        layout.nb_channels = 2;
        layout.u.mask = 0x3; // AV_CH_FRONT_LEFT | AV_CH_FRONT_RIGHT
        layout
    }
}

/// Set a string option on an `AVCodecContext` from inside an encoder hook
/// callback (e.g. `on_stream_params`). Searches children, so private codec
/// options such as `forced-idr` are resolved on `enc_ctx->priv_data`.
///
/// Mirrors [`EncoderBuilder::set_option`] for the post-build codec context the
/// hook receives.
pub fn codec_context_set_option(
    ctx: &mut sys::AVCodecContext,
    key: &str,
    value: &str,
) -> Result<()> {
    let key = CString::new(key)?;
    let value = CString::new(value)?;
    check_ret(unsafe {
        sys::av_opt_set(
            (ctx as *mut sys::AVCodecContext) as *mut _,
            key.as_ptr(),
            value.as_ptr(),
            sys::AV_OPT_SEARCH_CHILDREN as i32,
        )
    })?;
    Ok(())
}

type FrameInFn =
    dyn FnMut(&mut sys::AVCodecContext, Option<&mut sys::AVFrame>, u32, &mut u32) -> i32 + Send;
type PacketOutFn =
    dyn FnMut(&mut sys::AVCodecContext, Option<&mut sys::AVPacket>, u32, &mut u32) -> i32 + Send;
type StreamParamsFn =
    dyn FnMut(&mut sys::AVCodecContext, &mut sys::AVConvFrameParams, u32) -> i32 + Send;

/// Closure storage for encoder hooks. The struct sits in a `Box` whose
/// address is handed to libavconv via `AVConvEncoder::opaque`; the boxed
/// heap allocation is stable, so the pointer remains valid for the lifetime
/// of the owning `Encoder`.
#[derive(Default)]
pub(crate) struct EncoderHookStorage {
    on_frame_in: Option<Box<FrameInFn>>,
    on_packet_out: Option<Box<PacketOutFn>>,
    on_stream_params: Option<Box<StreamParamsFn>>,
}

/// In-construction encoder. Holds the scheduler's encoder node pre-init
/// option setters and closure-based hook setters live here.
pub struct EncoderBuilder {
    ptr: *mut sys::AVConvEncoder,
    hooks: Box<EncoderHookStorage>,
    _ctx: Arc<AvconvContext>,
}

unsafe impl Send for EncoderBuilder {}

impl EncoderBuilder {
    /// Allocate a new encoder node for `codec`.
    pub fn new(ctx: Arc<AvconvContext>, codec: Codec) -> Result<Self> {
        let mut enc: *mut sys::AVConvEncoder = std::ptr::null_mut();
        check_ret(unsafe { sys::avconv_enc_create(ctx.as_ptr(), &mut enc, codec.as_ptr(), 0) })?;
        Ok(Self {
            ptr: enc,
            hooks: Box::new(EncoderHookStorage::default()),
            _ctx: ctx,
        })
    }

    /// Set the encoder target bitrate in bits per second.
    pub fn set_bit_rate(&self, bit_rate: i64) {
        unsafe {
            (*(*self.ptr).enc_ctx).bit_rate = bit_rate;
        }
    }

    /// Set the encoder's `AV_CODEC_FLAG_GLOBAL_HEADER` bit. Required when the
    /// downstream muxer expects extradata out-of-band (e.g. RTP, ISOBMFF).
    pub fn enable_global_header(&self) {
        unsafe {
            (*(*self.ptr).enc_ctx).flags |= sys::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }
    }

    /// Set the encoder's expected input sample rate (audio encoders).
    pub fn set_sample_rate(&self, sample_rate: i32) {
        unsafe {
            (*(*self.ptr).enc_ctx).sample_rate = sample_rate;
        }
    }

    /// Set the encoder's expected input sample format (audio encoders).
    pub fn set_sample_fmt(&self, sample_fmt: sys::AVSampleFormat) {
        unsafe {
            (*(*self.ptr).enc_ctx).sample_fmt = sample_fmt;
        }
    }

    /// Set the encoder's expected input channel layout (audio encoders).
    /// Ownership of `layout` is moved into the encoder context.
    pub fn set_channel_layout(&self, layout: sys::AVChannelLayout) {
        unsafe {
            (*(*self.ptr).enc_ctx).ch_layout = layout;
        }
    }

    /// Set a single string option on `enc_ctx` using `av_opt_set`.
    pub fn set_option(&self, key: &str, value: &str) -> Result<()> {
        let key = CString::new(key)?;
        let value = CString::new(value)?;
        check_ret(unsafe {
            sys::av_opt_set(
                (*self.ptr).enc_ctx as *mut _,
                key.as_ptr(),
                value.as_ptr(),
                sys::AV_OPT_SEARCH_CHILDREN as i32,
            )
        })?;
        Ok(())
    }

    /// Parse and apply a comma-separated `key=value` option string to `enc_ctx`.
    pub fn set_options_string(&self, opts: &str) -> Result<()> {
        let opts = CString::new(opts)?;
        let sep_kv = b'=' as libc::c_char;
        let sep_pairs = b',' as libc::c_char;
        check_ret(unsafe {
            sys::av_set_options_string(
                (*self.ptr).enc_ctx as *mut _,
                opts.as_ptr(),
                &sep_kv,
                &sep_pairs,
            )
        })?;
        Ok(())
    }

    /// Scheduler input handle. Available before init so that callers can
    /// register the encoder with a sync queue or wire upstream nodes.
    pub fn handle_in(&self) -> Handle {
        Handle::from_raw(unsafe { (*self.ptr).hdl_in }, self._ctx.as_ptr())
    }

    /// Scheduler output handle. Available before init for connect()ing the
    /// encoder to downstream nodes.
    pub fn handle_out(&self) -> Handle {
        Handle::from_raw(unsafe { (*self.ptr).hdl_out }, self._ctx.as_ptr())
    }

    /// Register a closure invoked when a frame enters the encoder. `frame`
    /// is `None` on EOS.
    pub fn on_frame_in<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(&mut sys::AVCodecContext, Option<&mut sys::AVFrame>, u32, &mut u32) -> i32
            + Send
            + 'static,
    {
        self.hooks.on_frame_in = Some(Box::new(f));
        self
    }

    /// Register a closure invoked when a packet leaves the encoder. `pkt`
    /// is `None` on EOS.
    pub fn on_packet_out<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(&mut sys::AVCodecContext, Option<&mut sys::AVPacket>, u32, &mut u32) -> i32
            + Send
            + 'static,
    {
        self.hooks.on_packet_out = Some(Box::new(f));
        self
    }

    /// Register a closure invoked when the encoder announces stream params
    /// downstream.
    pub fn on_stream_params<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(&mut sys::AVCodecContext, &mut sys::AVConvFrameParams, u32) -> i32
            + Send
            + 'static,
    {
        self.hooks.on_stream_params = Some(Box::new(f));
        self
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::AVConvEncoder {
        self.ptr
    }

    /// Finish encoder setup and produce a runnable [`Encoder`].
    pub fn build(mut self) -> Result<Encoder> {
        // Always wire all thunks regardless of which closures the caller set
        // - each thunk checks its own slot and returns 0 when no closure is
        // present. libavconv may call any hook unconditionally; leaving a
        // function-pointer slot NULL would access-violate.
        let storage_ptr = &mut *self.hooks as *mut EncoderHookStorage;
        unsafe {
            (*self.ptr).opaque = storage_ptr as *mut std::ffi::c_void;
            let dst = (*self.ptr).hooks;
            if !dst.is_null() {
                *dst = sys::AVConvEncoderHooks {
                    on_frame_in: Some(enc_on_frame_in_thunk),
                    on_packet_out: Some(enc_on_packet_out_thunk),
                    on_stream_params: Some(enc_on_stream_params_thunk),
                };
            }
        }

        check_ret(unsafe { sys::avconv_enc_init(self.ptr) })?;

        let EncoderBuilder { ptr, hooks, _ctx } = self;
        Ok(Encoder {
            ptr,
            _hooks: hooks,
            _ctx,
        })
    }
}

/// Initialised encoder node. Setup-phase methods (option setters, hook
/// installation) are not available here use [`EncoderBuilder`] to configure
/// before calling `build()`.
pub struct Encoder {
    ptr: *mut sys::AVConvEncoder,
    _hooks: Box<EncoderHookStorage>,
    _ctx: Arc<AvconvContext>,
}

// SAFETY: post-build the encoder exposes only read-only handle accessors
// (`handle_in`, `handle_out`). The underlying `AVConvEncoder` is mutated by
// the scheduler from its own worker threads, but those mutations are
// internal to the C side and not observed through `&self` here.
unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}

impl Encoder {
    /// Scheduler input handle connect an upstream output to this.
    pub fn handle_in(&self) -> Handle {
        Handle::from_raw(unsafe { (*self.ptr).hdl_in }, self._ctx.as_ptr())
    }

    /// Scheduler output handle connect this to a downstream input.
    pub fn handle_out(&self) -> Handle {
        Handle::from_raw(unsafe { (*self.ptr).hdl_out }, self._ctx.as_ptr())
    }
}

unsafe extern "C" fn enc_on_frame_in_thunk(
    enc: *mut sys::AVConvEncoder,
    enc_ctx: *mut sys::AVCodecContext,
    frame: *mut sys::AVFrame,
    flags_in: u32,
    flags_out: *mut u32,
) -> i32 {
    let known_flags_in = sys::AVConvEncHookFlagsIn_AVCONV_ENC_HOOK_FLAG_IN_EOS
        | sys::AVConvEncHookFlagsIn_AVCONV_ENC_HOOK_FLAG_IN_REPEAT;
    if flags_in & !known_flags_in != 0 {
        return -libc::ENOSYS;
    }
    let storage = unsafe { &mut *((*enc).opaque as *mut EncoderHookStorage) };
    let Some(f) = storage.on_frame_in.as_mut() else {
        return 0;
    };
    let frame_ref = if flags_in & sys::AVConvEncHookFlagsIn_AVCONV_ENC_HOOK_FLAG_IN_EOS != 0 {
        None
    } else {
        Some(unsafe { &mut *frame })
    };
    f(unsafe { &mut *enc_ctx }, frame_ref, flags_in, unsafe {
        &mut *flags_out
    })
}

unsafe extern "C" fn enc_on_packet_out_thunk(
    enc: *mut sys::AVConvEncoder,
    enc_ctx: *mut sys::AVCodecContext,
    pkt: *mut sys::AVPacket,
    flags_in: u32,
    flags_out: *mut u32,
) -> i32 {
    // Reject flags this wrapper doesn't know about instead of silently
    // misinterpreting them: a future libavconv may add IN flags that change
    // packet semantics, and we'd rather fail loudly than mishandle them.
    let known_flags_in = sys::AVConvEncHookFlagsIn_AVCONV_ENC_HOOK_FLAG_IN_EOS
        | sys::AVConvEncHookFlagsIn_AVCONV_ENC_HOOK_FLAG_IN_REPEAT;
    if flags_in & !known_flags_in != 0 {
        return -libc::ENOSYS;
    }
    let storage = unsafe { &mut *((*enc).opaque as *mut EncoderHookStorage) };
    let Some(f) = storage.on_packet_out.as_mut() else {
        return 0;
    };
    let pkt_ref = if flags_in & sys::AVConvEncHookFlagsIn_AVCONV_ENC_HOOK_FLAG_IN_EOS != 0 {
        None
    } else {
        Some(unsafe { &mut *pkt })
    };
    f(unsafe { &mut *enc_ctx }, pkt_ref, flags_in, unsafe {
        &mut *flags_out
    })
}

unsafe extern "C" fn enc_on_stream_params_thunk(
    enc: *mut sys::AVConvEncoder,
    enc_ctx: *mut sys::AVCodecContext,
    par: *mut sys::AVConvFrameParams,
    flags: u32,
) -> i32 {
    if flags != 0 {
        return -libc::ENOSYS;
    }
    let storage = unsafe { &mut *((*enc).opaque as *mut EncoderHookStorage) };
    let Some(f) = storage.on_stream_params.as_mut() else {
        return 0;
    };
    f(unsafe { &mut *enc_ctx }, unsafe { &mut *par }, flags)
}
