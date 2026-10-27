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
#[cfg(target_os = "linux")]
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use log::{info, warn};
use serde::Deserialize;

#[cfg(target_os = "linux")]
use kyavservice_types::VideoCodec;

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
    #[error("Invalid configuration: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

/// Which engine handles the AV pipeline.  Selected at startup from the
/// `[kyavserver] backend = "..."` field in `kyber_config.toml`.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Backend {
    Libavconv,
    #[default]
    Txproto,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::upper_case_acronyms)]
pub(crate) enum VideoEncoder {
    AMF,
    NVENC,
    QSV,
    VAAPI,
    V4L2M2M,
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
// Off Linux the enum has no variants, so it can have no default either.
#[cfg_attr(target_os = "linux", derive(Default))]
#[serde(rename_all = "lowercase")]
pub(crate) enum GrabBackend {
    #[cfg(target_os = "linux")]
    #[default]
    NvFBC,
    #[cfg(target_os = "linux")]
    Drm,
    #[cfg(target_os = "linux")]
    Xcb,
    #[cfg(target_os = "linux")]
    Wlroots,
    #[cfg(target_os = "linux")]
    Gstreamer,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum VideoSourceMode {
    #[default]
    Raw,
    Encoded,
}

#[cfg(target_os = "linux")]
fn default_raw_format() -> String {
    "NV12".to_string()
}

#[cfg(target_os = "linux")]
fn default_interlace_mode() -> String {
    "progressive".to_string()
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GstreamerVideoSource {
    pub(crate) name: String,
    source: String,
    width: i32,
    height: i32,
    framerate: String,
    #[serde(default = "default_raw_format")]
    format: String,
    #[serde(default = "default_interlace_mode", alias = "interlace-mode")]
    interlace_mode: String,
    output_width: Option<i32>,
    output_height: Option<i32>,
    pub(crate) codec: Option<VideoCodec>,
    #[serde(default)]
    pub(crate) mode: VideoSourceMode,
}

#[cfg(target_os = "linux")]
impl GstreamerVideoSource {
    pub(crate) fn source_size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    pub(crate) fn output_size(&self) -> (i32, i32) {
        (
            self.output_width.unwrap_or(self.width),
            self.output_height.unwrap_or(self.height),
        )
    }

    pub(crate) fn pipeline(&self) -> Result<String> {
        let pipeline = match self.mode {
            VideoSourceMode::Raw => format!(
                "{} ! video/x-raw,width={},height={},framerate={},format={},interlace-mode={}",
                self.source,
                self.width,
                self.height,
                self.framerate,
                self.format,
                self.interlace_mode
            ),
            VideoSourceMode::Encoded => match self.codec {
                Some(VideoCodec::H264) => format!(
                    "{} ! video/x-h264,width={},height={},framerate={} ! h264parse config-interval=-1 ! video/x-h264,stream-format=byte-stream,alignment=au",
                    self.source, self.width, self.height, self.framerate,
                ),
                Some(VideoCodec::HEVC) => format!(
                    "{} ! video/x-h265,width={},height={},framerate={} ! h265parse config-interval=-1 ! video/x-h265,stream-format=byte-stream,alignment=au",
                    self.source, self.width, self.height, self.framerate,
                ),
                Some(VideoCodec::AV1) | None => {
                    return Err(ConfigError::Invalid(format!(
                        "GStreamer encoded video source '{}' must set codec to H264 or HEVC",
                        self.name
                    )));
                }
            },
        };
        Ok(pipeline)
    }

    fn validate_framerate(&self) -> Result<()> {
        let is_fraction = self.framerate.split_once('/').is_some_and(|(num, den)| {
            !num.is_empty()
                && !den.is_empty()
                && num.bytes().all(|b| b.is_ascii_digit())
                && den.bytes().all(|b| b.is_ascii_digit())
                && den != "0"
        });
        if is_fraction {
            Ok(())
        } else {
            Err(ConfigError::Invalid(format!(
                "GStreamer video source '{}' framerate must be a GStreamer fraction like \"30/1\", got '{}'",
                self.name, self.framerate
            )))
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug, Default, Deserialize)]
struct GstreamerConfig {
    default_source: Option<String>,
    #[serde(default)]
    video_sources: Vec<GstreamerVideoSource>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    backend: Backend,
    encoder: Option<VideoEncoder>,
    intra_refresh: Option<bool>,
    minimum_bitrate: Option<u32>,
    pub(crate) video_encoder_config: Option<EncoderConfig>,
    clock_type: Option<ClockType>,
    #[cfg(feature = "watermark")]
    pub(crate) watermark: Option<WatermarkConfig>,
    #[cfg(target_os = "linux")]
    grab_backend: Option<GrabBackend>,
    #[cfg(target_os = "linux")]
    gstreamer: Option<GstreamerConfig>,
}

impl Config {
    pub(crate) fn backend(&self) -> Backend {
        self.backend
    }

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

    /// A bitrate requested during a session (`VideoSetBitrate`), raised to
    /// the minimum like the one the session started with. That covers a
    /// negative request too, which the encoder would otherwise receive.
    pub(crate) fn runtime_bitrate(&self, bitrate: i32) -> i32 {
        let min_bitrate = i32::try_from(self.minimum_bitrate()).unwrap_or(i32::MAX);
        if bitrate < min_bitrate {
            warn!("Bitrate too low. Reconfigure it: {bitrate} -> {min_bitrate}");
            min_bitrate
        } else {
            bitrate
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn grab_backend(&self) -> GrabBackend {
        self.grab_backend.unwrap_or_default()
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn is_gstreamer(&self) -> bool {
        matches!(self.grab_backend(), GrabBackend::Gstreamer)
    }

    #[cfg(target_os = "linux")]
    fn validate_gstreamer_source(source: &GstreamerVideoSource) -> Result<()> {
        if source.name.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "GStreamer video source name cannot be empty".to_string(),
            ));
        }
        if source.source.trim().is_empty() {
            return Err(ConfigError::Invalid(format!(
                "GStreamer video source '{}' has an empty source",
                source.name
            )));
        }
        // Raw mode appends the raw caps to `source`, so it must be a single
        // source element. Encoded mode passes `source` verbatim ahead of the
        // parser, so it may be a full launch fragment (e.g. a hardware
        // V4L2 capture-and-encode chain: `v4l2src ! NV12 caps ! v4l2h264enc`).
        if matches!(source.mode, VideoSourceMode::Raw) && source.source.contains('!') {
            return Err(ConfigError::Invalid(format!(
                "GStreamer raw video source '{}' source must be a single source element; caps are configured separately",
                source.name
            )));
        }
        if source.width <= 0 || source.height <= 0 {
            return Err(ConfigError::Invalid(format!(
                "GStreamer video source '{}' width and height must be positive",
                source.name
            )));
        }
        source.validate_framerate()?;

        match (source.output_width, source.output_height) {
            (Some(width), Some(height)) if width > 0 && height > 0 => {}
            (Some(_), Some(_)) => {
                return Err(ConfigError::Invalid(format!(
                    "GStreamer video source '{}' output_width and output_height must be positive",
                    source.name
                )));
            }
            (None, None) => {}
            _ => {
                return Err(ConfigError::Invalid(format!(
                    "GStreamer video source '{}' must set both output_width and output_height, or neither",
                    source.name
                )));
            }
        }

        match source.mode {
            VideoSourceMode::Raw => {
                if source.codec.is_some() {
                    return Err(ConfigError::Invalid(format!(
                        "GStreamer raw video source '{}' must not set codec",
                        source.name
                    )));
                }
            }
            VideoSourceMode::Encoded => {
                if !matches!(source.codec, Some(VideoCodec::H264 | VideoCodec::HEVC)) {
                    return Err(ConfigError::Invalid(format!(
                        "GStreamer encoded video source '{}' must set codec to H264 or HEVC",
                        source.name
                    )));
                }
                if source.output_size() != source.source_size() {
                    return Err(ConfigError::Invalid(format!(
                        "GStreamer encoded video source '{}' output size must match source size",
                        source.name
                    )));
                }
            }
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn gstreamer_sources_slice(&self) -> &[GstreamerVideoSource] {
        self.gstreamer
            .as_ref()
            .map(|gstreamer| gstreamer.video_sources.as_slice())
            .unwrap_or_default()
    }

    /// Validate the GStreamer section and move `default_source` to index 0,
    /// so display id N is simply the N-th entry afterwards. Runs once at load
    /// (`from_table`); the accessors below are plain lookups.
    #[cfg(target_os = "linux")]
    fn validate_gstreamer_config(&mut self) -> Result<()> {
        if !self.is_gstreamer() {
            return Ok(());
        }

        let sources = self.gstreamer_sources_slice();
        if sources.is_empty() {
            return Err(ConfigError::Invalid(
                "GStreamer backend requires configured video_sources".to_string(),
            ));
        }

        let mut names = HashSet::new();
        for source in sources {
            Self::validate_gstreamer_source(source)?;
            if !names.insert(source.name.as_str()) {
                return Err(ConfigError::Invalid(format!(
                    "Duplicate GStreamer video source '{}'",
                    source.name
                )));
            }
        }

        let Some(gstreamer) = self.gstreamer.as_mut() else {
            return Ok(());
        };
        let Some(default_source) = gstreamer.default_source.as_deref() else {
            return Ok(());
        };
        let Some(default_idx) = gstreamer
            .video_sources
            .iter()
            .position(|source| source.name == default_source)
        else {
            return Err(ConfigError::Invalid(format!(
                "Unknown default GStreamer source '{default_source}'"
            )));
        };
        let default_source = gstreamer.video_sources.remove(default_idx);
        gstreamer.video_sources.insert(0, default_source);
        Ok(())
    }

    /// Configured GStreamer sources in display-id order (empty unless the
    /// GStreamer backend is selected).
    #[cfg(target_os = "linux")]
    pub(crate) fn gstreamer_video_sources(&self) -> &[GstreamerVideoSource] {
        if self.is_gstreamer() {
            self.gstreamer_sources_slice()
        } else {
            &[]
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn gstreamer_video_source(
        &self,
        display_id: u32,
    ) -> Result<Option<&GstreamerVideoSource>> {
        if !self.is_gstreamer() {
            return Ok(None);
        }

        self.gstreamer_video_sources()
            .get(display_id as usize)
            .map(Some)
            .ok_or_else(|| {
                ConfigError::Invalid(format!("Unknown GStreamer display id {display_id}"))
            })
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

/// Extra log filters used by `kyavserver`: separate levels for the txproto
/// and FFmpeg (libavconv backend) pipeline targets. When `None`, the caller's
/// configured root level is used.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct KyavserverLogExtras {
    pub txproto_log_level: Option<log::LevelFilter>,
    pub ffmpeg_log_level: Option<log::LevelFilter>,
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

    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut config: Config = kyavserver_conf
        .clone()
        .try_into()
        .map_err(ConfigError::Load)?;

    #[cfg(target_os = "linux")]
    config.validate_gstreamer_config()?;

    Ok(config)
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
        assert!(extras.ffmpeg_log_level.is_none());
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
    fn log_extras_parses_ffmpeg_level() {
        let table: toml::Table = "[kyavserver]\nffmpeg_log_level = \"debug\""
            .parse()
            .unwrap();
        let extras = kyavserver_log_extras(&table).unwrap();
        assert_eq!(extras.ffmpeg_log_level, Some(log::LevelFilter::Debug));
    }

    #[test]
    fn runtime_bitrate_is_raised_to_the_minimum() {
        let table: toml::Table = "[kyavserver]\nminimum_bitrate = 8000000".parse().unwrap();
        let config = from_table(&table).unwrap();
        assert_eq!(config.runtime_bitrate(20_000_000), 20_000_000);
        assert_eq!(config.runtime_bitrate(8_000_000), 8_000_000);
        assert_eq!(config.runtime_bitrate(1_000_000), 8_000_000);
        assert_eq!(config.runtime_bitrate(-3_000_000), 8_000_000);
    }

    #[test]
    fn runtime_bitrate_uses_the_default_minimum() {
        let config = Config::default();
        assert_eq!(config.runtime_bitrate(0), 5_000_000);
    }

    #[test]
    fn log_extras_invalid_level_is_fatal() {
        let table: toml::Table = "[kyavserver]\ntxproto_log_level = \"nope\""
            .parse()
            .unwrap();
        assert!(kyavserver_log_extras(&table).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gstreamer_sources_put_configured_default_first() {
        let table: toml::Table = r#"
[kyavserver]
backend = "libavconv"
grab_backend = "gstreamer"

[kyavserver.gstreamer]
default_source = "secondary"

[[kyavserver.gstreamer.video_sources]]
name = "primary"
source = "v4l2src device=/dev/video0"
width = 1920
height = 1080
framerate = "60/1"

[[kyavserver.gstreamer.video_sources]]
name = "secondary"
source = "v4l2src device=/dev/video1"
width = 1280
height = 720
framerate = "30/1"
"#
        .parse()
        .unwrap();

        let config = from_table(&table).unwrap();
        let sources = config.gstreamer_video_sources();

        assert_eq!(sources[0].name, "secondary");
        assert_eq!(sources[1].name, "primary");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gstreamer_encoded_source_builds_h264_byte_stream_pipeline() {
        let table: toml::Table = r#"
[kyavserver]
backend = "libavconv"
grab_backend = "gstreamer"

[kyavserver.gstreamer]

[[kyavserver.gstreamer.video_sources]]
name = "encoded"
source = "v4l2src device=/dev/video0"
width = 1920
height = 1080
framerate = "60/1"
codec = "H264"
mode = "encoded"
"#
        .parse()
        .unwrap();

        let config = from_table(&table).unwrap();
        let source = config.gstreamer_video_source(0).unwrap().unwrap();

        assert_eq!(
            source.pipeline().unwrap(),
            "v4l2src device=/dev/video0 ! video/x-h264,width=1920,height=1080,framerate=60/1 ! h264parse config-interval=-1 ! video/x-h264,stream-format=byte-stream,alignment=au"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gstreamer_encoded_source_builds_hevc_byte_stream_pipeline() {
        let table: toml::Table = r#"
[kyavserver]
backend = "libavconv"
grab_backend = "gstreamer"

[kyavserver.gstreamer]

[[kyavserver.gstreamer.video_sources]]
name = "encoded"
source = "v4l2src device=/dev/video0"
width = 1920
height = 1080
framerate = "60/1"
codec = "HEVC"
mode = "encoded"
"#
        .parse()
        .unwrap();

        let config = from_table(&table).unwrap();
        let source = config.gstreamer_video_source(0).unwrap().unwrap();

        assert_eq!(
            source.pipeline().unwrap(),
            "v4l2src device=/dev/video0 ! video/x-h265,width=1920,height=1080,framerate=60/1 ! h265parse config-interval=-1 ! video/x-h265,stream-format=byte-stream,alignment=au"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gstreamer_encoded_source_rejects_av1() {
        let table: toml::Table = r#"
[kyavserver]
backend = "libavconv"
grab_backend = "gstreamer"

[kyavserver.gstreamer]

[[kyavserver.gstreamer.video_sources]]
name = "encoded"
source = "v4l2src device=/dev/video0"
width = 1920
height = 1080
framerate = "60/1"
codec = "AV1"
mode = "encoded"
"#
        .parse()
        .unwrap();

        let err = from_table(&table).unwrap_err();

        assert!(err.to_string().contains("must set codec to H264 or HEVC"));
        assert!(matches!(err, ConfigError::Invalid(_)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gstreamer_source_rejects_embedded_pipeline() {
        let table: toml::Table = r#"
[kyavserver]
backend = "libavconv"
grab_backend = "gstreamer"

[kyavserver.gstreamer]

[[kyavserver.gstreamer.video_sources]]
name = "invalid"
source = "v4l2src ! videoconvert"
width = 1920
height = 1080
framerate = "60/1"
"#
        .parse()
        .unwrap();

        let err = from_table(&table).unwrap_err();

        assert!(err.to_string().contains("must be a single source element"));
        assert!(matches!(err, ConfigError::Invalid(_)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gstreamer_source_rejects_non_fraction_framerate() {
        let table: toml::Table = r#"
[kyavserver]
backend = "libavconv"
grab_backend = "gstreamer"

[kyavserver.gstreamer]

[[kyavserver.gstreamer.video_sources]]
name = "invalid"
source = "v4l2src device=/dev/video0"
width = 1920
height = 1080
framerate = "60/1 ! fakesink"
"#
        .parse()
        .unwrap();

        let err = from_table(&table).unwrap_err();

        assert!(
            err.to_string()
                .contains("framerate must be a GStreamer fraction")
        );
        assert!(matches!(err, ConfigError::Invalid(_)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gstreamer_backend_requires_video_sources() {
        let table: toml::Table =
            "[kyavserver]\nbackend = \"libavconv\"\ngrab_backend = \"gstreamer\""
                .parse()
                .unwrap();

        let err = from_table(&table).unwrap_err();

        assert!(
            err.to_string()
                .contains("requires configured video_sources")
        );
        assert!(matches!(err, ConfigError::Invalid(_)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gstreamer_encoded_source_rejects_output_scaling() {
        let table: toml::Table = r#"
[kyavserver]
backend = "libavconv"
grab_backend = "gstreamer"

[kyavserver.gstreamer]

[[kyavserver.gstreamer.video_sources]]
name = "encoded"
source = "v4l2src device=/dev/video0"
width = 1920
height = 1080
framerate = "60/1"
output_width = 1280
output_height = 720
codec = "H264"
mode = "encoded"
"#
        .parse()
        .unwrap();

        let err = from_table(&table).unwrap_err();

        assert!(
            err.to_string()
                .contains("output size must match source size")
        );
        assert!(matches!(err, ConfigError::Invalid(_)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gstreamer_raw_source_pipeline_includes_caps() {
        let source = GstreamerVideoSource {
            name: "raw".to_string(),
            source: "v4l2src device=/dev/video0".to_string(),
            width: 1920,
            height: 1080,
            framerate: "60/1".to_string(),
            format: "NV12".to_string(),
            interlace_mode: "progressive".to_string(),
            output_width: Some(1280),
            output_height: Some(720),
            codec: None,
            mode: VideoSourceMode::Raw,
        };

        assert_eq!(
            source.pipeline().unwrap(),
            "v4l2src device=/dev/video0 ! video/x-raw,width=1920,height=1080,framerate=60/1,format=NV12,interlace-mode=progressive"
        );
        assert_eq!(source.source_size(), (1920, 1080));
        assert_eq!(source.output_size(), (1280, 720));
    }
}
