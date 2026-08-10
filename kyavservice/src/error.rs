// Project Kyber: error.rs
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

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("libkypc error: {source:?}")]
    LibypcError {
        #[from]
        source: libkypc::Error,
    },
    #[error("Failed to delete Video SDP file: {reason:?}")]
    RtpVideoSdpFileDeleteFailed { reason: std::io::Error },
    #[error("RTP Video SDP file hasn't been found")]
    RtpVideoSdpFileNotFound,
    #[error("RTP Video SDP file parse failed")]
    RtpVideoSdpParseFailed,
    #[error("txproto error: {source:?}")]
    TxprotoError {
        #[from]
        source: txproto_rs::Error,
    },
    #[error("No default audio monitor found")]
    AudioNoDefaultMonitor,
    #[error("OsString '{s:?}' conversion to string failed")]
    OsStringConversion { s: std::ffi::OsString },
    #[error("Configuration file is not valid UTF-8: {reason}")]
    ConfigInvalidUtf8 { reason: std::string::FromUtf8Error },
    #[error("Failed to parse configuration file: {reason}")]
    ConfigParseFail { reason: toml::de::Error },
    #[error("Failed to load configuration: {reason}")]
    ConfigLoadFail { reason: toml::de::Error },
    #[error("Failed to write configuration: {reason}")]
    ConfigWriteFail { reason: toml::ser::Error },
    #[error("Thread has panicked")]
    ThreadPanicked,
    #[error("IO Error: {source:?}")]
    IO {
        #[from]
        source: std::io::Error,
    },
    #[error("kycom client error: {source:?}")]
    KycomClient {
        #[from]
        source: kycom::ProtocolError,
    },
    #[error("Integer conversion error: {source:?}")]
    TryFromInt {
        #[from]
        source: std::num::TryFromIntError,
    },
    #[error("rmp encode error: {source:?}")]
    RmpEncode {
        #[from]
        source: rmp_serde::encode::Error,
    },
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_: std::sync::PoisonError<T>) -> Error {
        Error::ThreadPanicked
    }
}

pub type Result<T> = std::result::Result<T, Error>;
