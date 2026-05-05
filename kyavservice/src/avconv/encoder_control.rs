// Project Kyber: encoder_control.rs
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

//! Runtime encoder control state.
//!
//! Bridges the command-handling task (which runs `VideoSetBitrate` /
//! `VideoForceIdr`) with the libavconv encoder hook (which actually applies
//! the change before the next frame is sent into the encoder).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

/// Sentinel value for `pending_bitrate`: "no change pending".
const NO_PENDING_BITRATE: i64 = -1;

/// Shared between the command handler and the encoder `on_frame_in` hook.
pub(crate) struct EncoderControl {
    pending_bitrate: AtomicI64,
    force_idr: AtomicBool,
}

impl EncoderControl {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            pending_bitrate: AtomicI64::new(NO_PENDING_BITRATE),
            force_idr: AtomicBool::new(false),
        })
    }

    /// Queue a bitrate change; the next encoder hook invocation applies it.
    pub(crate) fn request_bitrate(&self, bitrate: i64) {
        self.pending_bitrate.store(bitrate, Ordering::Release);
    }

    /// Queue a forced IDR on the next incoming frame.
    pub(crate) fn request_force_idr(&self) {
        self.force_idr.store(true, Ordering::Release);
    }

    /// Hook-side: returns the pending bitrate (and clears it), or `None`.
    pub(crate) fn take_pending_bitrate(&self) -> Option<i64> {
        let v = self
            .pending_bitrate
            .swap(NO_PENDING_BITRATE, Ordering::AcqRel);
        (v != NO_PENDING_BITRATE).then_some(v)
    }

    /// Hook-side: returns `true` once if a force-IDR was requested.
    pub(crate) fn take_force_idr(&self) -> bool {
        self.force_idr.swap(false, Ordering::AcqRel)
    }
}
