// Project Kyber: mod.rs
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

//! Legacy txproto-based AV pipeline.  Kept as a runtime-selectable
//! fallback while the libavconv backend is being validated.

mod audio;
mod clock;
mod displays;
mod error;
mod kymux;
mod metrics;
mod rtp;
mod run;
mod utils;
mod video;

use txproto_rs::Txproto;

use crate::TargetLogLevel;

pub(crate) use clock::configure_clock;
pub use error::{Error, Result};
pub(crate) use kyavservice_types::Event;
pub(crate) use run::{Context, run};
pub(crate) use video::VideoConfig;

/// Opens a `Txproto` context at the `txproto` target's level.
pub(crate) fn open_txproto(log_levels: &dyn TargetLogLevel) -> Result<Txproto> {
    Ok(Txproto::new(log_levels.level_for("txproto"))?)
}
