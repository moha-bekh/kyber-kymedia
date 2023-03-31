use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum VideoCodec {
    H264,
    HEVC,
    AV1,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Command {
    RtpGenerateVideoSdp {
        yuv444: bool,
    },
    RtpStartVideo {
        ip: String,
        port: u16,
        yuv444: bool,
    },
    RtpStartAudio {
        ip: String,
        port: u16,
    },
    KymuxStartVideo {
        uri: String,
        codec: VideoCodec,
        yuv444: bool,
    },
    KymuxStartAudio {
        uri: String,
    },
    Stop,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Event {
    RtpSdpGenerated { sdp: String },
}
