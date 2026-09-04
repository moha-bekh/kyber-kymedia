// Project Kyber: watermark.rs
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

//! Watermark closure for libavconv filter graphs.
//!
//! Builds a `kywatermark::Watermark` instance and returns a closure suitable
//! for `FilterGraphBuilder::on_frame_in`. The closure fires on each frame
//! entering the graph (before any filter is applied) and runs the GPU
//! compositing pipeline in place.

use std::path::Path;

use log::error;

use libavconv_rs::HwDeviceRef;
use libavconv_rs::sys;

use super::Result;
use crate::config::WatermarkConfig;

/// Build a `FilterGraphBuilder::on_frame_in` closure that composites the
/// watermark onto each incoming frame.
///
/// Errors from `kywatermark_apply` are logged but do **not** propagate up :
/// dropping a frame on watermark failure would stall the encoder, so we
/// prefer a corrupted watermark to a stalled stream.
pub(crate) fn watermark_hook(
    hw_device: &HwDeviceRef,
    cfg: &WatermarkConfig,
) -> Result<impl FnMut(Option<&mut sys::AVFrame>, u32, &mut u32) -> i32 + Send + 'static> {
    // `kywatermark` owns the FFI: `Watermark::new` bridges the two *-sys crates'
    // ABI-identical AVBufferRef behind a safe API, so this module stays
    // unsafe-free.
    let mut watermark = kywatermark::Watermark::new(hw_device)?;

    watermark.add(&kywatermark::WatermarkParams {
        image_path: Path::new(&cfg.file),
        x: cfg.x,
        y: cfg.y,
        opacity: 1.0,
    })?;

    Ok(
        move |frame: Option<&mut sys::AVFrame>, flags_in: u32, flags_out: &mut u32| {
            let Some(frame) = frame else { return 0 };

            if flags_in & sys::AVConvFGHookFlagsIn_AVCONV_FG_HOOK_FLAG_IN_HEARTBEAT != 0 {
                *flags_out |= sys::AVConvFGHookFlagsOut_AVCONV_FG_HOOK_FLAG_OUT_HEARTBEAT;
                return 0;
            }

            if let Err(err) = watermark.apply_frame(frame) {
                error!("kywatermark apply failed: {err:?}");
            }
            0
        },
    )
}
