// Project Kyber: error.rs
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

use std::ffi::CStr;
use thiserror::Error;

pub use libavconv_sys as sys;

/// Error type wrapping AVERROR return codes from libav*.
#[derive(Error, Debug)]
pub enum Error {
    #[error("libav error {code}: {description}")]
    AvError { code: i32, description: String },
    #[error("component not found: {name}")]
    NotFound { name: &'static str },
    #[error("allocation failed")]
    AllocFailed,
    #[error("no source available")]
    NoSource,
    #[error("null string")]
    NulError(#[from] std::ffi::NulError),
}

pub type Result<T> = std::result::Result<T, Error>;

/// `AVERROR_EOF` = `FFERRTAG('E','O','F',' ')`. bindgen doesn't surface it as a
/// constant (it expands to a macro over a compound expression), so compute it.
pub(crate) const AVERROR_EOF: i32 =
    -(b'E' as i32 | (b'O' as i32) << 8 | (b'F' as i32) << 16 | (b' ' as i32) << 24);

/// `AVERROR(EAGAIN)`; libav negates errno values.
pub(crate) const AVERROR_EAGAIN: i32 = -libc::EAGAIN;

/// `AVERROR_EXTERNAL` = `FFERRTAG('E','X','T',' ')`; same bindgen gap as
/// `AVERROR_EOF` above.
pub(crate) const AVERROR_EXTERNAL: i32 =
    -(b'E' as i32 | (b'X' as i32) << 8 | (b'T' as i32) << 16 | (b' ' as i32) << 24);

pub(crate) fn catch_hook_panic<F: FnOnce() -> i32>(f: F) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(AVERROR_EXTERNAL)
}

/// Wraps a raw AVERROR return code: returns Ok(ret) when ret >= 0,
/// or Err(Error::AvError) otherwise.
pub fn check_ret(ret: i32) -> Result<i32> {
    if ret >= 0 {
        Ok(ret)
    } else {
        let mut buf = [0i8; 256];
        let description = unsafe {
            sys::av_strerror(ret, buf.as_mut_ptr(), buf.len());
            CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
        };
        Err(Error::AvError {
            code: ret,
            description,
        })
    }
}
