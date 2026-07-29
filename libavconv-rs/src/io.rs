// Project Kyber: io.rs
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
use std::path::Path;

use crate::error::{check_ret, Error, Result};
use libavconv_sys as sys;

/// RAII wrapper around `AVIOContext`. Drops via `avio_closep` unless ownership
/// has been transferred elsewhere (e.g. via [`AvconvContext::request_sdp`]).
pub struct AvioContext {
    ptr: *mut sys::AVIOContext,
}

// SAFETY: AVIOContext is not thread-local; libavformat's avio layer guards
// internal state. The wrapper exposes only owning operations.
unsafe impl Send for AvioContext {}

impl AvioContext {
    /// Open a file-backed `AVIOContext` for writing. Returns
    /// [`Error::NotFound`] if `path` contains non-UTF-8 bytes or interior NULs.
    pub fn open_for_write(path: &Path) -> Result<Self> {
        let path_str = path.to_str().ok_or(Error::NotFound {
            name: "non-UTF-8 path",
        })?;
        let path_c = CString::new(path_str)?;
        let mut io: *mut sys::AVIOContext = std::ptr::null_mut();
        check_ret(unsafe {
            sys::avio_open(&mut io, path_c.as_ptr(), sys::AVIO_FLAG_WRITE as i32)
        })?;
        if io.is_null() {
            return Err(Error::AllocFailed);
        }
        Ok(Self { ptr: io })
    }

    /// Consume the wrapper without closing the underlying `AVIOContext`,
    /// transferring ownership to the caller. Intended for FFI handover
    /// (e.g. `avconv_sdp`).
    pub(crate) fn into_raw(self) -> *mut sys::AVIOContext {
        let ptr = self.ptr;
        std::mem::forget(self);
        ptr
    }
}

impl Drop for AvioContext {
    fn drop(&mut self) {
        unsafe { sys::avio_closep(&mut self.ptr) };
    }
}
