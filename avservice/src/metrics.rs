use kycom::ProtocolEndpoint;
use log::warn;
use serde::Serialize;
use tokio::{sync::mpsc, task};

use crate::Result;

// Must match struct Metrics in libclient::kymux_backend::metrics
#[derive(Debug, Serialize)]
pub(crate) struct Metrics {
    pub(crate) key: String,
    pub(crate) value: i64,
}

async fn forward_task(mut rx: mpsc::Receiver<Vec<Metrics>>, uri: String) -> Result<()> {
    let addr = uri.parse::<kycom::KyComAddr>()?;
    let kycom_client = kycom::MetricsServerEndpoint::connect(addr).await?;
    let mut ipc = kycom_client.ready().await?;

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
