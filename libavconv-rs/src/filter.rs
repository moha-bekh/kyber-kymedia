// Project Kyber: filter.rs
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
use crate::enc::EncoderBuilder;
use crate::error::{check_ret, Result};
use crate::hw::HwDeviceRef;
use crate::Handle;
use libavconv_sys as sys;

type FrameInFn = dyn FnMut(Option<&mut sys::AVFrame>, u32, &mut u32) -> i32 + Send;

/// Closure storage for filter-graph hooks. The boxed allocation is stable
/// across moves so the address handed to libavconv via
/// `AVConvFilterGraph::opaque` stays valid for the lifetime of the
/// owning [`FilterGraph`].
#[derive(Default)]
pub(crate) struct FilterGraphHookStorage {
    on_frame_in: Option<Box<FrameInFn>>,
}

/// In-construction filtergraph. Pre-init configuration (output-params binding,
/// hook closures) lives on the builder; `build()` calls `avconv_fg_init` and
/// yields a runnable [`FilterGraph`].
pub struct FilterGraphBuilder {
    ptr: *mut sys::AVConvFilterGraph,
    hooks: Box<FilterGraphHookStorage>,
    _ctx: Arc<AvconvContext>,
}

unsafe impl Send for FilterGraphBuilder {}

impl FilterGraphBuilder {
    /// Allocate a new filtergraph.
    ///
    /// `graph_desc` lavfi filter graph description (e.g. `"null"`,
    ///   `"hwdownload,format=bgra"`, `"scale_vaapi=format=nv12"`).
    ///
    /// `hw_device` optional reference to a hardware device context. The
    ///   function takes its own `av_buffer_ref` internally; the caller keeps
    ///   their reference.
    pub fn new(
        ctx: Arc<AvconvContext>,
        graph_desc: &str,
        hw_device: Option<&HwDeviceRef>,
    ) -> Result<Self> {
        let desc = CString::new(graph_desc)?;
        let mut fg: *mut sys::AVConvFilterGraph = std::ptr::null_mut();
        let hw_ptr = hw_device.map_or(std::ptr::null_mut(), |hw| hw.as_ptr());
        check_ret(unsafe {
            sys::avconv_fg_create(ctx.as_ptr(), &mut fg, desc.as_ptr(), hw_ptr, 0)
        })?;
        Ok(Self {
            ptr: fg,
            hooks: Box::new(FilterGraphHookStorage::default()),
            _ctx: ctx,
        })
    }

    /// Scheduler input handle for filtergraph input `idx`. Available pre-init
    /// for wiring with [`AvconvContext::connect`].
    pub fn input_handle(&self, idx: usize) -> Handle {
        let n = self.nb_inputs();
        assert!(
            idx < n,
            "FilterGraphBuilder::input_handle: idx {idx} out of bounds (nb_inputs = {n})"
        );
        Handle::from_raw(
            unsafe { (**(*self.ptr).inputs.add(idx)).hdl },
            self._ctx.as_ptr(),
        )
    }

    /// Scheduler output handle for filtergraph output `idx`. Available pre-init
    /// for wiring with [`AvconvContext::connect`].
    pub fn output_handle(&self, idx: usize) -> Handle {
        let n = self.nb_outputs();
        assert!(
            idx < n,
            "FilterGraphBuilder::output_handle: idx {idx} out of bounds (nb_outputs = {n})"
        );
        Handle::from_raw(
            unsafe { (**(*self.ptr).outputs.add(idx)).hdl },
            self._ctx.as_ptr(),
        )
    }

    pub fn nb_inputs(&self) -> usize {
        unsafe { (*self.ptr).nb_inputs as usize }
    }

    pub fn nb_outputs(&self) -> usize {
        unsafe { (*self.ptr).nb_outputs as usize }
    }

    /// Fill the filtergraph output parameters from an encoder so the filter
    /// output matches what the encoder expects. The encoder must still be in
    /// its builder phase (`enc_ctx` is cleared once the encoder is built).
    pub fn set_out_params_from_enc(&mut self, out_idx: usize, enc: &EncoderBuilder) -> Result<()> {
        let n = self.nb_outputs();
        assert!(
            out_idx < n,
            "FilterGraphBuilder::set_out_params_from_enc: out_idx {out_idx} out of bounds (nb_outputs = {n})"
        );
        let params = unsafe { (**(*self.ptr).outputs.add(out_idx)).params };
        let enc_ctx = unsafe { (*enc.as_ptr()).enc_ctx };
        check_ret(unsafe { sys::avconv_fg_out_params_from_enc(params, enc_ctx) })?;
        Ok(())
    }

    /// Register a closure invoked for each frame entering the graph.
    /// `frame` is `None` on EOS.
    pub fn on_frame_in<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(Option<&mut sys::AVFrame>, u32, &mut u32) -> i32 + Send + 'static,
    {
        self.hooks.on_frame_in = Some(Box::new(f));
        self
    }

    /// Finish filtergraph setup and produce a runnable [`FilterGraph`].
    pub fn build(mut self) -> Result<FilterGraph> {
        // Always wire all thunks - leaving a function-pointer slot NULL
        // would access-violate when libavconv invokes the hook. Stub thunks
        // exist for events we don't expose to closure users.
        let storage_ptr = &mut *self.hooks as *mut FilterGraphHookStorage;
        unsafe {
            (*self.ptr).opaque = storage_ptr as *mut std::ffi::c_void;
            let dst = (*self.ptr).hooks;
            if !dst.is_null() {
                *dst = sys::AVConvFGHooks {
                    on_filter_init: Some(fg_on_filter_init_stub),
                    on_graph_config: Some(fg_on_graph_config_stub),
                    on_stream_params: Some(fg_on_stream_params_stub),
                    on_frame_in: Some(fg_on_frame_in_thunk),
                };
            }
        }

        check_ret(unsafe { sys::avconv_fg_init(self.ptr) })?;

        let FilterGraphBuilder { ptr, hooks, _ctx } = self;
        Ok(FilterGraph {
            ptr,
            _hooks: hooks,
            _ctx,
        })
    }
}

/// An initialised filtergraph node within an `AvconvContext`. Owned by the
/// context; freed when the context is dropped.
pub struct FilterGraph {
    ptr: *mut sys::AVConvFilterGraph,
    /// Keeps the closure storage alive while the graph may still invoke it.
    _hooks: Box<FilterGraphHookStorage>,
    _ctx: Arc<AvconvContext>,
}

// SAFETY: post-build the filtergraph exposes only read-only handle/count
// accessors. Internal mutation happens inside the scheduler's worker threads
// and is not observed through `&self`.
unsafe impl Send for FilterGraph {}
unsafe impl Sync for FilterGraph {}

impl FilterGraph {
    /// Scheduler input handle for filtergraph input `idx`.
    pub fn input_handle(&self, idx: usize) -> Handle {
        let n = self.nb_inputs();
        assert!(
            idx < n,
            "FilterGraph::input_handle: idx {idx} out of bounds (nb_inputs = {n})"
        );
        Handle::from_raw(
            unsafe { (**(*self.ptr).inputs.add(idx)).hdl },
            self._ctx.as_ptr(),
        )
    }

    /// Scheduler output handle for filtergraph output `idx`.
    pub fn output_handle(&self, idx: usize) -> Handle {
        let n = self.nb_outputs();
        assert!(
            idx < n,
            "FilterGraph::output_handle: idx {idx} out of bounds (nb_outputs = {n})"
        );
        Handle::from_raw(
            unsafe { (**(*self.ptr).outputs.add(idx)).hdl },
            self._ctx.as_ptr(),
        )
    }

    pub fn nb_inputs(&self) -> usize {
        unsafe { (*self.ptr).nb_inputs as usize }
    }

    pub fn nb_outputs(&self) -> usize {
        unsafe { (*self.ptr).nb_outputs as usize }
    }
}

unsafe extern "C" fn fg_on_frame_in_thunk(
    fg: *mut sys::AVConvFilterGraph,
    _pin: *mut *mut sys::AVConvFGIn,
    frame: *mut sys::AVFrame,
    flags_in: u32,
    flags_out: *mut u32,
) -> i32 {
    let known_flags_in = sys::AVConvFGHookFlagsIn_AVCONV_FG_HOOK_FLAG_IN_EOS
        | sys::AVConvFGHookFlagsIn_AVCONV_FG_HOOK_FLAG_IN_REPEAT
        | sys::AVConvFGHookFlagsIn_AVCONV_FG_HOOK_FLAG_IN_HEARTBEAT;
    if flags_in & !known_flags_in != 0 {
        return -libc::ENOSYS;
    }
    let storage = unsafe { &mut *((*fg).opaque as *mut FilterGraphHookStorage) };
    let Some(f) = storage.on_frame_in.as_mut() else {
        return -libc::ENOSYS;
    };
    let frame_ref = if flags_in & sys::AVConvFGHookFlagsIn_AVCONV_FG_HOOK_FLAG_IN_EOS != 0 {
        None
    } else {
        Some(unsafe { &mut *frame })
    };
    f(frame_ref, flags_in, unsafe { &mut *flags_out })
}

// Stub thunks for events not exposed via closures. Returning 0 lets
// libavconv continue with default behaviour.
unsafe extern "C" fn fg_on_filter_init_stub(
    _fg: *mut sys::AVConvFilterGraph,
    _filter: *mut sys::AVFilterContext,
    _opts: *mut *mut sys::AVDictionary,
    _flags: u32,
) -> i32 {
    0
}

unsafe extern "C" fn fg_on_graph_config_stub(
    _fg: *mut sys::AVConvFilterGraph,
    _graph: *mut sys::AVFilterGraph,
) -> i32 {
    0
}

unsafe extern "C" fn fg_on_stream_params_stub(
    _fg: *mut sys::AVConvFilterGraph,
    _input: *mut sys::AVConvFGIn,
    _par: *mut sys::AVConvFrameParams,
    flags: u32,
) -> i32 {
    if flags != 0 {
        return -libc::ENOSYS;
    }
    0
}
