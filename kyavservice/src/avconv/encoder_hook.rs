// Project Kyber: encoder_hook.rs
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

use std::sync::Arc;

use log::debug;

use libavconv_rs::{EncoderBuilder, sys};

use super::encoder_control::EncoderControl;
use super::metrics::MetricsForwarder;

/// Install the metrics + runtime-control closures on an `EncoderBuilder`.
///
/// One builder gets three closures because libavconv only allows a single
/// `AVConvEncoderHooks` per encoder:
///   - `on_frame_in`: apply pending bitrate / forced-IDR commands, emit
///     `(pts, encoding)`.
///   - `on_packet_out`: emit `(pts, encoded)`.
///   - `on_stream_params`: enable `forced-idr` AVOption on codecs that
///     expose it, and fall back to a nominal frame rate when the capture
///     didn't declare one.
pub(crate) fn install(
    enc: &mut EncoderBuilder,
    forwarder: Option<MetricsForwarder>,
    control: Arc<EncoderControl>,
) {
    let forwarder_frame_in = forwarder.clone();
    let control_frame_in = Arc::clone(&control);
    enc.on_frame_in(move |enc_ctx, frame, _flags_in, _flags_out| {
        let Some(frame) = frame else { return 0 };

        if let Some(bitrate) = control_frame_in.take_pending_bitrate() {
            enc_ctx.bit_rate = bitrate;
        }

        if control_frame_in.take_force_idr() {
            frame.pict_type = sys::AVPictureType_AV_PICTURE_TYPE_I;
        }

        if let Some(forwarder) = &forwarder_frame_in {
            forwarder.record_event(frame.pts, "encoding");
        }
        0
    });

    let forwarder_pkt_out = forwarder;
    enc.on_packet_out(move |_enc_ctx, pkt, _flags_in, _flags_out| {
        let Some(pkt) = pkt else { return 0 };
        if let Some(forwarder) = &forwarder_pkt_out {
            forwarder.record_event(pkt.pts, "encoded");
        }
        0
    });

    // Enable forced-IDR on the encoder so subsequent `frame->pict_type = I`
    // overrides are honoured as true IDRs.
    //
    // The AVOption is spelt `forced-idr` (dash) on nvenc and x264 but
    // `forced_idr` (underscore) on AMF and QSV - set both, since
    // `av_opt_set` silently ignores the spelling not recognised by the
    // active codec. VAAPI and videotoolbox don't expose the option and
    // derive IDR directly from `pict_type`.
    enc.on_stream_params(|enc_ctx, par, _flags| {
        if par.frame_rate.num <= 0 || par.frame_rate.den <= 0 {
            par.frame_rate = sys::AVRational { num: 60, den: 1 };
        }

        for name in ["forced-idr", "forced_idr"] {
            if let Err(err) = libavconv_rs::codec_context_set_option(enc_ctx, name, "1") {
                debug!("encoder_on_stream_params: {name}=1 not applied ({err:?})");
            }
        }
        0
    });
}
