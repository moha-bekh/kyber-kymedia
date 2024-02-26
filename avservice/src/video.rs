use log::info;
use serde::Deserialize;

use avservice_types::VideoCodec;
use txproto_rs::{Dictionary, DictionaryEntry, Txproto};

use crate::Result;

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
}

fn print_options(header: &str, options: &[DictionaryEntry]) {
    info!("{header}");

    for o in options {
        info!("\t{}: {:?}", o.key, o.value);
    }
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
    }

    let enc_name = match config.codec {
        VideoCodec::H264 => {
            if config.yuv444 {
                options.push(("profile", "high444p").into());
            } else {
                options.push(("profile", "high").into());
            }

            "h264_nvenc"
        }
        VideoCodec::HEVC => "hevc_nvenc",
        VideoCodec::AV1 => "av1_nvenc",
    };

    print_options("NVENC options", &options);
    let options = Dictionary::from_entries(&options)?;

    // Encoder
    let init_opts = Dictionary::from_entries(&[("fifo_size", 1).into()])?;

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
        ("flags", "+global_header").into(),
    ];

    let enc_name = match config.codec {
        VideoCodec::H264 => {
            options.push(("profile", "high").into());

            "h264_vaapi"
        }
        VideoCodec::HEVC => "hevc_vaapi",
        VideoCodec::AV1 => "av1_vaapi",
    };

    print_options("VAAPI options", &options);
    let options = Dictionary::from_entries(&options)?;

    // Encoder
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

fn create_qsv(
    ctx: &Txproto,
    config: &VideoConfig,
    video_source: &txproto_rs::Handle,
) -> Result<txproto_rs::Handle> {
    let options = Dictionary::from_entries(&[("dump_graph", false).into()])?;

    let filter = ctx.create_filtergraph(
        "hwmap,format=qsv,scale_qsv=format=nv12",
        txproto_rs::HWDeviceType::QSV,
        Some(options),
    )?;

    // Configure Codec
    let mut options = vec![
        ("preset", "veryfast").into(),
        ("bf", 0).into(),
        ("g", 600).into(),
        ("b", config.bitrate as i64).into(),
        ("async_depth", 1).into(),
        ("flags", "+global_header").into(),
    ];

    let enc_name = match config.codec {
        VideoCodec::H264 => {
            options.push(("profile", "high").into());

            "h264_qsv"
        }
        VideoCodec::HEVC => "hevc_qsv",
        VideoCodec::AV1 => "av1_qsv",
    };

    print_options("QSV options", &options);
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

    ctx.link(video_source, &filter, 0)?;
    ctx.link(&filter, &encoder, 0)?;

    Ok(encoder)
}

fn create_amf(
    ctx: &Txproto,
    config: &VideoConfig,
    video_source: &txproto_rs::Handle,
) -> Result<txproto_rs::Handle> {
    let options = Dictionary::from_entries(&[("dump_graph", false).into()])?;

    let filter = ctx.create_filtergraph(
        "hwdownload,format=bgra,hwupload",
        txproto_rs::HWDeviceType::D3D11VA,
        Some(options),
    )?;

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

    print_options("AMF options", &options);
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

    ctx.link(video_source, &filter, 0)?;
    ctx.link(&filter, &encoder, 0)?;

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

    print_options("libx264 options", options);

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

pub(crate) fn configure(
    ctx: &Txproto,
    video_config: VideoConfig,
    muxer: txproto_rs::Handle,
) -> Result<()> {
    let event = ctx.register_io_cb(|_| {})?;
    drop(event);

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

    Ok(())
}
