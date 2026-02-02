// Project Kyber: config.rs
// Copyright © 2022-2026 Kyber SAS
// SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0
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

use std::collections::HashMap;

use log::{error, info};
use serde::Deserialize;
use tokio::fs;

use txproto_rs::{DictionaryEntry, DictionaryValue};

use crate::utils;
use crate::{Error, Result, VideoEncoder};

pub(crate) type EncoderConfig = HashMap<String, toml::Value>;

#[allow(dead_code)]
pub(crate) enum EncoderType {
    Audio,
    Video,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub(crate) enum ClockType {
    ZeroedOrigin,
    System,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct WatermarkConfig {
    pub file: String,
    #[serde(default = "default_position")]
    pub x: f32, // Horizontal position [-1.0 = left, 0.0 = center, 1.0 = right]
    #[serde(default = "default_position")]
    pub y: f32, // Vertical position [-1.0 = top, 0.0 = center, 1.0 = bottom]
}

fn default_position() -> f32 {
    0.0 // Center
}

impl WatermarkConfig {
    pub(crate) fn position(&self) -> txproto_rs::TxWatermarkPosition {
        txproto_rs::TxWatermarkPosition {
            x: self.x,
            y: self.y,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum GrabBackend {
    #[cfg(target_os = "linux")]
    NvFBC,
    #[cfg(target_os = "linux")]
    Drm,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    pub(crate) encoder: VideoEncoder,
    intra_refresh: Option<bool>,
    minimum_bitrate: Option<u32>,
    pub(crate) video_encoder_config: Option<EncoderConfig>,
    clock_type: Option<ClockType>,
    pub(crate) watermark: Option<WatermarkConfig>,
    #[cfg(target_os = "linux")]
    pub(crate) grab_backend: Option<GrabBackend>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            encoder: VideoEncoder::NVENC,
            intra_refresh: None,
            minimum_bitrate: None,
            video_encoder_config: None,
            clock_type: None,
            watermark: None,
            #[cfg(target_os = "linux")]
            grab_backend: Some(GrabBackend::NvFBC),
        }
    }
}

impl Config {
    pub(crate) fn intra_refresh(&self) -> bool {
        self.intra_refresh.unwrap_or(true)
    }

    pub(crate) fn clock_type(&self) -> ClockType {
        *self.clock_type.as_ref().unwrap_or(&ClockType::ZeroedOrigin)
    }

    pub(crate) fn minimum_bitrate(&self) -> u32 {
        self.minimum_bitrate.unwrap_or(5_000_000)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn grab_backend(&self) -> Option<GrabBackend> {
        self.grab_backend
    }
}

pub(crate) async fn load() -> Result<Config> {
    let path = utils::get_resource_path("kyber_config.toml");

    let Ok(content) = fs::read(&path).await else {
        info!("Use default configuration");
        return Ok(Config::default());
    };

    info!("Using configuration file {path:?}");
    let content = String::from_utf8(content).map_err(|err| {
        error!("Configuration file isn't a valid UTF8 file: {err:?}");
        Error::ConfigParseFail
    })?;

    let content: toml::Table = content.parse().map_err(|err| {
        error!("Configuration file parsing failed: {err:?}");
        Error::ConfigParseFail
    })?;

    let Some(kyavserver_conf) = content.get("kyavserver") else {
        info!("No kyavserver section found. Use default configuration");
        return Ok(Config::default());
    };

    kyavserver_conf
        .clone()
        .try_into()
        .map_err(|err| Error::ConfigLoadFail { reason: err })
}

pub(crate) fn dump_encoder_config(
    encoder_type: EncoderType,
    entries: &[DictionaryEntry],
) -> Result<(String, String)> {
    let encoder_type = match encoder_type {
        EncoderType::Audio => "audio",
        EncoderType::Video => "video",
    };

    let map: HashMap<_, _> = entries
        .iter()
        .map(|entry| {
            let value = match &entry.value {
                DictionaryValue::Bool(b) => toml::value::Value::Boolean(*b),
                DictionaryValue::Int(i) => toml::value::Value::Integer(*i),
                DictionaryValue::String(s) => toml::value::Value::String(s.into()),
            };

            (entry.key.to_string(), value)
        })
        .collect();

    let encoder_config =
        toml::to_string(&map).map_err(|err| Error::ConfigWriteFail { reason: err })?;

    Ok((
        format!("kyavserver.{encoder_type}_encoder_config"),
        encoder_config,
    ))
}
