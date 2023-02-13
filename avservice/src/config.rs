use log::{error, info};
use serde::Deserialize;
use tokio::fs;

use crate::utils;
use crate::{Error, Result};

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    pub(crate) encoder: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            encoder: "nvenc".into(),
        }
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
