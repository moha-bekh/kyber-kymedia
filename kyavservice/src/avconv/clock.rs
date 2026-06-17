// Project Kyber: clock.rs
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

use libavconv_rs::{GrabCaptureBuilder, time};

use crate::config::ClockType;

/// Configure the pts epoch on a grab capture before it is built.
///
/// - `ZeroedOrigin`: sets epoch to the current monotonic clock so that
///   `pts` starts near zero at session start.
/// - `System`: sets epoch so that `pts` approximates the Unix wall-clock
///   time (us since 1970-01-01).
pub(crate) fn configure_capture_clock(capture: &mut GrabCaptureBuilder, clock_type: &ClockType) {
    let epoch = match clock_type {
        ClockType::ZeroedOrigin => time::gettime_relative(),
        ClockType::System => time::gettime_relative() - time::gettime(),
    };
    capture.set_epoch(epoch);
}
