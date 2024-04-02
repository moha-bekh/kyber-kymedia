use log::{info, warn};
use serde::Deserialize;

use avservice_types::{PipelineEvent, PipelineEventType, PipelineStage, PipelineType, VideoCodec};
use libkypc::EventSender;
use txproto_rs::{Dictionary, DictionaryEntry, DictionaryValue, Txproto};

use crate::config;
use crate::{Event, Result};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::upper_case_acronyms)]
pub(crate) enum VideoEncoder {
    AMF,
    NVENC,
    QSV,
    VAAPI,
    X264,
}

pub(crate) struct VideoConfig {
    pub(crate) display_id: u32,
    pub(crate) encoder: VideoEncoder,
    pub(crate) bitrate: u32,
    pub(crate) codec: VideoCodec,
    pub(crate) yuv444: bool,
    pub(crate) intra_refresh: bool,
    pub(crate) encoder_config: Option<config::EncoderConfig>,
}

fn print_options(encoder: &str, options: &[DictionaryEntry]) {
    match config::dump_encoder_config(config::EncoderType::Video, options) {
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

fn create_nvenc(
    ctx: &Txproto,
    config: &VideoConfig,
    video_source: &txproto_rs::Handle,
) -> Result<txproto_rs::Handle> {
    // Configure Codec
    let options = if let Some(encoder_config) = &config.encoder_config {
        encoder_config_to_dict(encoder_config)
    } else {
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
        }

        match config.codec {
            VideoCodec::H264 => {
                if config.yuv444 {
                    options.push(("profile", "high444p").into());
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

        options
    };

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
    let options = if let Some(encoder_config) = &config.encoder_config {
        encoder_config_to_dict(encoder_config)
    } else {
        let mut options = vec![
            ("bf", 0).into(),
            ("b", config.bitrate as i64).into(),
            ("flags", "+global_header").into(),
        ];

        match config.codec {
            VideoCodec::H264 => {
                options.push(("profile", "high").into());
            }
            VideoCodec::HEVC | VideoCodec::AV1 => {}
        };

        options
    };

    print_options("vaapi", &options);
    let options = Dictionary::from_entries(&options)?;

    // Encoder
    let enc_name = match config.codec {
        VideoCodec::H264 => "h264_vaapi",
        VideoCodec::HEVC => "hevc_vaapi",
        VideoCodec::AV1 => "av1_vaapi",
    };

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
    ctx.link(video_source, &encoder, 0)?;

    Ok(encoder)
}

fn create_qsv_nv12_filter(ctx: &Txproto) -> Result<txproto_rs::Handle> {
    let options = Dictionary::from_entries(&[("dump_graph", false).into()])?;

    let filter = ctx.create_filtergraph(
        "hwmap,format=qsv,scale_qsv=format=nv12",
        txproto_rs::HWDeviceType::QSV,
        Some(options),
    )?;

    Ok(filter)
}

fn create_qsv(
    ctx: &Txproto,
    config: &VideoConfig,
    video_source: &txproto_rs::Handle,
) -> Result<txproto_rs::Handle> {
    // Configure Codec
    let mut options = vec![
        ("preset", "veryfast").into(),
        ("bf", 0).into(),
        ("g", 600).into(),
        ("b", config.bitrate as i64).into(),
        ("async_depth", 1).into(),
        ("flags", "+global_header").into(),
    ];

    let (enc_name, filter) = match config.codec {
        VideoCodec::H264 => {
            options.push(("profile", "high").into());

            let filter = create_qsv_nv12_filter(ctx)?;

            ("h264_qsv", Some(filter))
        }
        VideoCodec::HEVC => {
            let filter = if config.yuv444 {
                options.push(("profile", "rext").into());

                None
            } else {
                let filter = create_qsv_nv12_filter(ctx)?;

                Some(filter)
            };

            ("hevc_qsv", filter)
        }
        VideoCodec::AV1 => {
            let filter = create_qsv_nv12_filter(ctx)?;

            ("av1_qsv", Some(filter))
        }
    };

    print_options("qsv", &options);
    let options = Dictionary::from_entries(&options)?;

    // Encoder
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
        ("preset", "veryfast").into(),
        ("bf", 0).into(),
        ("b", config.bitrate as i64).into(),
        ("flags", "+global_header").into(),
    ];

    let enc_name = match config.codec {
        VideoCodec::H264 => {
            options.push(("usage", "ultralowlatency").into());
            options.push(("profile", "high").into());

            "h264_amf"
        }
        VideoCodec::HEVC => {
            options.push(("usage", "ultralowlatency").into());

            "hevc_amf"
        }
        VideoCodec::AV1 => {
            options.push(("usage", "lowlatency").into());

            "av1_amf"
        }
    };

    print_options("amf", &options);
    let options = Dictionary::from_entries(&options)?;

    // Encoder
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

    let filter = ctx.create_filtergraph(
        "scale=w=1920:h=-1,format=nv12",
        txproto_rs::HWDeviceType::NONE,
        Some(options),
    )?;

    // Configure Codec
    let options = &[
        ("preset", "ultrafast").into(),
        ("tune", "zerolatency").into(),
        ("b", config.bitrate as i64).into(),
        ("flags", "+global_header").into(),
    ];

    print_options("libx264", options);

    let options = Dictionary::from_entries(options)?;

    // Encoder
    let enc_name = "libx264";

    let config = txproto_rs::EncoderConfig {
        enc_name,
        options: Some(options),
        ..Default::default()
    };

    info!("Create encoder {enc_name}");
    let encoder = ctx.create_encoder(config)?;

    ctx.link(video_source, &filter, 0)?;
    ctx.link(&filter, &encoder, 0)?;

    Ok(encoder)
}

fn plug_error_listener(
    ctx: &Txproto,
    handle: &txproto_rs::Handle,
    stage: PipelineStage,
    mut sender: EventSender<Event>,
) -> Result<txproto_rs::Event> {
    let error_cb = move |event| {
        info!("Video acquisition event: {event:?}");

        let ret = sender.blocking_send(Event::PipelineEvent(PipelineEvent {
            type_: PipelineType::Video,
            stage,
            event: PipelineEventType::Error,
        }));
        if let Err(err) = ret {
            warn!("Failed to send video acquisition error: {err:?}");
        }
    };

    let mut error_event = txproto_rs::Event::new(txproto_rs::EventType::Error, error_cb);
    ctx.register_event(handle, &mut error_event)?;

    Ok(error_event)
}

pub(crate) fn configure(
    ctx: &Txproto,
    video_config: VideoConfig,
    muxer: txproto_rs::Handle,
    sender: Option<EventSender<Event>>,
) -> Result<Vec<txproto_rs::Event>> {
    let mut events = vec![];

    let io_event = ctx.register_io_cb(|_| {})?;
    drop(io_event);

    // Video source
    info!("Start screen grab on display {}", video_config.display_id);
    let video_source = ctx.create_io(video_config.display_id, None)?;

    // Encoder
    let create_encoder = match video_config.encoder {
        VideoEncoder::AMF => create_amf,
        VideoEncoder::NVENC => create_nvenc,
        VideoEncoder::QSV => create_qsv,
        VideoEncoder::VAAPI => create_vaapi,
        VideoEncoder::X264 => create_x264,
    };

    let encoder = create_encoder(ctx, &video_config, &video_source)?;

    // Link graph
    ctx.link(&encoder, &muxer, 0)?;

    // Plug error listeners
    if let Some(sender) = sender {
        // Acquisition
        let event = plug_error_listener(
            ctx,
            &video_source,
            PipelineStage::Acquisition,
            sender.clone(),
        )?;
        events.push(event);

        // Encoder
        let event = plug_error_listener(ctx, &encoder, PipelineStage::Encoder, sender.clone())?;
        events.push(event);
    }

    Ok(events)
}
