// Project Kyber: kymux_sink.rs
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

//! Kymux sink fed by a libavconv data sink (`AVConvDSink`).
//!
//! The connection, handshake and wire serialization are delegated to the
//! `kycom` crate (the same component the metrics forwarder uses): we connect
//! a `kymux://host:port/endpoint` server endpoint and push
//! `kymux_types::av::AVPacket` values over it.
//!
//! The pipeline terminates in a coded-payload data sink, drained by the
//! `avconv_wait` watcher thread. That thread cannot `.await`, so it builds
//! owned packets and forwards them over an `mpsc` channel to a tokio task that
//! drives the async `kycom` send protocol. The channel preserves ordering, so
//! the config packet built from the sink's stream parameters always precedes
//! the data packets.

use log::{debug, error, info, warn};
use tokio::sync::mpsc;
use tokio::task;

use kycom::{
    AVPacket, CodecPacket, CodecPacketHeader, MediaPacket, MediaPacketHeader, ProtocolSend,
};
use libavconv_rs::{CodecParams, Packet, SinkPacket, sys};

use super::Result;
use super::metrics::MetricsForwarder;

const KYMUX_CODEC_H264: u32 = 0x6832_3634;
const KYMUX_CODEC_H265: u32 = 0x6832_3635;
const KYMUX_CODEC_AV1: u32 = 0x0061_7631;
const KYMUX_CODEC_OPUS: u32 = 0x6F70_7573;

// AVERROR(errno) = -(errno).  Values are stable on Linux and Windows.
const AVERROR_EIO: i32 = -5; // -EIO
const AVERROR_EINVAL: i32 = -22; // -EINVAL

/// Bounded capacity of the sync->async forwarding channel. Kept small on
/// purpose: the drain callback uses `blocking_send`, so a full channel
/// back-presses the sink - and through it the encoder, since a data sink holds
/// its producer until each item is taken - mirroring the blocking TCP write the
/// synchronous sink used to do, which the encode pipeline relies on for pacing.
const CHANNEL_CAPACITY: usize = 8;

/// Selects which `kycom` server endpoint the sink connects to.
#[derive(Debug, Clone, Copy)]
pub(crate) enum StreamKind {
    Video,
}

/// Messages forwarded from the (synchronous) muxer hooks to the async task.
enum SinkMsg {
    /// Stream parameters: codec id plus (optional) extradata.
    Config { codec: u32, extradata: Vec<u8> },
    /// Encoded packet payload.
    Packet {
        pts_raw: i64,
        is_key: bool,
        payload: Vec<u8>,
    },
}

/// Connect to a kymux server endpoint and return the callback that forwards
/// data-sink items to it, for use with `AvconvContext::watch_packet_sink`.
///
/// `uri` format: `kymux://host:port/endpoint_hex`
pub(crate) fn connect(
    uri: &str,
    kind: StreamKind,
    metrics: Option<MetricsForwarder>,
) -> Result<impl FnMut(SinkPacket<'_>) -> std::result::Result<(), i32> + Send + 'static> {
    let addr = uri.parse::<kycom::KyComAddr>()?;
    info!(
        "Connecting to kymux {} endpoint=0x{:04x} ({kind:?})",
        addr.addr, addr.endpoint_id
    );

    let (tx, rx) = mpsc::channel::<SinkMsg>(CHANNEL_CAPACITY);
    task::spawn(async move {
        if let Err(err) = forward_task(rx, addr, kind, metrics).await {
            error!("kymux_sink: forward task ended: {err:?}");
        }
    });

    Ok(move |item: SinkPacket<'_>| match item {
        SinkPacket::Params(params) => {
            info!("kymux_sink: stream params received");
            send_config(&tx, &params)
        }
        SinkPacket::Packet(pkt) => send_packet(&tx, &pkt),
        // No new data up to the carried pts - nothing to forward.
        SinkPacket::Heartbeat => Ok(()),
    })
}

/// Drives the async `kycom` send protocol, draining the forwarding channel.
async fn forward_task(
    mut rx: mpsc::Receiver<SinkMsg>,
    addr: kycom::KyComAddr,
    kind: StreamKind,
    metrics: Option<MetricsForwarder>,
) -> Result<()> {
    let mut send: ProtocolSend<AVPacket> = match kind {
        StreamKind::Video => {
            kycom::Channel::connect(addr)
                .into_video_server_endpoint()
                .ready()
                .await?
                .send
        }
    };
    info!("kymux_sink: connected to {} ({kind:?})", addr.addr);

    let mut packets_sent: u64 = 0;
    while let Some(msg) = rx.recv().await {
        match msg {
            SinkMsg::Config { codec, extradata } => {
                let ed_size = extradata.len() as u32;
                send.send(AVPacket::Codec(CodecPacket {
                    header: CodecPacketHeader {
                        codec,
                        rotation: 0,
                        frame_size: 0,
                    },
                }))
                .await?;
                send.send(AVPacket::Media(MediaPacket {
                    header: MediaPacketHeader {
                        is_config: true,
                        is_key: false,
                        pts: 0,
                        size: ed_size,
                    },
                    payload: extradata.into(),
                }))
                .await?;
                info!(
                    "kymux_sink: config packet sent (codec=0x{codec:08x}, {ed_size} B extradata)"
                );
            }
            SinkMsg::Packet {
                pts_raw,
                is_key,
                payload,
            } => {
                let size = payload.len() as u32;
                send.send(AVPacket::Media(MediaPacket {
                    header: MediaPacketHeader {
                        is_config: false,
                        is_key,
                        pts: clamp_pts(pts_raw),
                        size,
                    },
                    payload: payload.into(),
                }))
                .await?;

                if packets_sent % 300 == 0 {
                    debug!(
                        "kymux_sink: {} packets sent (last: {size} B, key={is_key})",
                        packets_sent + 1
                    );
                }
                packets_sent += 1;

                if let Some(m) = &metrics {
                    m.record_event(pts_raw, "sent");
                }
            }
        }
    }

    Ok(())
}

/// Build a config message from the sink's stream params and enqueue it.
fn send_config(
    tx: &mpsc::Sender<SinkMsg>,
    params: &CodecParams<'_>,
) -> std::result::Result<(), i32> {
    let codec_id = params.codec_id();

    let Some(codec) = codec_id_to_kymux(codec_id) else {
        error!("kymux_sink: unsupported codec id {codec_id:?}");
        return Err(AVERROR_EINVAL);
    };

    // Extradata may be zero length for AV1/OPUS that embed config in-band.
    let extradata = params.extradata().to_vec();

    // blocking_send back-presses the draining thread instead of dropping; it
    // only errors if the async task has gone away (connection failed/closed).
    if tx
        .blocking_send(SinkMsg::Config { codec, extradata })
        .is_err()
    {
        error!("kymux_sink: config enqueue failed (forward task gone)");
        return Err(AVERROR_EIO);
    }

    Ok(())
}

/// Build a data message from the packet and enqueue it.
fn send_packet(tx: &mpsc::Sender<SinkMsg>, pkt: &Packet<'_>) -> std::result::Result<(), i32> {
    let is_key = pkt.is_key();
    let payload = pkt.data().to_vec();

    // blocking_send back-presses the draining thread (matching the old
    // synchronous TCP write) instead of dropping. It only errors if the forward
    // task is gone, in which case report EIO like the synchronous sink did on a
    // write failure.
    if tx
        .blocking_send(SinkMsg::Packet {
            pts_raw: pkt.pts(),
            is_key,
            payload,
        })
        .is_err()
    {
        warn!("kymux_sink: packet enqueue failed (forward task gone)");
        return Err(AVERROR_EIO);
    }

    Ok(())
}

fn codec_id_to_kymux(codec_id: sys::AVCodecID) -> Option<u32> {
    // bindgen generates codec IDs as AVCodecID_AV_CODEC_ID_xxx constants
    if codec_id == sys::AVCodecID_AV_CODEC_ID_H264 {
        Some(KYMUX_CODEC_H264)
    } else if codec_id == sys::AVCodecID_AV_CODEC_ID_HEVC {
        Some(KYMUX_CODEC_H265)
    } else if codec_id == sys::AVCodecID_AV_CODEC_ID_AV1 {
        Some(KYMUX_CODEC_AV1)
    } else if codec_id == sys::AVCodecID_AV_CODEC_ID_OPUS {
        Some(KYMUX_CODEC_OPUS)
    } else {
        None
    }
}

/// `MediaPacketHeader::serialize` requires pts to fit in 61 bits; clamp
/// AV_NOPTS_VALUE (i64::MIN) to 0 and mask everything else.
fn clamp_pts(pts_raw: i64) -> u64 {
    const PTS_MASK: u64 = 0x1FFF_FFFF_FFFF_FFFF;
    if pts_raw == i64::MIN {
        0
    } else {
        (pts_raw as u64) & PTS_MASK
    }
}
