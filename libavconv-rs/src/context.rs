// Project Kyber: context.rs
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

use crate::error::{check_ret, Result};
use crate::{AvioContext, Handle};
use libavconv_sys as sys;

pub struct AvconvContext {
    ptr: *mut sys::AVConvContext,
}

// SAFETY: libavconv guards lifecycle state with internal mutexes
// (`schedule_lock`, `finish_lock`, `mux_ready_lock` in sched.c). The runtime
// methods (`start`, `wait`, `stop`) are explicitly designed to be callable
// concurrently from the scheduler's worker threads.
unsafe impl Send for AvconvContext {}
unsafe impl Sync for AvconvContext {}

impl AvconvContext {
    pub fn new() -> Result<Self> {
        let ptr = unsafe { sys::avconv_alloc() };
        if ptr.is_null() {
            Err(crate::error::Error::AllocFailed)
        } else {
            Ok(Self { ptr })
        }
    }

    /// Raw pointer for FFI plumbing inside this crate.
    pub(crate) fn as_ptr(&self) -> *mut sys::AVConvContext {
        self.ptr
    }

    /// Create a pre-encoding sync queue and return a handle for binding
    /// encoders to it. Required for encoders that report a fixed frame size
    /// once opened (e.g. Opus) - without it the scheduler asserts on
    /// `enc->sq_idx[0] >= 0`.
    pub fn add_sync_queue(&self, buf_size_us: u64) -> Result<SyncQueue<'_>> {
        let mut sq_hdl: *const sys::AVConvHandle = std::ptr::null();
        check_ret(unsafe {
            sys::avconv_add_sq_enc(self.ptr, buf_size_us, std::ptr::null_mut(), &mut sq_hdl)
        })?;
        Ok(SyncQueue {
            ctx: self,
            hdl: Handle::from_raw(sq_hdl, self.ptr),
        })
    }

    pub fn connect(&self, src: Handle, dst: Handle, flags: u32) -> Result<()> {
        assert_eq!(
            src.ctx_ptr(),
            self.ptr,
            "src handle belongs to a different AvconvContext"
        );
        assert_eq!(
            dst.ctx_ptr(),
            self.ptr,
            "dst handle belongs to a different AvconvContext"
        );
        check_ret(unsafe { sys::avconv_connect(self.ptr, src.as_ptr(), dst.as_ptr(), flags) })?;
        Ok(())
    }

    /// Configure the input queue size of a node. Must be called after the
    /// node is created and before `start()`. A smaller queue keeps fewer
    /// frames in flight - combine with [`CONNECT_SEND_NONBLOCK`](crate::CONNECT_SEND_NONBLOCK)
    /// on the upstream `connect` to drop late frames instead of stalling
    /// the producer.
    pub fn set_node_queue_size(&self, hdl: Handle, queue_size: u32) -> Result<()> {
        assert_eq!(
            hdl.ctx_ptr(),
            self.ptr,
            "handle belongs to a different AvconvContext"
        );
        check_ret(unsafe { sys::avconv_node_queue_size(self.ptr, hdl.as_ptr(), queue_size, 0) })?;
        Ok(())
    }

    /// Start the transcoding pipeline.
    pub fn start(&self) -> Result<()> {
        check_ret(unsafe { sys::avconv_start(self.ptr, 0) })?;
        Ok(())
    }

    /// Stop the transcoding pipeline.
    pub fn stop(&self) -> Result<()> {
        check_ret(unsafe { sys::avconv_stop(self.ptr, std::ptr::null_mut()) })?;
        Ok(())
    }

    /// Request that libavconv writes the SDP into `io` once the RTP muxer(s)
    /// have received their stream parameters. Must be called **before**
    /// `start()`. Consumes the [`AvioContext`] - libavconv takes ownership
    /// and closes it after writing.
    pub fn request_sdp(&self, io: AvioContext) -> Result<()> {
        let mut ptr = io.into_raw();
        check_ret(unsafe { sys::avconv_sdp(self.ptr, &mut ptr) })?;
        Ok(())
    }
}

impl Drop for AvconvContext {
    fn drop(&mut self) {
        unsafe { sys::avconv_free(&mut self.ptr) };
    }
}

/// Handle to a pre-encoding sync queue created via
/// [`AvconvContext::add_sync_queue`]. The queue itself is owned by the
/// context; this struct just carries the handle for `add_encoder()` calls.
pub struct SyncQueue<'ctx> {
    ctx: &'ctx AvconvContext,
    hdl: Handle,
}

impl SyncQueue<'_> {
    /// Bind an encoder to this sync queue. `limiting=true` marks the stream
    /// as the SQ's limiting stream (no other stream may run longer);
    /// `max_frames` terminates the stream after that many frames
    /// (`u64::MAX` for "no limit").
    pub fn add_encoder(&self, enc_handle: Handle, limiting: bool, max_frames: u64) -> Result<()> {
        assert_eq!(
            enc_handle.ctx_ptr(),
            self.ctx.ptr,
            "enc_handle belongs to a different AvconvContext"
        );
        check_ret(unsafe {
            sys::avconv_sq_add_enc(
                self.ctx.ptr,
                self.hdl.as_ptr(),
                enc_handle.as_ptr(),
                limiting as i32,
                max_frames,
            )
        })?;
        Ok(())
    }
}
