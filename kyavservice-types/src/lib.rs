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

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum VideoCodec {
    H264,
    HEVC,
    AV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Display {
    pub id: u32,
    pub name: String,
    pub height: i32,
    pub width: i32,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Command {
    EnumerateDisplays,
    RtpGenerateVideoSdp {
        display_id: u32,
        yuv444: bool,
        intra_refresh: Option<bool>,
    },
    RtpStartVideo {
        display_id: u32,
        ip: String,
        port: u16,
        bitrate: u32,
        yuv444: bool,
        intra_refresh: Option<bool>,
    },
    RtpStartAudio {
        ip: String,
        port: u16,
    },
    KymuxStartVideo {
        display_id: u32,
        uri: String,
        bitrate: u32,
        codec: VideoCodec,
        yuv444: bool,
        intra_refresh: Option<bool>,
        metrics_uri: Option<String>,
    },
    KymuxStartAudio {
        uri: String,
    },
    VideoSetBitrate {
        bitrate: i32,
    },
    VideoForceIdr,
    Stop,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Response {
    None,
    DisplayList { displays: Vec<Display> },
    RtpSdpGenerated { sdp: String },
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum PipelineType {
    Audio,
    Video,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum PipelineStage {
    Initialization,
    Acquisition,
    Encoder,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum PipelineEventType {
    Error,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PipelineEvent {
    pub type_: PipelineType,
    pub stage: PipelineStage,
    pub event: PipelineEventType,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Event {
    DisplayListUpdated { displays: Vec<Display> },
    PipelineEvent(PipelineEvent),
}

pub struct IpcApi;

impl libkypc::IpcApi for IpcApi {
    type Event = Event;
    type Command = Command;
    type Response = Result<Response>;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
