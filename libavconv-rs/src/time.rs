// Project Kyber: time.rs
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

//! Safe wrappers over libavutil's monotonic / wall clock helpers.
//!
//! These take no pointers and only read a clock, so the FFI calls are
//! trivially sound - exposing them here keeps `av_gettime*` out of the
//! application layer.

use libavconv_sys as sys;

/// Value of FFmpeg's internal monotonic clock, in microseconds.
///
/// Wraps `av_gettime_relative`. This is the clock grab captures use for their
/// pts epoch (`pts = av_gettime_relative() - epoch`).
pub fn gettime_relative() -> i64 {
    // SAFETY: av_gettime_relative takes no arguments and only reads a clock.
    unsafe { sys::av_gettime_relative() }
}

/// Current wall-clock time, in microseconds since the Unix epoch.
///
/// Wraps `av_gettime`.
pub fn gettime() -> i64 {
    // SAFETY: av_gettime takes no arguments and only reads a clock.
    unsafe { sys::av_gettime() }
}
