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

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    pub(crate) encoder: VideoEncoder,
    intra_refresh: Option<bool>,
    pub(crate) video_encoder_config: Option<EncoderConfig>,
    clock_type: Option<ClockType>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            encoder: VideoEncoder::NVENC,
            intra_refresh: None,
            video_encoder_config: None,
            clock_type: None,
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

    let Some(avserver_conf) = content.get("avserver") else {
        info!("No avserver section found. Use default configuration");
        return Ok(Config::default());
    };

    avserver_conf
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
        format!("avserver.{encoder_type}_encoder_config"),
        encoder_config,
    ))
}
