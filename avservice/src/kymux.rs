use crate::utils;
use crate::{Error, Result};

const LUA_SCRIPT: &str = "avserver_kymux_stream.lua";
const VIDEO_ENTRYPOINT: &str = "main_video";
const AUDIO_ENTRYPOINT: &str = "main_audio";

pub(crate) fn start_video(
    display_id: u32,
    uri: &str,
    encoder: &str,
    bitrate: u32,
    codec: &str,
    yuv444: bool,
    intra_refresh: bool,
) -> Result<txproto_rs::Context> {
    let script = utils::get_resource_path_str(LUA_SCRIPT)?;

    let display_id = format!("{display_id}");
    let bitrate = bitrate.to_string();
    let yuv444 = format!("{yuv444}");
    let intra_refresh = format!("{intra_refresh}");

    let args: Vec<&str> = vec![
        &display_id,
        uri,
        encoder,
        &bitrate,
        codec,
        &yuv444,
        &intra_refresh,
    ];

    let ctx = txproto_rs::Context::new(&script, VIDEO_ENTRYPOINT, Some(args))
        .ok_or(Error::TxprotoCreateFailed)?;

    Ok(ctx)
}

pub(crate) fn start_audio(uri: &str) -> Result<txproto_rs::Context> {
    let script = utils::get_resource_path_str(LUA_SCRIPT)?;

    let args: Vec<&str> = vec![uri];

    let ctx = txproto_rs::Context::new(&script, AUDIO_ENTRYPOINT, Some(args))
        .ok_or(Error::TxprotoCreateFailed)?;

    Ok(ctx)
}
