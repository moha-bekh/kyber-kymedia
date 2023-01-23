use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum Command {
    RtpGenerateVideoSdp { yuv444: bool },
    RtpStartVideo { ip: String, port: u16, yuv444: bool },
    RtpStartAudio { ip: String, port: u16 },
    Stop,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Event {
    RtpSdpGenerated { sdp: String },
}
