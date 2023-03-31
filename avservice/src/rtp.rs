use std::env;
use std::io;
use std::time::Duration;

use log::{info, warn};
use tokio::{fs, time};

use crate::utils;
use crate::{Error, Result};

const LUA_SCRIPT: &str = "avserver_rtp_stream.lua";
const VIDEO_ENTRYPOINT: &str = "main_video";
const AUDIO_ENTRYPOINT: &str = "main_audio";

pub(crate) async fn generate_sdp(display_id: u32, yuv444: bool) -> Result<String> {
    // Prepare target file
    let sdp_path = env::temp_dir().join("video.sdp");

    let ret = fs::remove_file(&sdp_path).await;
    if let Err(err) = ret {
        if err.kind() != io::ErrorKind::NotFound {
            warn!("Fail to delete SDP file");
            return Err(Error::RtpVideoSdpFileDeleteFailed { reason: err });
        }
    }

    // Run txproto
    let script = utils::get_resource_path_str(LUA_SCRIPT)?;

    let display_id = format!("{display_id}");
    let yuv444_str = format!("{yuv444}");
    let sdp_path_str = utils::path_to_string(&sdp_path)?;

    let args = vec![
        display_id.as_str(),
        "localhost",
        "30000",
        &yuv444_str,
        &sdp_path_str,
    ];

    let ctx = txproto_rs::Context::new(&script, VIDEO_ENTRYPOINT, Some(args))
        .ok_or(Error::TxprotoCreateFailed)?;

    // Wait for sdp file to be available
    let mut sdp_content = None;

    for _ in 0..50 {
        let ret = fs::read_to_string(&sdp_path).await;
        match ret {
            Ok(file_content) => {
                info!("Got SDP file");
                sdp_content = Some(file_content);
                break;
            }
            Err(_) => {
                info!("Waiting for SDP file");
                time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
    }

    ctx.stop();

    // Parse SDP content
    let Some(sdp_content) = sdp_content else {
        warn!("Couldn't find SDP file {sdp_path:?}");
        return Err(Error::RtpVideoSdpFileNotFound);
    };

    for l in sdp_content.lines() {
        if l.starts_with("a=fmtp") {
            return Ok(l.to_string());
        }
    }

    warn!("Line starting with 'a=fmtp' not found in {sdp_path:?}");

    Err(Error::RtpVideoSdpParseFailed)
}

pub(crate) fn start_video(
    display_id: u32,
    ip: String,
    port: u16,
    yuv444: bool,
) -> Result<txproto_rs::Context> {
    let script = utils::get_resource_path_str(LUA_SCRIPT)?;

    let display_id = format!("{display_id}");
    let port = format!("{port}");
    let yuv444 = format!("{yuv444}");

    let args: Vec<&str> = vec![&display_id, &ip, &port, &yuv444];

    let ctx = txproto_rs::Context::new(&script, VIDEO_ENTRYPOINT, Some(args))
        .ok_or(Error::TxprotoCreateFailed)?;

    Ok(ctx)
}

pub(crate) fn start_audio(ip: String, port: u16) -> Result<txproto_rs::Context> {
    let script = utils::get_resource_path_str(LUA_SCRIPT)?;

    let port = format!("{port}");

    let args: Vec<&str> = vec![&ip, &port];

    let ctx = txproto_rs::Context::new(&script, AUDIO_ENTRYPOINT, Some(args))
        .ok_or(Error::TxprotoCreateFailed)?;

    Ok(ctx)
}
