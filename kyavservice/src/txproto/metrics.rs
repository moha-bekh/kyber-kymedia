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

use crate::txproto::Result;

// Must match struct Metrics in libclient::kymux_backend::metrics
#[derive(Debug, Serialize)]
pub(crate) struct Metrics {
    pub(crate) key: String,
    pub(crate) value: i64,
}

async fn forward_task(mut rx: mpsc::Receiver<Vec<Metrics>>, uri: String) -> Result<()> {
    let addr = uri.parse::<kycom::KyComAddr>()?;
    let kycom_client = kycom::Channel::connect(addr).into_endpoint();
    let mut ipc: kycom::ProtocolSend<_> = kycom_client.ready().await?;

    while let Some(metrics) = rx.recv().await {
        let buf = rmp_serde::to_vec(&metrics)?;
        let packet = kycom::MetricsPacket {
            payload: buf.into(),
        };
        ipc.send(packet).await?;
    }

    Ok(())
}

pub(crate) fn forward(ctx: &txproto_rs::Txproto, uri: &str) -> Result<()> {
    let (tx, rx) = mpsc::channel::<Vec<Metrics>>(32);

    ctx.set_metrics_cb(move |metrics| {
        let metrics: Vec<_> = metrics
            .iter()
            .map(|metrics| Metrics {
                key: metrics.key.into(),
                value: metrics.value,
            })
            .collect();

        let ret = tx.try_send(metrics);
        if let Err(err) = ret {
            warn!("Failed to send Metrics: {err:?}");
        }
    })?;

    let uri = uri.to_string();
    task::spawn(async move {
        let ret = forward_task(rx, uri).await;
        if let Err(err) = ret {
            warn!("Metrics forward task ended: {err:?}");
        }
    });

    Ok(())
}
