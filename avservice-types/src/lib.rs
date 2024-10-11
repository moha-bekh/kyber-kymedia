use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum VideoCodec {
    H264,
    HEVC,
    AV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Display {
    pub id: u32,
    pub name: String,
    pub height: i32,
    pub width: i32,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Command {
    EnumerateDisplays,
    RtpGenerateVideoSdp {
        display_id: u32,
        yuv444: bool,
        intra_refresh: Option<bool>,
    },
    RtpStartVideo {
        display_id: u32,
        ip: String,
        port: u16,
        bitrate: u32,
        yuv444: bool,
        intra_refresh: Option<bool>,
    },
    RtpStartAudio {
        ip: String,
        port: u16,
    },
    KymuxStartVideo {
        display_id: u32,
        uri: String,
        bitrate: u32,
        codec: VideoCodec,
        yuv444: bool,
        intra_refresh: Option<bool>,
    },
    KymuxStartAudio {
        uri: String,
    },
    VideoSetBitrate {
        bitrate: i32,
    },
    VideoForceIdr,
    Stop,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum PipelineType {
    Audio,
    Video,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum PipelineStage {
    Initialization,
    Acquisition,
    Encoder,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum PipelineEventType {
    Error,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PipelineEvent {
    pub type_: PipelineType,
    pub stage: PipelineStage,
    pub event: PipelineEventType,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Event {
    DisplayList { displays: Vec<Display> },
    DisplayListUpdated { displays: Vec<Display> },
    RtpSdpGenerated { sdp: String },
    PipelineEvent(PipelineEvent),
}
