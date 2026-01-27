// Project Kyber: average.rs
// Copyright © 2022-2026 Kyber SAS
// SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0
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

pub(crate) struct Average {
    pub(crate) avg: f32,
    range: u32,
    count: u32,
}

impl Average {
    pub(crate) fn new(range: u32) -> Self {
        Self {
            avg: 0f32,
            range,
            count: 0,
        }
    }

    pub(crate) fn push(&mut self, value: f32) {
        if self.count < self.range {
            self.count += 1;
        }

        self.avg = ((self.count - 1) as f32 * self.avg + value) / self.count as f32;
    }
}
