// Project Kyber: lib.rs
// Copyright © 2022-2026 Kyber SAS
// SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0-or-later
//
// Safe Rust wrapper over the kywatermark C library.

use std::ffi::CString;
use std::path::Path;
use std::ptr::NonNull;

use thiserror::Error;

pub use kywatermark_sys as sys;

#[derive(Debug, Error)]
pub enum Error {
    #[error("kywatermark_alloc returned NULL")]
    AllocFailed,
    #[error("invalid path (contains NUL byte)")]
    InvalidPath,
    #[error("kywatermark returned AVERROR({code})")]
    AvError { code: i32 },
}

pub type Result<T> = std::result::Result<T, Error>;

fn check_ret(ret: i32) -> Result<()> {
    if ret < 0 {
        Err(Error::AvError { code: ret })
    } else {
        Ok(())
    }
}

/// Per-watermark parameters. Anchor is in `[-1.0, 1.0]` matching the kyavservice
/// `WatermarkConfig` semantics: (0, 0) = center, (-1, -1) = top-left,
/// (1, 1) = bottom-right.
#[derive(Debug, Clone)]
pub struct WatermarkParams<'a> {
    pub image_path: &'a Path,
    pub x: f32,
    pub y: f32,
    pub opacity: f32,
}

/// GPU watermark compositor. Holds a reference to the hwframe device passed
/// at construction; the C lib drops that reference on `Drop`.
pub struct Watermark {
    ptr: NonNull<sys::kywatermark_ctx>,
}

// SAFETY: kywatermark state is owned exclusively by this struct (`&mut self`
// for any mutation). The underlying libplacebo render queue is
// single-threaded.
unsafe impl Send for Watermark {}

impl Watermark {
    /// Allocate an watermark context bound to a libavutil hwframe device.
    ///
    /// `hw_device` must point to a valid `AVBufferRef` wrapping an
    /// `AVHWDeviceContext`. The C lib calls `av_buffer_ref` internally, so
    /// the caller retains its own reference.
    ///
    /// # Safety
    /// `hw_device` must be a valid, non-null `AVBufferRef *`.
    unsafe fn alloc(hw_device: *mut sys::AVBufferRef) -> Result<Self> {
        let ptr = unsafe { sys::kywatermark_alloc(hw_device) };
        NonNull::new(ptr)
            .map(|ptr| Self { ptr })
            .ok_or(Error::AllocFailed)
    }

    /// Allocate an watermark bound to a libavconv hardware device.
    ///
    /// Safe entry point: `HwDeviceRef` guarantees a valid `AVBufferRef`
    /// wrapping an `AVHWDeviceContext`, so no precondition is left to the
    /// caller. The C lib takes its own reference, so `hw_device` may be
    /// dropped afterwards.
    pub fn new(hw_device: &libavconv_rs::HwDeviceRef) -> Result<Self> {
        // SAFETY: the two `-sys` crates bindgen the same C `AVBufferRef`, so
        // the pointer is ABI-compatible; `HwDeviceRef` guarantees it is valid.
        let raw = hw_device.as_ptr() as *mut sys::AVBufferRef;
        unsafe { Self::alloc(raw) }
    }

    /// Add a watermark. Loads the PNG at `image_path` and uploads it to GPU.
    pub fn add(&mut self, params: &WatermarkParams) -> Result<()> {
        let path_str = params.image_path.to_str().ok_or(Error::InvalidPath)?;
        let cpath = CString::new(path_str).map_err(|_| Error::InvalidPath)?;

        let raw = sys::kywatermark_params {
            image_path: cpath.as_ptr(),
            x: params.x,
            y: params.y,
            opacity: params.opacity,
        };
        check_ret(unsafe { sys::kywatermark_add(self.ptr.as_ptr(), &raw) })
    }

    /// Composite all watermarks onto `frame` in place.
    ///
    /// # Safety
    /// `frame` must be a valid `AVFrame *` pointing to a hwframe in the
    /// same hwframe context as the device passed at construction.
    unsafe fn apply(&mut self, frame: *mut sys::AVFrame) -> Result<()> {
        check_ret(unsafe { sys::kywatermark_apply(self.ptr.as_ptr(), frame) })
    }

    /// Composite all watermarks onto a libavconv `AVFrame` in place.
    ///
    /// Safe entry point taking the frame by `&mut` reference. `frame` must
    /// reference a hwframe in the same hwframe context as the device passed
    /// to [`Watermark::new`]; this is upheld by the libavconv filter graph that
    /// produced it.
    pub fn apply_frame(&mut self, frame: &mut libavconv_sys::AVFrame) -> Result<()> {
        // SAFETY: `frame` is a valid `&mut AVFrame`; the two `-sys` crates
        // bindgen the same C `AVFrame`, so the pointer cast is ABI-compatible.
        let raw = frame as *mut libavconv_sys::AVFrame as *mut sys::AVFrame;
        unsafe { self.apply(raw) }
    }
}

impl Drop for Watermark {
    fn drop(&mut self) {
        let mut p = self.ptr.as_ptr();
        unsafe { sys::kywatermark_free(&mut p) };
    }
}
