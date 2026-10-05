// Project Kyber: video.rs
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

use log::{info, warn};

use kyavservice_types::{
    IpcApi, PipelineEvent, PipelineEventType, PipelineStage, PipelineType, VideoCodec,
};
use libkypc::EventSender;
use txproto_rs::{Dictionary, DictionaryEntry, DictionaryValue, Txproto};

use crate::config;
use crate::config::VideoEncoder;
use crate::txproto::{Event, Result};

pub(crate) struct VideoConfig {
    pub(crate) display_id: u32,
    pub(crate) encoder: VideoEncoder,
    pub(crate) bitrate: u32,
    pub(crate) codec: VideoCodec,
    pub(crate) yuv444: bool,
    pub(crate) intra_refresh: bool,
    pub(crate) encoder_config: Option<config::EncoderConfig>,
    pub(crate) clock_type: config::ClockType,
    #[cfg(feature = "watermark")]
    pub(crate) watermark: Option<config::WatermarkConfig>,
    #[cfg(target_os = "linux")]
    pub(crate) grab_backend: config::GrabBackend,
}

#[cfg(feature = "watermark")]
fn watermark_position(cfg: &config::WatermarkConfig) -> txproto_rs::TxWatermarkPosition {
    txproto_rs::TxWatermarkPosition { x: cfg.x, y: cfg.y }
}

fn print_options(encoder: &str, options: &[DictionaryEntry]) {
    let entries: Vec<(&str, toml::Value)> = options
        .iter()
        .map(|entry| {
            let value = match &entry.value {
                DictionaryValue::Bool(b) => toml::Value::Boolean(*b),
                DictionaryValue::Int(i) => toml::Value::Integer(*i),
                DictionaryValue::String(s) => toml::Value::String(s.clone()),
            };
            (entry.key.as_str(), value)
        })
        .collect();

    match config::dump_encoder_config(config::EncoderType::Video, &entries) {
        Ok((section, options)) => {
            info!(
                r#"{encoder} encoder configuration dump:
[{section}]
{options}"#
            );
        }
        Err(err) => {
            warn!("Fail to dump encoder config: {err:?}");
        }
    }
}

fn encoder_config_to_dict(encoder_config: &config::EncoderConfig) -> Vec<DictionaryEntry> {
    warn!("Using custom video encoder configuration");

    encoder_config
        .iter()
        .filter_map(|(k, v)| {
            let v = match v {
                toml::value::Value::Boolean(b) => Some(DictionaryValue::Bool(*b)),
                toml::value::Value::Integer(i) => Some(DictionaryValue::Int(*i)),
                toml::value::Value::String(s) => Some(DictionaryValue::String(s.clone())),
                _ => {
                    warn!("Encoder config '{k}': Unhandled type for value {v:?}");
                    None
                }
            };

            v.map(|v| (k.as_str(), v).into())
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub(crate) fn grab_backend_api_list(grab_backend: config::GrabBackend) -> Vec<&'static str> {
    match grab_backend {
        config::GrabBackend::NvFBC => vec!["nvfbc", "pulse"],
        config::GrabBackend::Drm => vec!["drm", "pulse"],
        config::GrabBackend::Xcb => vec!["xcb", "pulse"],
        config::GrabBackend::Wlroots => vec!["wayland", "pulse"],
        config::GrabBackend::Gstreamer => vec![],
    }
}

#[cfg(target_os = "linux")]
fn grab_backend_to_dict(backend: config::GrabBackend) -> Result<Dictionary> {
    let entries = match backend {
        config::GrabBackend::Gstreamer => {
            return Err(crate::txproto::Error::ConfigInvalid {
                reason: "gstreamer grab_backend requires the libavconv backend".to_string(),
            });
        }
        config::GrabBackend::NvFBC => {
            info!("Using NvFBC grab backend");
            vec![("grab_backend", "nvfbc").into()]
        }
        config::GrabBackend::Drm => {
            info!("Using DRM/KMS grab backend");
            vec![("grab_backend", "drm").into()]
        }
        config::GrabBackend::Xcb => {
            info!("Using XCB grab backend");
            vec![("grab_backend", "xcb").into()]
        }
        config::GrabBackend::Wlroots => {
            info!("Using wlroots grab backend");
            vec![("grab_backend", "wayland").into()]
        }
    };

    Ok(Dictionary::from_entries(&entries)?)
}

fn create_nvenc(
    ctx: &Txproto,
    config: &VideoConfig,
    video_source: &txproto_rs::Handle,
) -> Result<txproto_rs::Handle> {
    // Configure Codec
    let mut options = vec![
        ("preset", "p2").into(),
        ("tune", "ull").into(),
        ("zerolatency", true).into(),
        ("delay", 1).into(),
        ("bf", 0).into(),
        ("b_ref_mode", 0).into(),
        ("b", config.bitrate as i64).into(),
        ("flags", "+global_header").into(),
    ];

    if config.intra_refresh {
        options.push(("intra-refresh", 1).into());
        options.push(("g", 120).into());
    } else {
        options.push(("g", 600).into());
    }

    match config.codec {
        VideoCodec::H264 => {
            if config.yuv444 {
                options.push(("profile", "high444p").into());
                options.push(("rgb_mode", "yuv444").into());
            } else {
                options.push(("profile", "high").into());
            }
        }
        VideoCodec::HEVC => {
            if config.yuv444 {
                options.push(("rgb_mode", "yuv444").into());
            }
        }
        VideoCodec::AV1 => {
            if config.yuv444 {
                options.push(("rgb_mode", "yuv444").into());
            }
        }
    }

    if let Some(encoder_config) = &config.encoder_config {
        options.extend(encoder_config_to_dict(encoder_config));
    }

    print_options("nvenc", &options);
    let options = Dictionary::from_entries(&options)?;

    // Encoder
    let init_opts = Dictionary::from_entries(&[("fifo_size", 1).into()])?;

    let enc_name = match config.codec {
        VideoCodec::H264 => "h264_nvenc",
        VideoCodec::HEVC => "hevc_nvenc",
        VideoCodec::AV1 => "av1_nvenc",
    };

    let config = txproto_rs::EncoderConfig {
        enc_name,
        options: Some(options),
        init_opts: Some(init_opts),
        ..Default::default()
    };

    info!("Create encoder {enc_name}");
    let encoder = ctx.create_encoder(config)?;
    ctx.link(video_source, &encoder, 0)?;

    Ok(encoder)
}

fn create_vaapi(
    ctx: &Txproto,
    config: &VideoConfig,
    video_source: &txproto_rs::Handle,
) -> Result<txproto_rs::Handle> {
    // Configure Codec
    let mut options = vec![
        ("bf", 0).into(),
        ("b", config.bitrate as i64).into(),
        ("async_depth", 1).into(),
        ("flags", "+global_header").into(),
    ];

    match config.codec {
        VideoCodec::H264 => {
            options.push(("profile", "high").into());
        }
        VideoCodec::HEVC | VideoCodec::AV1 => {}
    };

    if let Some(encoder_config) = &config.encoder_config {
        options.extend(encoder_config_to_dict(encoder_config));
    }

    print_options("vaapi", &options);
    let options = Dictionary::from_entries(&options)?;

    // Encoder
    let enc_name = match config.codec {
        VideoCodec::H264 => "h264_vaapi",
        VideoCodec::HEVC => "hevc_vaapi",
        VideoCodec::AV1 => "av1_vaapi",
    };

    #[cfg(target_os = "linux")]
    let nv12_filter_graph: Option<&str> = match config.grab_backend {
        config::GrabBackend::Xcb => Some("hwupload,scale_vaapi=format=nv12"),
        config::GrabBackend::Drm => Some("scale_vaapi=format=nv12"),
        config::GrabBackend::Wlroots => Some("hwmap,scale_vaapi=format=nv12"),
        config::GrabBackend::NvFBC | config::GrabBackend::Gstreamer => None,
    };
    #[cfg(not(target_os = "linux"))]
    let nv12_filter_graph: Option<&str> = None;

    let init_opts = Dictionary::from_entries(&[("fifo_size", 2).into()])?;

    let config = txproto_rs::EncoderConfig {
        enc_name,
        options: Some(options),
        init_opts: Some(init_opts),
        pix_fmt: txproto_rs::PixelFormat::NV12,
        ..Default::default()
    };

    info!("Creating encoder {enc_name}");
    let encoder = ctx.create_encoder(config)?;

    if let Some(graph_str) = nv12_filter_graph {
        let filter = create_vaapi_nv12_filter(ctx, graph_str)?;
        ctx.link(video_source, &filter, 0)?;
        ctx.link(&filter, &encoder, 0)?;
    } else {
        ctx.link(video_source, &encoder, 0)?;
    }

    Ok(encoder)
}

fn create_qsv_nv12_filter(ctx: &Txproto) -> Result<txproto_rs::Handle> {
    let options = Dictionary::from_entries(&[("dump_graph", false).into()])?;

    // Derive the QSV device from the capture frames' device: QSV can only
    // process textures owned by the D3D11 device its session runs on, and
    // txproto may attach an unrelated device to the filters.
    let filter = ctx.create_filtergraph(
        "hwmap=derive_device=qsv,format=qsv,scale_qsv=format=nv12",
        txproto_rs::HWDeviceType::QSV,
        Some(options),
    )?;

    let init_opts = Dictionary::from_entries(&[("fifo_size", 1).into()])?;

    ctx.ctrl_opts(&filter, init_opts)?;

    Ok(filter)
}

fn create_vaapi_nv12_filter(ctx: &Txproto, graph_str: &str) -> Result<txproto_rs::Handle> {
    let options = Dictionary::from_entries(&[("dump_graph", false).into()])?;

    let filter =
        ctx.create_filtergraph(graph_str, txproto_rs::HWDeviceType::VAAPI, Some(options))?;

    Ok(filter)
}

fn create_qsv(
    ctx: &Txproto,
    config: &VideoConfig,
    video_source: &txproto_rs::Handle,
) -> Result<txproto_rs::Handle> {
    // Configure Codec options
    let mut options = vec![
        ("preset", "veryfast").into(),
        ("bf", 0).into(),
        ("g", 600).into(),
        ("b", config.bitrate as i64).into(),
        ("async_depth", 1).into(),
        ("flags", "+global_header").into(),
    ];

    match config.codec {
        VideoCodec::H264 => {
            options.push(("profile", "high").into());
        }
        VideoCodec::HEVC => {
            if config.yuv444 {
                options.push(("profile", "rext").into());
            }
        }
        VideoCodec::AV1 => {}
    }

    if let Some(encoder_config) = &config.encoder_config {
        options.extend(encoder_config_to_dict(encoder_config));
    }

    print_options("qsv", &options);
    let options = Dictionary::from_entries(&options)?;

    // Encoder
    let (enc_name, filter) = match config.codec {
        VideoCodec::H264 => {
            let filter = create_qsv_nv12_filter(ctx)?;

            ("h264_qsv", Some(filter))
        }
        VideoCodec::HEVC => {
            let filter = if config.yuv444 {
                None
            } else {
                Some(create_qsv_nv12_filter(ctx)?)
            };

            ("hevc_qsv", filter)
        }
        VideoCodec::AV1 => {
            let filter = create_qsv_nv12_filter(ctx)?;

            ("av1_qsv", Some(filter))
        }
    };

    let init_opts = Dictionary::from_entries(&[("fifo_size", 1).into()])?;

    let config = txproto_rs::EncoderConfig {
        enc_name,
        options: Some(options),
        init_opts: Some(init_opts),
        ..Default::default()
    };

    info!("Creating encoder {enc_name}");
    let encoder = ctx.create_encoder(config)?;

    if let Some(filter) = filter {
        ctx.link(video_source, &filter, 0)?;
        ctx.link(&filter, &encoder, 0)?;
    } else {
        ctx.link(video_source, &encoder, 0)?;
    }

    Ok(encoder)
}

fn create_amf(
    ctx: &Txproto,
    config: &VideoConfig,
    video_source: &txproto_rs::Handle,
) -> Result<txproto_rs::Handle> {
    // Configure Codec
    let mut options = vec![
        ("bf", 0).into(),
        ("b", config.bitrate as i64).into(),
        ("flags", "+global_header+low_delay").into(),
        ("async_depth", 1).into(),
    ];

    match config.codec {
        VideoCodec::H264 => {
            options.push(("usage", "ultralowlatency").into());
            options.push(("profile", "high").into());
        }
        VideoCodec::HEVC => {
            options.push(("usage", "ultralowlatency").into());
        }
        VideoCodec::AV1 => {
            options.push(("usage", "lowlatency").into());
        }
    };

    if let Some(encoder_config) = &config.encoder_config {
        options.extend(encoder_config_to_dict(encoder_config));
    }

    print_options("amf", &options);
    let options = Dictionary::from_entries(&options)?;

    // Encoder
    let enc_name = match config.codec {
        VideoCodec::H264 => "h264_amf",
        VideoCodec::HEVC => "hevc_amf",
        VideoCodec::AV1 => "av1_amf",
    };

    let init_opts = Dictionary::from_entries(&[("fifo_size", 1).into()])?;

    let config = txproto_rs::EncoderConfig {
        enc_name,
        options: Some(options),
        init_opts: Some(init_opts),
        ..Default::default()
    };

    info!("Creating encoder {enc_name}");
    let encoder = ctx.create_encoder(config)?;

    ctx.link(video_source, &encoder, 0)?;

    Ok(encoder)
}

fn create_x264(
    ctx: &Txproto,
    config: &VideoConfig,
    video_source: &txproto_rs::Handle,
) -> Result<txproto_rs::Handle> {
    let options = Dictionary::from_entries(&[("dump_graph", false).into()])?;

    let pix_fmt = if config.yuv444 { "yuv444p" } else { "nv12" };

    #[cfg(target_os = "windows")]
    let filter_desc = format!("hwdownload,format=bgra,format={pix_fmt}");
    #[cfg(not(target_os = "windows"))]
    let filter_desc = format!("format={pix_fmt}");

    let filter =
        ctx.create_filtergraph(&filter_desc, txproto_rs::HWDeviceType::NONE, Some(options))?;

    // Configure Codec
    let mut options = vec![
        ("preset", "ultrafast").into(),
        ("tune", "zerolatency").into(),
        ("b", config.bitrate as i64).into(),
        ("flags", "+global_header").into(),
    ];
    if config.yuv444 {
        options.push(("profile", "high444").into());
    }

    if let Some(encoder_config) = &config.encoder_config {
        options.extend(encoder_config_to_dict(encoder_config));
    }

    print_options("libx264", &options);

    let options = Dictionary::from_entries(&options)?;

    // Encoder
    let init_opts = Dictionary::from_entries(&[("fifo_size", 1).into()])?;

    let enc_name = "libx264";

    let config = txproto_rs::EncoderConfig {
        enc_name,
        options: Some(options),
        init_opts: Some(init_opts),
        ..Default::default()
    };

    info!("Create encoder {enc_name}");
    let encoder = ctx.create_encoder(config)?;

    ctx.link(video_source, &filter, 0)?;
    ctx.link(&filter, &encoder, 0)?;

    Ok(encoder)
}

fn create_videotoolbox(
    ctx: &Txproto,
    config: &VideoConfig,
    video_source: &txproto_rs::Handle,
) -> Result<txproto_rs::Handle> {
    // Configure Codec
    let mut options = vec![
        ("preset", "medium").into(),
        ("hardware", true).into(),
        ("pix_fmt", "nv12").into(),
        ("flags", "+global_header").into(),
        ("prio_speed", true).into(),
        ("realtime", true).into(),
        ("power_efficient", 0).into(),
        ("low_delay", true).into(),
        ("bf", 0).into(),
        ("b_ref_mode", 0).into(),
        ("b", config.bitrate as i64).into(),
    ];

    if let Some(encoder_config) = &config.encoder_config {
        options.extend(encoder_config_to_dict(encoder_config));
    }

    print_options("videotoolbox", &options);
    let options = Dictionary::from_entries(&options)?;

    // Encoder
    let init_opts = Dictionary::from_entries(&[("fifo_size", 1).into()])?;

    let enc_name = match config.codec {
        VideoCodec::H264 => "h264_videotoolbox",
        VideoCodec::HEVC => "hevc_videotoolbox",
        VideoCodec::AV1 => "av1_videotoolbox",
    };

    let config = txproto_rs::EncoderConfig {
        enc_name,
        options: Some(options),
        init_opts: Some(init_opts),
        ..Default::default()
    };

    info!("Create encoder {enc_name}");
    let encoder = ctx.create_encoder(config)?;
    ctx.link(video_source, &encoder, 0)?;

    Ok(encoder)
}

fn plug_error_listener(
    ctx: &Txproto,
    handle: &txproto_rs::Handle,
    stage: PipelineStage,
    sender: &EventSender<IpcApi>,
) -> Result<txproto_rs::Event> {
    let sender = sender.clone();
    let error_cb = move |event| {
        info!("Video acquisition event: {event:?}");

        sender.send(Event::PipelineEvent(PipelineEvent {
            type_: PipelineType::Video,
            stage,
            event: PipelineEventType::Error,
        }));
    };

    let mut error_event = txproto_rs::Event::new(txproto_rs::EventType::Error, error_cb);
    ctx.register_event(handle, &mut error_event)?;

    Ok(error_event)
}

pub(crate) struct VideoPipeline {
    pub(crate) encoder: Option<txproto_rs::Handle>,
    pub(crate) events: Vec<txproto_rs::Event>,
}

pub(crate) fn configure(
    ctx: &Txproto,
    video_config: VideoConfig,
    muxer: txproto_rs::Handle,
    sender: Option<&EventSender<IpcApi>>,
) -> Result<VideoPipeline> {
    let mut events = vec![];

    // Restrict initialization to only that backend + audio, so other video
    // backends are not loaded.
    #[cfg(target_os = "linux")]
    let api_list: Vec<&str> = grab_backend_api_list(video_config.grab_backend);
    #[cfg(not(target_os = "linux"))]
    let api_list: Vec<&str> = vec![];

    info!("register_io_cb with api_list: {:?}", api_list);
    let io_event = ctx.register_io_cb(|_| {}, &api_list)?;
    drop(io_event);

    // Video source
    info!("Start screen grab on display {}", video_config.display_id);
    #[cfg(target_os = "linux")]
    let grab_options = Some(grab_backend_to_dict(video_config.grab_backend)?);

    #[cfg(not(target_os = "linux"))]
    let grab_options = None;

    let video_source = ctx.create_io(video_config.display_id, grab_options)?;

    // Watermark
    #[cfg(feature = "watermark")]
    let video_source = if let Some(watermark_config) = &video_config.watermark {
        let watermark = ctx.watermark_filter_create(
            &watermark_config.file,
            watermark_position(watermark_config),
        )?;
        ctx.link(&video_source, &watermark, 0)?;

        #[cfg(target_os = "linux")]
        {
            let filter =
                ctx.create_filtergraph("hwupload", txproto_rs::HWDeviceType::CUDA, None)?;

            ctx.link(&watermark, &filter, 0)?;

            filter
        }

        #[cfg(target_os = "windows")]
        watermark
    } else {
        video_source
    };

    // Encoder
    let create_encoder = match video_config.encoder {
        VideoEncoder::AMF => create_amf,
        VideoEncoder::NVENC => create_nvenc,
        VideoEncoder::QSV => create_qsv,
        VideoEncoder::VAAPI => create_vaapi,
        VideoEncoder::V4L2M2M => {
            return Err(crate::txproto::Error::ConfigInvalid {
                reason: "v4l2m2m encoder requires the libavconv backend".to_string(),
            });
        }
        VideoEncoder::X264 => create_x264,
        VideoEncoder::VIDEOTOOLBOX => create_videotoolbox,
    };

    let encoder = create_encoder(ctx, &video_config, &video_source)?;

    // Link graph
    ctx.link(&encoder, &muxer, 0)?;

    // Plug error listeners
    if let Some(sender) = sender {
        // Acquisition
        let event = plug_error_listener(ctx, &video_source, PipelineStage::Acquisition, sender)?;
        events.push(event);

        // Encoder
        let event = plug_error_listener(ctx, &encoder, PipelineStage::Encoder, sender)?;
        events.push(event);
    }

    Ok(VideoPipeline {
        encoder: Some(encoder),
        events,
    })
}
