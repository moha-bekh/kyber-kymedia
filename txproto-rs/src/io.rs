// Project Kyber: io.rs
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

use txproto_sys as sys;

use crate::Rational;

#[derive(Debug, PartialEq)]
pub enum IOSysType {
    None,
    VideoDisplay,
    AudioMicrophone,
    AudioMonitor,
    AudioOutput,
}

impl IOSysType {
    pub fn from(c_type: sys::IOType) -> Option<Self> {
        match c_type {
            sys::IOType_SP_IO_TYPE_NONE => Some(Self::None),
            sys::IOType_SP_IO_TYPE_VIDEO_DISPLAY => Some(Self::VideoDisplay),
            sys::IOType_SP_IO_TYPE_AUDIO_MICROPHONE => Some(Self::AudioMicrophone),
            sys::IOType_SP_IO_TYPE_AUDIO_MONITOR => Some(Self::AudioMonitor),
            sys::IOType_SP_IO_TYPE_AUDIO_OUTPUT => Some(Self::AudioOutput),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum IOEntryType {
    None,
    Video {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        scale: i32,
        framerate: Rational,
    },
    Audio {
        sample_rate: i32,
        sample_fmt: i32,
        channels: i32,
        volume: f32,
    },
}

#[derive(Debug)]
pub struct IOSysEntry {
    pub name: String,
    pub sys_type: IOSysType,
    pub entry_type: IOEntryType,
    pub identifier: u32,
    pub api_id: u32,
    pub is_default: bool,
    pub is_unplugged: bool,
}
