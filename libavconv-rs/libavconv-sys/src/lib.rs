// Project Kyber: lib.rs
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

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(improper_ctypes)]
#![allow(clippy::type_complexity)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::os::raw::{c_char, c_int};

extern "C" {
    /// Install the FFmpeg log sink (defined in `log_shim.c`). The sink receives
    /// a severity `level` plus an already-formatted line and its byte length
    /// (no trailing newline). Passing `None` restores FFmpeg's default callback.
    pub fn kyav_install_log_sink(
        sink: Option<unsafe extern "C" fn(level: c_int, line: *const c_char, len: usize)>,
    );
}
