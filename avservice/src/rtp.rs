use std::env;
use std::io;
use std::time::Duration;

use log::{info, warn};
use tokio::{fs, time};

use avservice_types::VideoCodec;
use libkypc::EventSender;
use txproto_rs::{Dictionary, Txproto};

use crate::config;
use crate::utils;
use crate::video::{VideoConfig, VideoEncoder};
use crate::{audio, video};
use crate::{Context, Error, Event, Result};

fn create_muxer(
    ctx: &Txproto,
    ip: &str,
    port: u16,
    sdp_file: Option<&str>,
) -> Result<txproto_rs::Handle> {
    let out_url = format!("rtp://{ip}:{port}");

    let options = Dictionary::from_entries(&[("max_interleave_delta", 0).into()])?;

    let mut init_opts = vec![("dump_info", true).into(), ("low_latency", true).into()];

    if let Some(sdp_file) = sdp_file {
        init_opts.push(("sdp_file", sdp_file).into());
    }

    let init_opts = Dictionary::from_entries(&init_opts)?;

    let muxer = ctx.create_muxer(&out_url, "rtp", Some(options), Some(init_opts))?;
    Ok(muxer)
}

pub(crate) async fn generate_sdp(
    display_id: u32,
    encoder: VideoEncoder,
    codec: VideoCodec,
    yuv444: bool,
    intra_refresh: bool,
    encoder_config: &Option<config::EncoderConfig>,
) -> Result<String> {
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
    let ctx = Txproto::new()?;
    ctx.set_epoch_offset(0)?;

    let sdp_path_str = utils::path_to_string(&sdp_path)?;
    let muxer = create_muxer(&ctx, "localhost", 30_000, Some(&sdp_path_str))?;

    let video_config = VideoConfig {
        display_id,
        encoder,
        bitrate: 20_000_000,
        codec,
        yuv444,
        intra_refresh,
        encoder_config: encoder_config.clone(),
    };
    video::configure(&ctx, video_config, muxer, None)?;

    ctx.commit()?;

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

    drop(ctx);

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
    video_config: VideoConfig,
    ip: String,
    port: u16,
    sender: EventSender<Event>,
) -> Result<Context> {
    let ctx = Txproto::new()?;
    ctx.set_epoch_offset(0)?;

    let muxer = create_muxer(&ctx, &ip, port, None)?;
    let events = video::configure(&ctx, video_config, muxer, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        events: Some(events),
    })
}

pub(crate) fn start_audio(ip: String, port: u16, sender: EventSender<Event>) -> Result<Context> {
    let ctx = Txproto::new()?;
    ctx.set_epoch_offset(0)?;

    let muxer = create_muxer(&ctx, &ip, port, None)?;
    let events = audio::configure(&ctx, muxer, Some(sender))?;

    ctx.commit()?;

    Ok(Context {
        ctx,
        events: Some(events),
    })
}
