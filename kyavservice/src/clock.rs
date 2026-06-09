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

use txproto_rs::Txproto;

use crate::Result;
use crate::config::ClockType;

pub(crate) fn configure_clock(ctx: &Txproto, clock_type: &ClockType) -> Result<()> {
    match clock_type {
        ClockType::ZeroedOrigin => {
            ctx.set_epoch_offset(0)?;
        }
        ClockType::System => {
            ctx.set_epoch_system()?;
        }
    }

    Ok(())
}
