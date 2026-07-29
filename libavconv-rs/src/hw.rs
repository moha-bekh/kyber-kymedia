// Project Kyber: hw.rs
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
use libavconv_sys as sys;

/// Wrapper around an `AVBufferRef` that holds an `AVHWDeviceContext`.
/// Freed with `av_buffer_unref` on drop.
pub struct HwDeviceRef {
    ptr: *mut sys::AVBufferRef,
}

unsafe impl Send for HwDeviceRef {}
unsafe impl Sync for HwDeviceRef {}

impl HwDeviceRef {
    /// Create a hardware device context of the given type.
    ///
    /// `device` device name string passed to `av_hwdevice_ctx_create`
    ///   (NULL/empty for default device).
    pub fn new(device_type: sys::AVHWDeviceType) -> Result<Self> {
        let mut ptr: *mut sys::AVBufferRef = std::ptr::null_mut();
        check_ret(unsafe {
            sys::av_hwdevice_ctx_create(
                &mut ptr,
                device_type,
                std::ptr::null(),     // device name (default)
                std::ptr::null_mut(), // options
                0,                    // flags
            )
        })?;
        Ok(Self { ptr })
    }

    /// Raw pointer, for passing to APIs that take ownership of a new reference.
    ///
    /// The caller must call `av_buffer_ref()` themselves if they need their own
    /// copy, or use `as_ref_ptr()`.
    pub fn as_ptr(&self) -> *mut sys::AVBufferRef {
        self.ptr
    }

    /// Clone the reference returns a new `AVBufferRef*` that the caller owns.
    ///
    /// # Safety
    /// The returned pointer must be freed with `av_buffer_unref`.
    pub unsafe fn clone_ref(&self) -> *mut sys::AVBufferRef {
        unsafe { sys::av_buffer_ref(self.ptr) }
    }
}

impl Drop for HwDeviceRef {
    fn drop(&mut self) {
        unsafe { sys::av_buffer_unref(&mut self.ptr) };
    }
}
