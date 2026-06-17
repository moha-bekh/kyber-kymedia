// Project Kyber: lib.rs
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

mod avconv;
mod config;
mod txproto;

use kyavservice_types::IpcApi;
use libkypc::Worker;

pub use txproto::{Error, Result};

pub use config::parse as parse_config;
pub use config::{Config, KyavserverLogExtras, from_table, kyavserver_log_extras};

/// Per-target log level lookup, so a new pipeline backend (e.g. `avconv`,
/// alongside today's `txproto`) only needs its own `level_for` call at its
/// own `Txproto::new`/equivalent site, not a new parameter threaded through
/// every function between `run` and there.
pub trait TargetLogLevel {
    fn level_for(&self, target: &str) -> log::LevelFilter;
}

impl TargetLogLevel for log4ky::LoggerHandle {
    fn level_for(&self, target: &str) -> log::LevelFilter {
        log4ky::LoggerHandle::level_for(self, target)
    }
}

pub async fn run(
    worker: Worker<IpcApi>,
    config: Config,
    log_levels: &dyn TargetLogLevel,
) -> Result<()> {
    txproto::run(worker, config, log_levels).await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use kyavservice_types::{Command, Event, IpcApi, PipelineStage, PipelineType, Response};
    use libkypc::channel::create_pair;

    use crate::{Config, TargetLogLevel};

    /// A fixed level for every target, so tests don't need a real installed
    /// logger just to get a `TargetLogLevel`.
    struct FixedLogLevel(log::LevelFilter);

    impl TargetLogLevel for FixedLogLevel {
        fn level_for(&self, _target: &str) -> log::LevelFilter {
            self.0
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rtp_start_audio_error_reports_audio_pipeline_type() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mut commander, worker) = create_pair::<IpcApi>().unwrap();

                let worker_handle = tokio::task::spawn_local(async move {
                    super::run(
                        worker,
                        Config::default(),
                        &FixedLogLevel(log::LevelFilter::Off),
                    )
                    .await
                });

                let ack = commander
                    .commands
                    .send(Command::RtpStartAudio {
                        ip: "127.0.0.1".into(),
                        port: 0,
                    })
                    .await
                    .unwrap();
                assert!(
                    matches!(ack, Ok(Response::None)),
                    "RtpStartAudio accept carries no event"
                );

                let evt = tokio::time::timeout(Duration::from_secs(5), commander.events.recv())
                    .await
                    .expect("timed out waiting for pipeline error event")
                    .expect("channel closed before receiving event");

                match evt {
                    Event::PipelineEvent(pe) => {
                        assert!(
                            matches!(pe.type_, PipelineType::Audio),
                            "Expected PipelineType::Audio on RTP audio error, got {:?}",
                            pe.type_
                        );
                        assert!(matches!(pe.stage, PipelineStage::Initialization));
                    }
                    other => panic!("Expected PipelineEvent, got {other:?}"),
                }

                let _ = commander.commands.send(Command::Stop).await;
                let _ = worker_handle.await;
            })
            .await;
    }
}
