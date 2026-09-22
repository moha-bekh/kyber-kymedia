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

use std::sync::Arc;

use log::{info, warn};

use kyavservice_types::VideoCodec;
use libavconv_rs::{
    AvconvContext, DataSink, EncoderBuilder, FilterGraph, FilterGraphBuilder, GrabCapture,
    GrabCaptureBuilder, GrabProviderCtx, GrabSource, Handle, HwDeviceRef, StreamType, sys,
};

use super::Result;
use super::clock;
#[cfg(target_os = "windows")]
use super::cursor;
use super::encoder_control::EncoderControl;
use super::encoder_hook;
use super::metrics::{MetricsForwarder, capture_metrics_hook};
#[cfg(feature = "watermark")]
use super::watermark;
use crate::config;
use crate::config::VideoEncoder;
#[cfg(feature = "watermark")]
use crate::config::WatermarkConfig;

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

impl VideoConfig {
    pub(crate) fn create_hw_device(&self) -> Result<Option<HwDeviceRef>> {
        #[cfg(target_os = "windows")]
        {
            let _ = self;
            Ok(Some(HwDeviceRef::new(
                sys::AVHWDeviceType_AV_HWDEVICE_TYPE_D3D11VA,
            )?))
        }
        #[cfg(target_os = "linux")]
        {
            #[cfg(feature = "watermark")]
            let needs_watermark_device = self.watermark.is_some();
            #[cfg(not(feature = "watermark"))]
            let needs_watermark_device = false;

            let needs_vaapi_upload = matches!(self.grab_backend, config::GrabBackend::Xcb)
                && matches!(self.encoder, VideoEncoder::VAAPI | VideoEncoder::QSV);

            if needs_vaapi_upload || needs_watermark_device {
                Ok(Some(HwDeviceRef::new(
                    sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VAAPI,
                )?))
            } else {
                Ok(None)
            }
        }
        #[cfg(target_os = "macos")]
        {
            let _ = self;
            Ok(Some(HwDeviceRef::new(
                sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
            )?))
        }
    }
}

/// Selects the grab provider name for the current platform.
#[cfg(target_os = "windows")]
pub(crate) fn grab_provider_name(_backend: Option<config::GrabBackend>) -> &'static str {
    "dxgi"
}

#[cfg(target_os = "linux")]
pub(crate) fn grab_provider_name(backend: Option<config::GrabBackend>) -> &'static str {
    match backend.unwrap_or_default() {
        // NvFBC is a capture mode of the x11 provider (auto-selected when
        // the NVIDIA driver exposes it), not a standalone provider. Selecting
        // Xcb resolves to the same x11 provider; libavconv-rs doesn't expose
        // x11_capture_modes yet so NvFBC mode can't be force-disabled here.
        config::GrabBackend::NvFBC | config::GrabBackend::Xcb => "x11",
        config::GrabBackend::Drm => "kms",
        config::GrabBackend::Wlroots => "wayland",
    }
}

// libavgrab has no macOS screen-capture provider yet (only dxgi, x11, kms,
// wayland, nvfbc are implemented). Returning a name that isn't registered
// makes provider_by_name() fail with a clean NotFound error at runtime
// instead of failing to compile the avconv backend on macOS at all.
#[cfg(target_os = "macos")]
pub(crate) fn grab_provider_name(_backend: Option<config::GrabBackend>) -> &'static str {
    "avfoundation"
}

/// Set up the capture source.  Returns `(capture, output_handle)`.
fn create_grab_source(
    ctx: Arc<AvconvContext>,
    hw_device: Option<&HwDeviceRef>,
    video_config: &VideoConfig,
    metrics: Option<&MetricsForwarder>,
) -> Result<(GrabCapture, Handle)> {
    #[cfg(target_os = "linux")]
    let backend = Some(video_config.grab_backend);
    #[cfg(not(target_os = "linux"))]
    let backend: Option<config::GrabBackend> = None;

    let provider = libavconv_rs::provider_by_name(grab_provider_name(backend)).ok_or(
        libavconv_rs::Error::NotFound {
            name: "grab provider",
        },
    )?;

    let mut grab_ctx = GrabProviderCtx::new(provider)?;

    // Provider takes its own reference to hw_device.
    if let Some(hw_device) = hw_device {
        grab_ctx.set_hw_device(hw_device);
    }

    grab_ctx.init()?;

    // Request raw-frame screen sources.
    // bindgen emits C enum variants as TypeName_VARIANT constants.
    let payloads: u32 = 1 << sys::AVConvHandlePayload_AVCONV_HANDLE_PAYLOAD_FRAME;
    let src_types: u64 = 1u64 << (sys::AVGrabSourceType_AVGRAB_SOURCE_TYPE_SCREEN as u64);

    let sources = grab_ctx.sources(payloads, src_types)?;

    // Select by display index, fall back to first available.
    let src = sources
        .iter()
        .nth(video_config.display_id as usize)
        .or_else(|| sources.iter().next())
        .ok_or(libavconv_rs::Error::NoSource)?;

    // Always select the source's default (desktop) stream.
    let default_idx =
        src.find_stream_by_type(StreamType::Default)
            .ok_or(libavconv_rs::Error::NotFound {
                name: "default source stream",
            })?;
    let cursor_idx = cursor_stream_idx(src);

    let stream_indices: Vec<u32> = match cursor_idx {
        Some(i) => vec![default_idx, i],
        None => vec![default_idx],
    };
    let cursor_capture_idx: Option<u32> = cursor_idx.and_then(|i| {
        stream_indices
            .iter()
            .position(|&s| s == i)
            .map(|pos| pos as u32)
    });

    let mut capture = GrabCaptureBuilder::with_streams(src, &stream_indices)?;
    clock::configure_capture_clock(&mut capture, &video_config.clock_type);

    let video_hook = video_hook_closure(metrics.cloned());
    install_capture_hook(
        &mut capture,
        video_config.display_id,
        cursor_capture_idx,
        video_hook,
    );

    let capture = capture.build(ctx.clone())?;

    // Stream 0 is the default (video) stream by construction above.
    let handle = capture.stream_handle(0);
    Ok((capture, handle))
}

type VideoHook = Box<dyn FnMut(&mut u32, &mut sys::AVFrame, u32, &mut u32) -> i32 + Send + 'static>;

/// Closure invoked on each frame of the default (video) stream. Mirrors the
/// existing `capture_metrics_hook` behavior (returns `AVERROR(ENOSYS)` so
/// libavgrab takes the hookless path) but falls back to a no-op closure when
/// metrics aren't enabled.
fn video_hook_closure(metrics: Option<MetricsForwarder>) -> VideoHook {
    match metrics {
        Some(m) => {
            let mut metrics_hook = capture_metrics_hook(m);
            Box::new(
                move |idx: &mut u32,
                      frame: &mut sys::AVFrame,
                      flags_in: u32,
                      flags_out: &mut u32| {
                    metrics_hook(idx, frame, flags_in, flags_out)
                },
            )
        }
        None => Box::new(
            |_idx: &mut u32, _frame: &mut sys::AVFrame, _flags_in: u32, _flags_out: &mut u32| {
                -libc::ENOSYS
            },
        ),
    }
}

/// Returns the capture source's cursor stream index. Only Windows (DXGI)
/// exposes a separate cursor stream; every other platform has none, so the
/// lookup and the `StreamType::Cursor` reference are compiled out.
#[cfg(target_os = "windows")]
fn cursor_stream_idx(src: GrabSource<'_>) -> Option<u32> {
    src.find_stream_by_type(StreamType::Cursor)
}

#[cfg(not(target_os = "windows"))]
fn cursor_stream_idx(_src: GrabSource<'_>) -> Option<u32> {
    None
}

/// Installs the per-frame capture hook. On Windows, when a cursor stream is
/// present, the video hook is wrapped with the DXGI cursor forwarder that pushes
/// cursor packets to `\\.\pipe\KyberInputServer`; otherwise the plain video hook
/// is installed. On every other platform there is no cursor stream, so only the
/// video hook is installed and the `cursor` module is never compiled.
#[cfg(target_os = "windows")]
fn install_capture_hook(
    capture: &mut GrabCaptureBuilder,
    display_id: u32,
    cursor_capture_idx: Option<u32>,
    video_hook: VideoHook,
) {
    match cursor_capture_idx {
        Some(idx) => {
            let sink = cursor::CursorSink::new(display_id);
            capture.on_frame(cursor::cursor_hook(sink, display_id, idx, video_hook));
        }
        None => {
            capture.on_frame(video_hook);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn install_capture_hook(
    capture: &mut GrabCaptureBuilder,
    _display_id: u32,
    _cursor_capture_idx: Option<u32>,
    video_hook: VideoHook,
) {
    capture.on_frame(video_hook);
}

/// Returns `(encoder_name, filtergraph_description)`.
fn encoder_names(config: &VideoConfig) -> (&'static str, String) {
    match config.encoder {
        VideoEncoder::NVENC => {
            let enc = match config.codec {
                VideoCodec::H264 => "h264_nvenc",
                VideoCodec::HEVC => "hevc_nvenc",
                VideoCodec::AV1 => "av1_nvenc",
            };
            (enc, "null".to_string())
        }
        VideoEncoder::AMF => {
            let enc = match config.codec {
                VideoCodec::H264 => "h264_amf",
                VideoCodec::HEVC => "hevc_amf",
                VideoCodec::AV1 => "av1_amf",
            };
            (enc, "null".to_string())
        }
        VideoEncoder::VAAPI => {
            let enc = match config.codec {
                VideoCodec::H264 => "h264_vaapi",
                VideoCodec::HEVC => "hevc_vaapi",
                VideoCodec::AV1 => "av1_vaapi",
            };
            (enc, "null".to_string())
        }
        VideoEncoder::QSV => {
            let enc = match config.codec {
                VideoCodec::H264 => "h264_qsv",
                VideoCodec::HEVC => "hevc_qsv",
                VideoCodec::AV1 => "av1_qsv",
            };
            (enc, "hwmap,format=qsv,scale_qsv=format=nv12".to_string())
        }
        VideoEncoder::X264 => {
            let pix_fmt = if config.yuv444 { "yuv444p" } else { "nv12" };
            #[cfg(target_os = "windows")]
            let filter = format!("hwdownload,format=bgra,format={pix_fmt}");
            #[cfg(not(target_os = "windows"))]
            let filter = format!("format={pix_fmt}");
            ("libx264", filter)
        }
        // Not implemented by the libavconv backend yet (txproto is the only
        // engine with a macOS/VideoToolbox path so far). Returning a name
        // that isn't registered makes find_encoder_by_name() fail with a
        // clean NotFound error at runtime instead of failing to compile.
        VideoEncoder::VIDEOTOOLBOX => {
            let enc = match config.codec {
                VideoCodec::H264 => "h264_videotoolbox",
                VideoCodec::HEVC => "hevc_videotoolbox",
                VideoCodec::AV1 => "av1_videotoolbox",
            };
            (enc, "null".to_string())
        }
    }
}

fn print_options(encoder: &str, options: &[(&str, String)]) {
    let entries: Vec<(&str, toml::Value)> = options
        .iter()
        .map(|(key, value)| (*key, toml::Value::String(value.clone())))
        .collect();

    match config::dump_encoder_config(config::EncoderType::Video, &entries) {
        Ok((section, dumped)) => {
            info!(
                r#"{encoder} encoder configuration dump:
[{section}]
{dumped}"#
            );
        }
        Err(err) => {
            warn!("Fail to dump encoder config: {err:?}");
        }
    }
}

/// Set bitrate, global-header flag and per-encoder options on the builder.
///
/// Must be called before `EncoderBuilder::build` (which initialises the
/// encoder and locks out further configuration).
fn configure_enc_ctx(enc: &EncoderBuilder, config: &VideoConfig) -> Result<()> {
    let (name, default_options) = match config.encoder {
        VideoEncoder::NVENC => ("nvenc", configure_nvenc(config)),
        VideoEncoder::AMF => ("amf", configure_amf(config)),
        VideoEncoder::VAAPI => ("vaapi", configure_vaapi(config)),
        VideoEncoder::QSV => ("qsv", configure_qsv(config)),
        VideoEncoder::X264 => ("libx264", configure_x264(config)),
        // Unreachable in practice: encoder_names() already fails the
        // pipeline before configure_enc_ctx() runs (see the comment there).
        VideoEncoder::VIDEOTOOLBOX => ("videotoolbox", Vec::new()),
    };

    let mut options: Vec<(&str, String)> = default_options;
    options.push(("b", config.bitrate.to_string()));

    if let Some(encoder_config) = &config.encoder_config {
        warn!("Using custom video encoder configuration");
        for (key, value) in encoder_config_to_options(encoder_config) {
            if let Some(index) = options
                .iter()
                .position(|(option_key, _)| *option_key == key)
            {
                options[index].1 = value;
            } else {
                options.push((key, value));
            }
        }
    }

    print_options(name, &options);
    enc.set_options_string(&encoder_options_to_opt_string(&options))?;
    enc.enable_global_header();

    Ok(())
}

fn configure_nvenc(config: &VideoConfig) -> Vec<(&'static str, String)> {
    let mut options = vec![
        ("preset", "p2".to_string()),
        ("tune", "ull".to_string()),
        ("zerolatency", "1".to_string()),
        ("delay", "1".to_string()),
        ("bf", "0".to_string()),
        ("b_ref_mode", "0".to_string()),
    ];

    if config.intra_refresh {
        options.push(("intra-refresh", "1".to_string()));
        options.push(("g", "120".to_string()));
    } else {
        options.push(("g", "600".to_string()));
    }

    match config.codec {
        VideoCodec::H264 => {
            let profile = if config.yuv444 { "high444p" } else { "high" };
            options.push(("profile", profile.to_string()));
        }
        VideoCodec::HEVC | VideoCodec::AV1 => {
            if config.yuv444 {
                options.push(("rgb_mode", "yuv444".to_string()));
            }
        }
    }

    options
}

fn configure_amf(config: &VideoConfig) -> Vec<(&'static str, String)> {
    let mut options = vec![("bf", "0".to_string()), ("max_b_frames", "0".to_string())];

    match config.codec {
        VideoCodec::H264 => {
            options.push(("usage", "ultralowlatency".to_string()));
            options.push(("profile", "high".to_string()));
        }
        VideoCodec::HEVC => {
            options.push(("usage", "ultralowlatency".to_string()));
        }
        VideoCodec::AV1 => {
            options.push(("usage", "lowlatency".to_string()));
        }
    }

    options
}

fn configure_vaapi(config: &VideoConfig) -> Vec<(&'static str, String)> {
    let mut options = vec![("bf", "0".to_string())];
    if let VideoCodec::H264 = config.codec {
        options.push(("profile", "high".to_string()));
    }
    options
}

fn configure_qsv(config: &VideoConfig) -> Vec<(&'static str, String)> {
    let mut options = vec![
        ("preset", "veryfast".to_string()),
        ("bf", "0".to_string()),
        ("g", "600".to_string()),
        ("async_depth", "1".to_string()),
    ];

    match config.codec {
        VideoCodec::H264 => options.push(("profile", "high".to_string())),
        VideoCodec::HEVC => {
            if config.yuv444 {
                options.push(("profile", "rext".to_string()));
            }
        }
        VideoCodec::AV1 => {}
    }

    options
}

fn configure_x264(config: &VideoConfig) -> Vec<(&'static str, String)> {
    let mut options = vec![
        ("preset", "ultrafast".to_string()),
        ("tune", "zerolatency".to_string()),
    ];
    if config.yuv444 {
        options.push(("profile", "high444".to_string()));
    }
    options
}

fn encoder_config_to_options(encoder_config: &config::EncoderConfig) -> Vec<(&str, String)> {
    encoder_config
        .iter()
        .filter_map(|(k, v)| {
            let val = match v {
                toml::value::Value::Boolean(b) => Some(if *b { "1" } else { "0" }.to_string()),
                toml::value::Value::Integer(i) => Some(i.to_string()),
                toml::value::Value::String(s) => Some(s.clone()),
                _ => {
                    warn!("Encoder config '{k}': unsupported value type {v:?}");
                    None
                }
            };
            val.map(|v| (k.as_str(), v))
        })
        .collect()
}

fn encoder_options_to_opt_string(options: &[(&str, String)]) -> String {
    options
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Build filtergraph + encoder, wire them to `capture_handle`.
///
/// When `watermark` is `Some`, a watermark closure is installed on the
/// filtergraph's `on_frame_in` slot so that every incoming frame is
/// composited GPU-side.
fn create_encoder_pipeline(
    ctx: Arc<AvconvContext>,
    hw_device: Option<&HwDeviceRef>,
    video_config: &VideoConfig,
    capture_handle: Handle,
    #[cfg(feature = "watermark")] watermark: Option<&WatermarkConfig>,
) -> Result<(FilterGraph, EncoderBuilder)> {
    let (enc_name, filter_desc) = encoder_names(video_config);
    info!("Create encoder {enc_name}, filter: {filter_desc}");

    let mut fg = FilterGraphBuilder::new(Arc::clone(&ctx), &filter_desc, hw_device)?;

    // Install watermark closure before build() libavconv copies the hook
    // pointers into its internal storage during init. VideoConfig::
    // create_hw_device() always hands back a device when a watermark is
    // configured, so this can only miss one if the two get out of sync.
    #[cfg(feature = "watermark")]
    if let Some(cfg) = watermark {
        let device = hw_device.ok_or(libavconv_rs::Error::NotFound {
            name: "hw device for watermark",
        })?;
        let hook = watermark::watermark_hook(device, cfg)?;
        fg.on_frame_in(hook);
    }

    let codec = libavconv_rs::Codec::find_encoder_by_name(enc_name).ok_or(
        libavconv_rs::Error::NotFound {
            name: "encoder codec",
        },
    )?;

    let enc = EncoderBuilder::new(Arc::clone(&ctx), codec)?;

    configure_enc_ctx(&enc, video_config)?;

    fg.set_out_params_from_enc(0, &enc)?;

    // ULL: cap the filter node's input queue to a single frame and let the
    // upstream connect drop older queued frames when hwdownload falls behind,
    // instead of back-pressuring into the DXGI hw frame pool.
    ctx.set_node_queue_size(fg.input_handle(0), 1)?;
    ctx.connect(
        capture_handle,
        fg.input_handle(0),
        libavconv_rs::CONNECT_SEND_NONBLOCK,
    )?;
    ctx.connect(fg.output_handle(0), enc.handle_in(), 0)?;

    let fg = fg.build()?;

    Ok((fg, enc))
}

fn create_cursor_discard(ctx: Arc<AvconvContext>, cursor_handle: Handle) -> Result<DataSink> {
    let sink = DataSink::null(
        Arc::clone(&ctx),
        sys::AVConvHandlePayload_AVCONV_HANDLE_PAYLOAD_FRAME,
        sys::AVMediaType_AVMEDIA_TYPE_VIDEO,
    )?;

    ctx.connect(cursor_handle, sink.handle(), 0)?;

    Ok(sink)
}

pub(crate) struct VideoPipeline {
    /// `_capture`, `_fg` and the `Encoder` (held internally) keep their
    /// hook closures alive until the `AvconvContext` is dropped (which
    /// stops all callback dispatch).
    pub(crate) _capture: GrabCapture,
    pub(crate) _fg: FilterGraph,
    pub(crate) _enc: libavconv_rs::Encoder,
    _cursor_discard: Option<DataSink>,
}

/// Build a complete video capture + encode pipeline and wire it to the muxer.
///
/// Caller must then call `ctx.start()` to begin processing.
///
/// # Arguments
/// * `ctx`              - libavconv context.
/// * `hw_device`        - hardware device, when the encoder needs one (see
///   `VideoConfig::create_hw_device`). Always set on Windows (D3D11VA) and
///   macOS (VideoToolbox).
/// * `video_config`     - encoder / capture settings.
/// * `output_hdl`      - scheduler input handle of the terminal node the
///   encoder feeds: an RTP muxer stream or a kymux data sink.
/// * `metrics`          - optional metrics forwarder; when set, capture and
///   encoder hooks emit `(pts, acquired)`, `(pts, encoding)`,
///   `(pts, encoded)` events.
pub(crate) fn configure(
    ctx: Arc<AvconvContext>,
    hw_device: Option<&HwDeviceRef>,
    video_config: VideoConfig,
    output_hdl: Handle,
    metrics: Option<MetricsForwarder>,
) -> Result<(VideoPipeline, Arc<EncoderControl>)> {
    info!(
        "Start screen grab on display {}, encoder {:?}",
        video_config.display_id, video_config.encoder
    );

    let (capture, capture_handle) =
        create_grab_source(Arc::clone(&ctx), hw_device, &video_config, metrics.as_ref())?;

    let (fg, mut enc_builder) = create_encoder_pipeline(
        Arc::clone(&ctx),
        hw_device,
        &video_config,
        capture_handle,
        #[cfg(feature = "watermark")]
        video_config.watermark.as_ref(),
    )?;

    // Always install the encoder closures (they handle both metrics and
    // runtime VideoSetBitrate / VideoForceIdr commands). Must be installed
    // before build() - the closures are moved into the encoder's hook
    // storage and live as long as the encoder.
    let control = EncoderControl::new();
    encoder_hook::install(&mut enc_builder, metrics, Arc::clone(&control));

    let enc = enc_builder.build()?;
    let encoder_handle_out = enc.handle_out();

    ctx.connect(encoder_handle_out, output_hdl, 0)?;

    let cursor_discard = (0..capture.nb_streams())
        .find(|&i| capture.stream_type(i) == StreamType::Cursor)
        .map(|i| create_cursor_discard(Arc::clone(&ctx), capture.stream_handle(i)))
        .transpose()?;

    let pipeline = VideoPipeline {
        _capture: capture,
        _fg: fg,
        _enc: enc,
        _cursor_discard: cursor_discard,
    };
    Ok((pipeline, control))
}
