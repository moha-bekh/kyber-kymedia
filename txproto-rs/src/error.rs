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

use txproto_sys as sys;

#[derive(Debug, Error)]
pub enum Error {
    #[error("txproto error: {value} ({description:?}")]
    Txproto {
        value: i32,
        description: Option<String>,
    },
    #[error("txproto component {name} creation failed")]
    TxprotoComponentCreateFailed { name: String },
    #[error("NulError: {source:?}")]
    NulError {
        #[from]
        source: std::ffi::NulError,
    },
}

impl From<libc::c_int> for Error {
    fn from(value: libc::c_int) -> Self {
        const AV_ERROR_MAX_STRING_SIZE: usize = 64;
        let mut buf = vec![0; AV_ERROR_MAX_STRING_SIZE];

        let ret = unsafe { sys::av_strerror(value, buf.as_mut_ptr(), buf.len() as _) };

        let description = if ret == 0 {
            let s = unsafe { CStr::from_ptr(buf.as_ptr()) };
            s.to_str().map(|s| s.into()).ok()
        } else {
            None
        };

        Self::Txproto { value, description }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
