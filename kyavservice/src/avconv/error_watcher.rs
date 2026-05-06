// Project Kyber: error_watcher.rs
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

use log::{info, warn};

use kyavservice_types::{
    Event, IpcApi, PipelineEvent, PipelineEventType, PipelineStage, PipelineType,
};
use std::sync::Arc;

use libavconv_rs::{AvconvContext, DataSink, ErrorWatcher, SinkPacket, WaitOutcome};
use libkypc::EventSender;

/// Spawn the per-pipeline `avconv_wait` watcher. Maps libavconv outcomes onto
/// `PipelineEvent`s for delivery via `sender`.
pub(crate) fn spawn(
    ctx: Arc<AvconvContext>,
    sender: EventSender<IpcApi>,
    type_: PipelineType,
) -> ErrorWatcher {
    ctx.watch_errors(outcome_handler(sender, type_))
}

/// Same, for a pipeline that terminates in a retrieving data sink: the watcher
/// thread also drains `sink`, handing each item to `on_item`.
pub(crate) fn spawn_with_sink<F>(
    ctx: Arc<AvconvContext>,
    sink: Arc<DataSink>,
    on_item: F,
    sender: EventSender<IpcApi>,
    type_: PipelineType,
) -> ErrorWatcher
where
    F: FnMut(SinkPacket<'_>) -> Result<(), i32> + Send + 'static,
{
    ctx.watch_packet_sink(sink, on_item, outcome_handler(sender, type_))
}

fn outcome_handler(
    sender: EventSender<IpcApi>,
    type_: PipelineType,
) -> impl FnMut(WaitOutcome) + Send + 'static {
    move |outcome| match outcome {
        WaitOutcome::Eof => info!("{type_:?} pipeline ended cleanly"),
        WaitOutcome::Error(ret) => {
            warn!("{type_:?} pipeline reported runtime failure: avconv_wait returned {ret}");
            sender.send(Event::PipelineEvent(PipelineEvent {
                type_,
                stage: PipelineStage::Encoder,
                event: PipelineEventType::Error,
            }));
        }
    }
}
