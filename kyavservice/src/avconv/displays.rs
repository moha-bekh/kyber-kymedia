// Project Kyber: displays.rs
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

use kyavservice_types::Display;
use libavconv_rs::{GrabProviderCtx, GrabSourceSet, sys};

#[cfg(target_os = "windows")]
use libavconv_rs::HwDeviceRef;

use super::Result;
#[cfg(target_os = "linux")]
use crate::config::GstreamerVideoSource;
use crate::config::{Config, GrabBackend};

/// Build and initialise an avgrab provider context for screen sources.
///
/// Used by both the one-shot `enumerate` and the long-lived
/// [`display_watcher`](super::display_watcher) thread.
pub(crate) fn build_provider_ctx(backend: Option<GrabBackend>) -> Result<GrabProviderCtx> {
    let provider_name = super::video::grab_provider_name(backend);
    let provider =
        libavconv_rs::provider_by_name(provider_name).ok_or(libavconv_rs::Error::NotFound {
            name: "grab provider",
        })?;

    let mut ctx = GrabProviderCtx::new(provider)?;

    #[cfg(target_os = "windows")]
    {
        let hw_device = HwDeviceRef::new(sys::AVHWDeviceType_AV_HWDEVICE_TYPE_D3D11VA)?;
        ctx.set_hw_device(&hw_device);
    }

    ctx.init()?;
    Ok(ctx)
}

/// Query the screen-source set from `provider`.
pub(crate) fn fetch_screen_sources(provider: &GrabProviderCtx) -> Result<GrabSourceSet> {
    let payloads: u32 = 1 << sys::AVConvHandlePayload_AVCONV_HANDLE_PAYLOAD_FRAME;
    let src_types: u64 = 1u64 << (sys::AVGrabSourceType_AVGRAB_SOURCE_TYPE_SCREEN as u64);
    Ok(provider.sources(payloads, src_types)?)
}

/// Convert a `GrabSourceSet` into the `Display` wire shape.  Display
/// IDs are assigned by iteration index matches the existing
/// behaviour at the cost of stability across hot-plugs (see todo.md).
pub(crate) fn collect_displays(sources: &GrabSourceSet) -> Vec<Display> {
    sources
        .iter()
        .enumerate()
        .map(|(idx, src)| Display {
            id: idx as u32,
            name: src.name().unwrap_or_else(|| format!("Display {idx}")),
            width: src.width(),
            height: src.height(),
            x: src.x(),
            y: src.y(),
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn gstreamer_display(source: &GstreamerVideoSource, idx: usize) -> Display {
    let (width, height) = source.output_size();
    Display {
        id: idx as u32,
        name: source.name.clone(),
        width,
        height,
        x: 0,
        y: 0,
    }
}

#[cfg(target_os = "linux")]
fn configured_gstreamer_displays(config: &Config) -> Result<Vec<Display>> {
    Ok(config
        .gstreamer_video_sources()?
        .iter()
        .enumerate()
        .map(|(idx, source)| gstreamer_display(source, idx))
        .collect())
}

#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
pub(crate) fn enumerate(config: &Config) -> Result<Vec<Display>> {
    #[cfg(target_os = "linux")]
    if config.is_gstreamer() {
        return configured_gstreamer_displays(config);
    }

    #[cfg(target_os = "linux")]
    let backend = Some(config.grab_backend());
    #[cfg(not(target_os = "linux"))]
    let backend = None;

    let ctx = build_provider_ctx(backend)?;
    let sources = fetch_screen_sources(&ctx)?;
    Ok(collect_displays(&sources))
}
