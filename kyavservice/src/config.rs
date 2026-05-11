// Project Kyber: config.rs
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

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use log::info;
use serde::Deserialize;

/// Errors of the shared config module. Public because `parse`, `from_table`
/// and `kyavserver_log_extras` are called from the binary before either
/// backend (and therefore either backend's `Error`) is in play; each backend
/// bridges this into its own `Error` with a `From` impl.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read configuration file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Configuration file is not valid UTF-8: {0}")]
    InvalidUtf8(std::string::FromUtf8Error),
    #[error("Failed to parse configuration file: {0}")]
    Parse(toml::de::Error),
    #[error("Failed to load configuration: {0}")]
    Load(toml::de::Error),
    #[error("Failed to write configuration: {0}")]
    Write(toml::ser::Error),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::upper_case_acronyms)]
pub(crate) enum VideoEncoder {
    AMF,
    NVENC,
    QSV,
    VAAPI,
    X264,
    VIDEOTOOLBOX,
}

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

#[cfg(feature = "watermark")]
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct WatermarkConfig {
    pub file: String,
    #[serde(default = "default_position")]
    pub x: f32, // Horizontal position [-1.0 = left, 0.0 = center, 1.0 = right]
    #[serde(default = "default_position")]
    pub y: f32, // Vertical position [-1.0 = top, 0.0 = center, 1.0 = bottom]
}

#[cfg(feature = "watermark")]
fn default_position() -> f32 {
    0.0 // Center
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum GrabBackend {
    #[cfg(target_os = "linux")]
    NvFBC,
    #[cfg(target_os = "linux")]
    Drm,
    #[cfg(target_os = "linux")]
    Xcb,
    #[cfg(target_os = "linux")]
    Wlroots,
}

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    encoder: Option<VideoEncoder>,
    intra_refresh: Option<bool>,
    minimum_bitrate: Option<u32>,
    pub(crate) video_encoder_config: Option<EncoderConfig>,
    clock_type: Option<ClockType>,
    #[cfg(feature = "watermark")]
    pub(crate) watermark: Option<WatermarkConfig>,
    #[cfg(target_os = "linux")]
    pub(crate) grab_backend: Option<GrabBackend>,
}

impl Config {
    pub(crate) fn encoder(&self) -> VideoEncoder {
        #[cfg(target_os = "macos")]
        let default = VideoEncoder::VIDEOTOOLBOX;
        #[cfg(not(target_os = "macos"))]
        let default = VideoEncoder::NVENC;
        self.encoder.unwrap_or(default)
    }

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
    pub(crate) fn grab_backend(&self) -> GrabBackend {
        self.grab_backend.unwrap_or(GrabBackend::NvFBC)
    }
}

fn get_resource_path(resource_name: &str) -> PathBuf {
    if let Ok(mut path) = std::env::current_exe() {
        path.pop();
        path.push(resource_name);
        path
    } else {
        PathBuf::from(resource_name)
    }
}

const SECTION: &str = "kyavserver";

/// Extra log filters used by `kyavserver`: separate level for the txproto
/// pipeline target (capture, encoders, FFmpeg internals routed through it).
/// When `None`, the caller's configured root level is used.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct KyavserverLogExtras {
    pub txproto_log_level: Option<log::LevelFilter>,
}

/// Parse the kyavserver-specific log extras from the `[kyavserver]` section.
/// Runs before the logger is set up, so it must not log; the caller reports
/// the error once logging is available.
pub fn kyavserver_log_extras(table: &toml::Table) -> Result<KyavserverLogExtras> {
    let Some(section) = table.get(SECTION) else {
        return Ok(KyavserverLogExtras::default());
    };
    section.clone().try_into().map_err(ConfigError::Parse)
}

/// Read and parse the configuration file, without deserializing it. Returns
/// an empty table and no path when the file does not exist. Runs before the
/// logger is set up, so it must not log; the caller reports the returned path
/// once logging is available.
///
/// `KYBER_CONFIG` overrides the file location; a set-but-unreadable path is a
/// hard error, an unset default path only falls back to defaults.
pub fn parse() -> Result<(toml::Table, Option<PathBuf>)> {
    let (path, required) = match std::env::var_os("KYBER_CONFIG") {
        Some(path) => (PathBuf::from(path), true),
        None => (get_resource_path("kyber_config.toml"), false),
    };

    let content = match fs::read(&path) {
        Ok(content) => content,
        Err(_) if !required => return Ok((toml::Table::new(), None)),
        Err(err) => return Err(err.into()),
    };

    let content = String::from_utf8(content).map_err(ConfigError::InvalidUtf8)?;

    let table = content.parse().map_err(ConfigError::Parse)?;

    Ok((table, Some(path)))
}

pub fn from_table(table: &toml::Table) -> Result<Config> {
    let Some(kyavserver_conf) = table.get(SECTION) else {
        info!("No kyavserver section found, using defaults");
        return Ok(Config::default());
    };

    kyavserver_conf
        .clone()
        .try_into()
        .map_err(ConfigError::Load)
}

pub(crate) fn dump_encoder_config(
    encoder_type: EncoderType,
    entries: &[(&str, toml::Value)],
) -> Result<(String, String)> {
    let encoder_type = match encoder_type {
        EncoderType::Audio => "audio",
        EncoderType::Video => "video",
    };

    let map: HashMap<_, _> = entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect();

    let encoder_config = toml::to_string(&map).map_err(ConfigError::Write)?;

    Ok((
        format!("kyavserver.{encoder_type}_encoder_config"),
        encoder_config,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_extras_default_when_section_missing() {
        let extras = kyavserver_log_extras(&toml::Table::new()).unwrap();
        assert!(extras.txproto_log_level.is_none());
    }

    #[test]
    fn log_extras_parses_level_and_ignores_other_keys() {
        let table: toml::Table = "[kyavserver]\nencoder = \"NVENC\"\ntxproto_log_level = \"warn\""
            .parse()
            .unwrap();
        let extras = kyavserver_log_extras(&table).unwrap();
        assert_eq!(extras.txproto_log_level, Some(log::LevelFilter::Warn));
    }

    #[test]
    fn log_extras_invalid_level_is_fatal() {
        let table: toml::Table = "[kyavserver]\ntxproto_log_level = \"nope\""
            .parse()
            .unwrap();
        assert!(kyavserver_log_extras(&table).is_err());
    }
}
