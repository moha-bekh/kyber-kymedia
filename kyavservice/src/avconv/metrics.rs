// Project Kyber: metrics.rs
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

use log::warn;
use serde::Serialize;
use tokio::{sync::mpsc, task};

use libavconv_rs::sys;

use super::Result;

// Must match struct Metrics in libclient::kymux_backend::metrics
#[derive(Debug, Serialize)]
pub(crate) struct Metrics {
    pub(crate) key: String,
    pub(crate) value: i64,
}

#[derive(Clone)]
pub(crate) struct MetricsForwarder {
    tx: mpsc::Sender<Vec<Metrics>>,
}

impl MetricsForwarder {
    pub(crate) fn record(&self, metrics: Vec<Metrics>) {
        if let Err(err) = self.tx.try_send(metrics) {
            warn!("Failed to send Metrics: {err:?}");
        }
    }

    /// Emit a `(pts, key=av_gettime_relative())` batch the canonical txproto
    /// per-stage event shape.
    pub(crate) fn record_event(&self, pts: i64, key: &'static str) {
        let now = libavconv_rs::time::gettime_relative();
        self.record(vec![
            Metrics {
                key: "pts".into(),
                value: pts,
            },
            Metrics {
                key: key.into(),
                value: now,
            },
        ]);
    }
}

async fn forward_task(mut rx: mpsc::Receiver<Vec<Metrics>>, uri: String) -> Result<()> {
    let addr = uri.parse::<kycom::KyComAddr>()?;
    let kycom_client = kycom::Channel::connect(addr).into_endpoint();
    let mut ipc: kycom::MetricsServerProtocol = kycom_client.ready().await?;

    while let Some(metrics) = rx.recv().await {
        let buf = rmp_serde::to_vec(&metrics)?;
        let packet = kycom::MetricsPacket {
            payload: buf.into(),
        };
        ipc.send(packet).await?;
    }

    Ok(())
}

pub(crate) fn forward(uri: &str) -> Result<MetricsForwarder> {
    let (tx, rx) = mpsc::channel::<Vec<Metrics>>(32);

    let uri = uri.to_string();
    task::spawn(async move {
        if let Err(err) = forward_task(rx, uri).await {
            warn!("Metrics forward task ended: {err:?}");
        }
    });

    Ok(MetricsForwarder { tx })
}

/// Returns a closure that emits `(pts, acquired)` for each captured frame.
/// Use as `builder.on_frame(capture_metrics_hook(fwd))`.
///
/// Heartbeats are skipped: they assert the *absence* of new data up to their
/// pts (DXGI emits one per desktop-duplication acquire timeout), so counting
/// them as acquisitions would report frames that were never captured.
///
/// Returns `AVERROR(ENOSYS)` so libavgrab takes the hook-less default path:
/// the original frame is moved into the downstream queue and unref-ed in
/// place. Returning 0 instead would route through `cp->hook_frame`, leaving
/// the original capture frame ref-counted on the next iteration and leaking
/// hwframe pool slots (observed as "Static surface pool size exceeded").
pub(crate) fn capture_metrics_hook(
    forwarder: MetricsForwarder,
) -> impl FnMut(&mut u32, &mut sys::AVFrame, u32, &mut u32) -> i32 + Send + 'static {
    move |_stream_idx, frame, flags_in, _flags_out| {
        if flags_in & libavconv_rs::CAPTURE_HOOK_FLAG_IN_HEARTBEAT == 0 {
            forwarder.record_event(frame.pts, "acquired");
        }
        -libc::ENOSYS
    }
}
