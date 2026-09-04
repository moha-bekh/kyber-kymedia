// Project Kyber: grab.rs
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
use std::marker::PhantomData;
use std::sync::Arc;

use crate::context::AvconvContext;
use crate::error::{check_ret, Error, Result};
use crate::hw::HwDeviceRef;
use crate::Handle;
use libavconv_sys as sys;

/// `flags_in` bit set on a [`GrabCaptureBuilder::on_frame`] invocation that
/// carries a heartbeat rather than real data: an assertion that nothing new
/// arrived up to `frame.pts`. The frame is empty apart from `pts`, `time_base`
/// and possibly side data, so hooks must not interpret it as content.
///
/// Emitted by e.g. the DXGI provider whenever a desktop-duplication acquire
/// times out. Mirrors `AVGRAB_CAPTURE_HOOK_FLAG_IN_HEARTBEAT` in `avgrab.h`.
pub const CAPTURE_HOOK_FLAG_IN_HEARTBEAT: u32 =
    sys::AVGrabCaptureHookFlagsIn_AVGRAB_CAPTURE_HOOK_FLAG_IN_HEARTBEAT;

/// Opaque handle to a registered grab provider (e.g. "d3d", "x11", "pulse").
///
/// Provider descriptors are static C data registered at libavconv init, so
/// `GrabProvider` is `Copy` and effectively `'static`.
#[derive(Copy, Clone)]
pub struct GrabProvider {
    ptr: *const sys::AVGrabProvider,
}

// SAFETY: provider descriptors are static, read-only C data.
unsafe impl Send for GrabProvider {}
unsafe impl Sync for GrabProvider {}

impl GrabProvider {
    pub(crate) fn from_raw(ptr: *const sys::AVGrabProvider) -> Self {
        Self { ptr }
    }

    pub(crate) fn as_ptr(self) -> *const sys::AVGrabProvider {
        self.ptr
    }
}

/// Find a grab provider by its short name (e.g. "d3d", "x11", "pulse").
pub fn provider_by_name(name: &str) -> Option<GrabProvider> {
    let cname = CString::new(name).ok()?;
    let ptr = unsafe { sys::avgrab_provider_by_name(cname.as_ptr()) };
    if ptr.is_null() {
        None
    } else {
        Some(GrabProvider::from_raw(ptr))
    }
}

/// Iterate over all enabled providers.
pub fn providers() -> impl Iterator<Item = GrabProvider> {
    let mut iter: *mut libc::c_void = std::ptr::null_mut();
    std::iter::from_fn(move || {
        let p = unsafe { sys::avgrab_provider_iterate(&mut iter as *mut _) };
        if p.is_null() {
            None
        } else {
            Some(GrabProvider::from_raw(p))
        }
    })
}

// ---- GrabProviderCtx -------------------------------------------------------

/// An initialised connection to a grab provider (e.g. a DXGI adapter or an
/// X11 server).  Freed via `av_refstruct_unref` on drop.
pub struct GrabProviderCtx {
    ptr: *mut sys::AVGrabProviderContext,
}

// SAFETY: avgrab provider context is not thread-local; we maintain exclusive
// access through &mut borrows.
unsafe impl Send for GrabProviderCtx {}

impl GrabProviderCtx {
    /// Allocate a provider context for the given provider descriptor.
    pub fn new(provider: GrabProvider) -> Result<Self> {
        let ptr = unsafe { sys::avgrab_provider_alloc(provider.as_ptr()) };
        if ptr.is_null() {
            Err(Error::AllocFailed)
        } else {
            Ok(Self { ptr })
        }
    }

    /// Attach a hardware device reference. The context takes its own
    /// `av_buffer_ref` of `hw_device`; the caller keeps their reference.
    pub fn set_hw_device(&mut self, hw_device: &HwDeviceRef) {
        let owned = unsafe { sys::av_buffer_ref(hw_device.as_ptr()) };
        // The field hw_device is owned by the context; if already set we'd
        // need to unref the old one, but since we only call this once before
        // init() it's always NULL here.
        unsafe { (*self.ptr).hw_device = owned };
    }

    /// Initialise the provider connection.  Must be called after setting any
    /// context fields and before calling `sources()`.
    pub fn init(&mut self) -> Result<()> {
        check_ret(unsafe { sys::avgrab_provider_init(self.ptr) })?;
        Ok(())
    }

    /// Enumerate available sources from this provider.
    ///
    /// `payloads`: bitmask of `AVConvHandlePayload` values; use
    ///   `1 << AVCONV_HANDLE_PAYLOAD_FRAME` for raw-frame sources.
    ///
    /// `src_types`: bitmask of `AVGrabSourceType` values; use
    ///   `1 << AVGRAB_SOURCE_TYPE_SCREEN` for screen-capture sources.
    pub fn sources(&self, payloads: u32, src_types: u64) -> Result<GrabSourceSet> {
        let mut ss: *const sys::AVGrabSourceSet = std::ptr::null();
        check_ret(unsafe {
            sys::avgrab_provider_sources(self.ptr, &mut ss, payloads, src_types, 0)
        })?;
        if ss.is_null() {
            Err(Error::NoSource)
        } else {
            Ok(GrabSourceSet { ptr: ss })
        }
    }
}

impl Drop for GrabProviderCtx {
    fn drop(&mut self) {
        unsafe {
            sys::av_refstruct_unref(&mut self.ptr as *mut *mut _ as *mut libc::c_void);
        }
    }
}

// ---- GrabSourceSet ---------------------------------------------------------

/// An immutable set of available grab sources.  Freed via `av_refstruct_unref`.
pub struct GrabSourceSet {
    ptr: *const sys::AVGrabSourceSet,
}

unsafe impl Send for GrabSourceSet {}
unsafe impl Sync for GrabSourceSet {}

impl GrabSourceSet {
    /// Iterate over the sources in this set.
    pub fn iter(&self) -> GrabSourceIter<'_> {
        GrabSourceIter {
            set: self.ptr,
            iter: std::ptr::null_mut(),
            _marker: PhantomData,
        }
    }

    /// Number of sources in the set.
    pub fn len(&self) -> usize {
        unsafe { (*self.ptr).nb_sources as usize }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Refresh the source set against the underlying provider.  Returns
    /// `true` when the set has changed (and `self` now references the
    /// new set), `false` when nothing has changed.
    pub fn refresh(&mut self) -> Result<bool> {
        let ret = unsafe {
            sys::avgrab_sources_refresh(&mut self.ptr as *mut *const sys::AVGrabSourceSet)
        };
        if ret < 0 {
            return Err(crate::error::Error::AvError {
                code: ret,
                description: "avgrab_sources_refresh failed".into(),
            });
        }
        Ok(ret > 0)
    }
}

impl Drop for GrabSourceSet {
    fn drop(&mut self) {
        unsafe {
            sys::av_refstruct_unref(&mut self.ptr as *mut *const _ as *mut libc::c_void);
        }
    }
}

/// A source descriptor borrowed from a [`GrabSourceSet`]. Carries the set's
/// lifetime so iterating handles can't outlive the underlying refstruct.
#[derive(Copy, Clone)]
pub struct GrabSource<'a> {
    ptr: *const sys::AVGrabSource,
    _set: PhantomData<&'a GrabSourceSet>,
}

impl<'a> GrabSource<'a> {
    pub(crate) fn from_raw(ptr: *const sys::AVGrabSource) -> Self {
        Self {
            ptr,
            _set: PhantomData,
        }
    }

    pub(crate) fn as_ptr(self) -> *const sys::AVGrabSource {
        self.ptr
    }

    /// Access the raw source struct fields. Caller must respect FFmpeg
    /// invariants (read-only access, mind the lifetime of any embedded
    /// pointers).
    ///
    /// # Safety
    /// The dereference is sound while `self` is borrowed (the set is live);
    /// the pointer remains valid for the iterator borrow.
    pub unsafe fn raw(&self) -> &sys::AVGrabSource {
        unsafe { &*self.ptr }
    }

    /// Human-readable source name, or `None` when the provider did not set
    /// one. Not guaranteed to be unique.
    pub fn name(&self) -> Option<String> {
        // SAFETY: self.ptr is valid for the borrow; name is either NULL or a
        // NUL-terminated C string owned by the source.
        let name = unsafe { (*self.ptr).name };
        if name.is_null() {
            None
        } else {
            Some(
                unsafe { std::ffi::CStr::from_ptr(name) }
                    .to_string_lossy()
                    .into_owned(),
            )
        }
    }

    /// Object x position in pixels (video sources).
    pub fn x(&self) -> i32 {
        unsafe { (*self.ptr).x }
    }

    /// Object y position in pixels (video sources).
    pub fn y(&self) -> i32 {
        unsafe { (*self.ptr).y }
    }

    /// Object width in pixels (video sources).
    pub fn width(&self) -> i32 {
        unsafe { (*self.ptr).width }
    }

    /// Object height in pixels (video sources).
    pub fn height(&self) -> i32 {
        unsafe { (*self.ptr).height }
    }

    /// Category of this source (screen, audio loopback, etc.).
    pub fn source_type(&self) -> GrabSourceType {
        GrabSourceType::from_raw(unsafe { (*self.ptr).type_ })
    }

    /// Raw flags bitmask (`AVGrabSourceFlags`). Use the
    /// `sys::AVGrabSourceFlags_AVGRAB_SOURCE_FLAG_*` constants to test bits.
    pub fn flags(&self) -> u32 {
        unsafe { (*self.ptr).flags }
    }

    /// Iterate over `(stream_index, stream_type)` pairs describing every
    /// elementary stream advertised by this source. The index is what must be
    /// passed to [`GrabCaptureBuilder::with_streams`].
    pub fn streams(&self) -> impl Iterator<Item = (u32, StreamType)> + 'a {
        let raw = unsafe { &*self.ptr };
        let n = raw.nb_streams as usize;
        let streams = raw.streams;
        (0..n).map(move |i| {
            // SAFETY: streams[i] is a valid pointer for as long as the source
            // refstruct (and therefore `self`) is live.
            let st = unsafe { &**streams.add(i) };
            (i as u32, StreamType::from_raw(st.type_))
        })
    }

    /// Return the index of the first stream of type `ty`, if any.
    pub fn find_stream_by_type(&self, ty: StreamType) -> Option<u32> {
        self.streams().find(|(_, t)| *t == ty).map(|(i, _)| i)
    }
}

/// Category of a grab source. Mirrors `AVGrabSourceType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrabSourceType {
    Unknown,
    Screen,
    Window,
    AudioLoopback,
    Other(u32),
}

impl GrabSourceType {
    fn from_raw(t: sys::AVGrabSourceType) -> Self {
        match t {
            sys::AVGrabSourceType_AVGRAB_SOURCE_TYPE_UNKNOWN => Self::Unknown,
            sys::AVGrabSourceType_AVGRAB_SOURCE_TYPE_SCREEN => Self::Screen,
            sys::AVGrabSourceType_AVGRAB_SOURCE_TYPE_WINDOW => Self::Window,
            sys::AVGrabSourceType_AVGRAB_SOURCE_TYPE_AUDIO_LOOPBACK => Self::AudioLoopback,
            other => Self::Other(other),
        }
    }
}

/// Role of a stream within a grab source. Mirrors `AVGrabSourceStreamType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamType {
    Unknown,
    /// Main payload of the source (e.g. the desktop image).
    Default,
    /// Mouse cursor side-channel.
    Cursor,
    /// Provider-defined overlay layer.
    Overlay,
    /// Any other type added by a future libavconv version.
    Other(u32),
}

impl StreamType {
    fn from_raw(t: sys::AVGrabSourceStreamType) -> Self {
        match t {
            sys::AVGrabSourceStreamType_AVGRAB_SOURCE_STREAM_UNKNOWN => Self::Unknown,
            sys::AVGrabSourceStreamType_AVGRAB_SOURCE_STREAM_DEFAULT => Self::Default,
            sys::AVGrabSourceStreamType_AVGRAB_SOURCE_STREAM_CURSOR => Self::Cursor,
            sys::AVGrabSourceStreamType_AVGRAB_SOURCE_STREAM_OVERLAY => Self::Overlay,
            other => Self::Other(other),
        }
    }
}

/// Iterator over [`GrabSource`] entries within a [`GrabSourceSet`].
pub struct GrabSourceIter<'a> {
    set: *const sys::AVGrabSourceSet,
    iter: *mut libc::c_void,
    _marker: PhantomData<&'a GrabSourceSet>,
}

impl<'a> Iterator for GrabSourceIter<'a> {
    type Item = GrabSource<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let src = unsafe { sys::avgrab_sources_iterate(self.set, &mut self.iter as *mut _) };
        if src.is_null() {
            None
        } else {
            Some(GrabSource::from_raw(src))
        }
    }
}

// ---- GrabCaptureBuilder ----------------------------------------------------

type FrameFn = dyn FnMut(&mut u32, &mut sys::AVFrame, u32, &mut u32) -> i32 + Send;

/// Closure storage for grab-capture hooks. Heap-stable so the address
/// handed to libavgrab via `AVGrabCapture::opaque` remains valid for the
/// lifetime of the owning [`GrabCapture`].
#[derive(Default)]
pub(crate) struct GrabCaptureHookStorage {
    on_frame: Option<Box<FrameFn>>,
}

/// In-construction grab capture. The capture is bound to its source at
/// `new()` time, configured pre-init (epoch, closures) on the builder, and
/// registered with an `AvconvContext` by `build()`.
pub struct GrabCaptureBuilder {
    ptr: *mut sys::AVGrabCapture,
    hooks: Box<GrabCaptureHookStorage>,
}

unsafe impl Send for GrabCaptureBuilder {}

impl GrabCaptureBuilder {
    /// Create a capture process for the default stream of `src`.
    ///
    /// libavgrab takes its own refstruct reference to the source internally,
    /// so the borrow on the originating set may end after this call.
    pub fn new(src: GrabSource<'_>) -> Result<Self> {
        Self::with_streams(src, &[])
    }

    /// Create a capture process for an explicit list of source-stream indices.
    /// Pass an empty slice to capture only the default stream (same as `new`).
    /// Indices must come from [`GrabSource::streams`] / [`GrabSource::find_stream_by_type`].
    pub fn with_streams(src: GrabSource<'_>, streams: &[u32]) -> Result<Self> {
        let (streams_ptr, n) = if streams.is_empty() {
            (std::ptr::null(), 0)
        } else {
            (streams.as_ptr(), streams.len() as libc::c_uint)
        };

        let mut c: *mut sys::AVGrabCapture = std::ptr::null_mut();
        check_ret(unsafe {
            sys::avgrab_capture_create(
                src.as_ptr(),
                0, // flags (unused)
                streams_ptr,
                n,
                &mut c,
            )
        })?;
        if c.is_null() {
            Err(Error::AllocFailed)
        } else {
            Ok(Self {
                ptr: c,
                hooks: Box::new(GrabCaptureHookStorage::default()),
            })
        }
    }

    /// Set an `av_opt_set` option on the underlying `AVGrabCapture` (e.g.
    /// `framerate`, `initial_pool_size`). Must be called before `build()`.
    pub fn set_option(&self, key: &str, value: &str) -> Result<()> {
        let key = std::ffi::CString::new(key)?;
        let value = std::ffi::CString::new(value)?;
        check_ret(unsafe {
            sys::av_opt_set(
                self.ptr as *mut _,
                key.as_ptr(),
                value.as_ptr(),
                sys::AV_OPT_SEARCH_CHILDREN as i32,
            )
        })?;
        Ok(())
    }

    /// Set the clock epoch (us) used for pts computation:
    /// `pts = av_gettime_relative() - epoch`.
    ///
    /// `epoch` lives in `AVGrabCapture::params`, which libavgrab nulls out in
    /// `avgrab_capture_init()`; builder-only access keeps the write in the
    /// window where the pointer is still valid.
    pub fn set_epoch(&mut self, epoch: i64) {
        unsafe { (*(*self.ptr).params).epoch = epoch }
    }

    /// Register a closure invoked for each captured frame.
    pub fn on_frame<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(&mut u32, &mut sys::AVFrame, u32, &mut u32) -> i32 + Send + 'static,
    {
        self.hooks.on_frame = Some(Box::new(f));
        self
    }

    /// Register the capture with the given context and produce a runnable
    /// [`GrabCapture`]. After this call each stream's handle is available via
    /// [`GrabCapture::stream_handle`].
    pub fn build(mut self, ctx: Arc<AvconvContext>) -> Result<GrabCapture> {
        // Stub thunks exist for events we don't expose to closure users;
        // `on_packet` is left NULL because no capture stream we drive carries a
        // coded payload (libavgrab NULL-checks every hook slot).
        let storage_ptr = &mut *self.hooks as *mut GrabCaptureHookStorage;
        unsafe {
            (*self.ptr).opaque = storage_ptr as *mut std::ffi::c_void;
            let dst = (*self.ptr).hooks;
            if !dst.is_null() {
                *dst = sys::AVGrabCaptureHooks {
                    on_frame: Some(grab_on_frame_thunk),
                    on_packet: None,
                    on_frame_params: Some(grab_on_frame_params_stub),
                    on_codec_params: Some(grab_on_codec_params_stub),
                };
            }
        }

        check_ret(unsafe { sys::avgrab_capture_init(self.ptr, ctx.as_ptr()) })?;

        let this = std::mem::ManuallyDrop::new(self);
        let ptr = this.ptr;
        let hooks = unsafe { std::ptr::read(&this.hooks) };
        Ok(GrabCapture {
            ptr,
            _hooks: hooks,
            _ctx: ctx,
        })
    }
}

impl Drop for GrabCaptureBuilder {
    fn drop(&mut self) {
        // The builder owns a refstruct ref until it is consumed by `build`;
        // release it here if the builder is dropped without building.
        unsafe {
            sys::av_refstruct_unref(&mut self.ptr as *mut *mut _ as *mut libc::c_void);
        }
    }
}

// ---- GrabCapture -----------------------------------------------------------

/// An active grabbing process registered with an `AvconvContext`.
/// Freed via `av_refstruct_unref` on drop.
pub struct GrabCapture {
    ptr: *mut sys::AVGrabCapture,
    /// Keeps the closure storage alive while the capture may still invoke it.
    _hooks: Box<GrabCaptureHookStorage>,
    _ctx: Arc<AvconvContext>,
}

// SAFETY: post-build the capture exposes only read-only handle accessors.
// The capture thread mutates internal state but those mutations are not
// observed through `&self`.
unsafe impl Send for GrabCapture {}
unsafe impl Sync for GrabCapture {}

impl GrabCapture {
    /// Return the `AVConvHandle` (scheduler output handle) for stream `idx`.
    /// Panics if `idx >= nb_streams()`.
    pub fn stream_handle(&self, idx: usize) -> Handle {
        let n = self.nb_streams();
        assert!(
            idx < n,
            "GrabCapture::stream_handle: idx {idx} out of bounds (nb_streams = {n})"
        );
        Handle::from_raw(
            unsafe {
                let streams = (*self.ptr).streams;
                (**streams.add(idx)).hdl
            },
            self._ctx.as_ptr(),
        )
    }

    /// Number of capture streams.
    pub fn nb_streams(&self) -> usize {
        unsafe { (*self.ptr).nb_streams as usize }
    }

    /// Role of capture stream `idx` (as advertised by the source stream it was
    /// built from). Panics if `idx >= nb_streams()`.
    pub fn stream_type(&self, idx: usize) -> StreamType {
        let n = self.nb_streams();
        assert!(
            idx < n,
            "GrabCapture::stream_type: idx {idx} out of bounds (nb_streams = {n})"
        );
        let ty = unsafe {
            let cs = &**(*self.ptr).streams.add(idx);
            (*cs.src_stream).type_
        };
        StreamType::from_raw(ty)
    }

    /// Shut down the capture, signalling end-of-stream downstream.
    pub fn stop(&self) {
        unsafe { sys::avgrab_capture_stop(self.ptr) };
    }
}

unsafe extern "C" fn grab_on_frame_thunk(
    c: *mut sys::AVGrabCapture,
    stream_idx: *mut u32,
    frame: *mut sys::AVFrame,
    flags_in: u32,
    flags_out: *mut u32,
) -> i32 {
    let known_flags_in = sys::AVGrabCaptureHookFlagsIn_AVGRAB_CAPTURE_HOOK_FLAG_IN_REPEAT
        | sys::AVGrabCaptureHookFlagsIn_AVGRAB_CAPTURE_HOOK_FLAG_IN_HEARTBEAT;
    if flags_in & !known_flags_in != 0 {
        return -libc::ENOSYS;
    }
    let storage = unsafe { &mut *((*c).opaque as *mut GrabCaptureHookStorage) };
    let Some(f) = storage.on_frame.as_mut() else {
        // No closure attached: tell libavgrab to take the hook-less path so
        // the original capture frame is moved (not ref'd) into the downstream
        // queue. Returning 0 here would route through `cp->hook_frame`,
        // leaving the capture frame ref-counted forever and leaking hwframe
        // pool slots (observed as "Static surface pool size exceeded").
        return -libc::ENOSYS;
    };
    crate::error::catch_hook_panic(|| {
        f(
            unsafe { &mut *stream_idx },
            unsafe { &mut *frame },
            flags_in,
            unsafe { &mut *flags_out },
        )
    })
}

// Stub thunks for events not exposed via closures.
unsafe extern "C" fn grab_on_frame_params_stub(
    _c: *mut sys::AVGrabCapture,
    _par: *mut sys::AVConvFrameParams,
    _stream_idx: u32,
    _flags: u32,
) -> i32 {
    0
}

unsafe extern "C" fn grab_on_codec_params_stub(
    _c: *mut sys::AVGrabCapture,
    _par: *mut sys::AVConvCodecParams,
    _stream_idx: u32,
    _flags: u32,
) -> i32 {
    0
}

impl Drop for GrabCapture {
    fn drop(&mut self) {
        unsafe {
            sys::av_refstruct_unref(&mut self.ptr as *mut *mut _ as *mut libc::c_void);
        }
    }
}

// ---- Frame side-data accessors ---------------------------------------------

/// Read libavgrab side-data attached to a capture frame.
pub mod side_data {
    use super::sys;

    /// Borrowed view on `AVGRAB_FRAME_SIDE_DATA_CURSOR`: the cursor's hot spot
    /// and an optional XOR mask appended after the struct payload.
    pub struct CursorInfo<'a> {
        info: &'a sys::AVGrabCursorInfo,
        xor_mask: &'a [u8],
    }

    impl<'a> CursorInfo<'a> {
        pub fn hotspot_x(&self) -> i32 {
            self.info.hotspot_x
        }
        pub fn hotspot_y(&self) -> i32 {
            self.info.hotspot_y
        }
        /// XOR mask bytes (1 bit per cursor pixel, MSB-first); empty when the
        /// capture source did not emit one.
        pub fn xor_mask(&self) -> &'a [u8] {
            self.xor_mask
        }
    }

    /// Plain copy of `AVGRAB_FRAME_SIDE_DATA_GRAB_COORDS`. The `invisible`
    /// field semantics are provider-defined: DXGI populates it with the Win32
    /// `PointerPosition.Visible` flag (1 = visible) - see the consumer for
    /// platform-specific interpretation.
    pub type Coords = sys::AVGrabCoords;

    fn find(
        frame: &sys::AVFrame,
        ty: sys::AVGrabFrameSideDataType,
    ) -> Option<&sys::AVFrameSideData> {
        if frame.side_data.is_null() {
            return None;
        }
        let n = frame.nb_side_data.max(0) as usize;
        for i in 0..n {
            // SAFETY: side_data[0..nb_side_data] is a valid pointer array,
            // entries may be NULL though.
            let sd = unsafe { *frame.side_data.add(i) };
            if sd.is_null() {
                continue;
            }
            let sd_ref = unsafe { &*sd };
            if sd_ref.type_ == ty {
                return Some(sd_ref);
            }
        }
        None
    }

    /// Extract `AVGRAB_FRAME_SIDE_DATA_CURSOR` from `frame`, if present.
    pub fn cursor_info(frame: &sys::AVFrame) -> Option<CursorInfo<'_>> {
        let sd = find(
            frame,
            sys::AVGrabFrameSideDataType_AVGRAB_FRAME_SIDE_DATA_CURSOR,
        )?;
        if sd.data.is_null() || sd.size < std::mem::size_of::<sys::AVGrabCursorInfo>() {
            return None;
        }
        // SAFETY: side-data payload starts with an AVGrabCursorInfo (verified
        // by size check) and is valid for the borrow on `frame`.
        let info = unsafe { &*(sd.data as *const sys::AVGrabCursorInfo) };

        let mask: &[u8] = if info.xor_mask_len > 0 && info.xor_mask_offset > 0 {
            let off = info.xor_mask_offset as usize;
            let len = info.xor_mask_len;
            match off.checked_add(len) {
                Some(end) if end <= sd.size => unsafe {
                    std::slice::from_raw_parts(sd.data.add(off), len)
                },
                _ => &[],
            }
        } else {
            &[]
        };

        Some(CursorInfo {
            info,
            xor_mask: mask,
        })
    }

    /// Iterator over the packed `AV_PIX_FMT_RGBA` rows of a frame, each row
    /// borrowed as `width` host-endian `u32` words. Built by [`rgba_rows`].
    pub struct RgbaRows<'a> {
        data: *const u8,
        linesize: usize,
        width: usize,
        height: usize,
        row: usize,
        _marker: std::marker::PhantomData<&'a sys::AVFrame>,
    }

    impl<'a> Iterator for RgbaRows<'a> {
        type Item = &'a [u32];

        fn next(&mut self) -> Option<&'a [u32]> {
            if self.row >= self.height {
                return None;
            }
            let off = self.row * self.linesize;
            self.row += 1;
            // SAFETY: `row < height` and `linesize >= width * 4` were checked
            // when the iterator was built, so `width` u32 words starting at
            // `data + off` stay within the frame's plane-0 allocation.
            Some(unsafe {
                std::slice::from_raw_parts(self.data.add(off) as *const u32, self.width)
            })
        }
    }

    /// Borrow the packed `AV_PIX_FMT_RGBA` pixel rows of `frame` as host-endian
    /// `u32` words. Returns `None` if plane 0 is null or its stride is too
    /// small to hold `width` RGBA pixels per row. Keeps the raw pointer
    /// arithmetic inside the FFI wrapper so callers stay `unsafe`-free.
    pub fn rgba_rows(frame: &sys::AVFrame) -> Option<RgbaRows<'_>> {
        let width = frame.width.max(0) as usize;
        let height = frame.height.max(0) as usize;
        let linesize = frame.linesize[0].max(0) as usize;
        let data = frame.data[0];
        let row_bytes = width.checked_mul(4)?;
        if data.is_null() || linesize < row_bytes {
            return None;
        }
        Some(RgbaRows {
            data,
            linesize,
            width,
            height,
            row: 0,
            _marker: std::marker::PhantomData,
        })
    }

    /// Extract `AVGRAB_FRAME_SIDE_DATA_GRAB_COORDS` from `frame`, if present.
    pub fn coords(frame: &sys::AVFrame) -> Option<Coords> {
        let sd = find(
            frame,
            sys::AVGrabFrameSideDataType_AVGRAB_FRAME_SIDE_DATA_GRAB_COORDS,
        )?;
        if sd.data.is_null() || sd.size < std::mem::size_of::<sys::AVGrabCoords>() {
            return None;
        }
        // SAFETY: payload size matches AVGrabCoords; the struct is POD.
        Some(unsafe { std::ptr::read_unaligned(sd.data as *const sys::AVGrabCoords) })
    }
}
