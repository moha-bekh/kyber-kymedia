use txproto_sys as sys;

use crate::Rational;

#[derive(Debug, PartialEq)]
pub enum IOSysType {
    None,
    VideoDisplay,
    AudioMicrophone,
    AudioMonitor,
    AudioOutput,
}

impl IOSysType {
    pub fn from(c_type: sys::IOType) -> Option<Self> {
        match c_type {
            sys::IOType_SP_IO_TYPE_NONE => Some(Self::None),
            sys::IOType_SP_IO_TYPE_VIDEO_DISPLAY => Some(Self::VideoDisplay),
            sys::IOType_SP_IO_TYPE_AUDIO_MICROPHONE => Some(Self::AudioMicrophone),
            sys::IOType_SP_IO_TYPE_AUDIO_MONITOR => Some(Self::AudioMonitor),
            sys::IOType_SP_IO_TYPE_AUDIO_OUTPUT => Some(Self::AudioOutput),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum IOEntryType {
    None,
    Video {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        scale: i32,
        framerate: Rational,
    },
    Audio {
        sample_rate: i32,
        sample_fmt: i32,
        channels: i32,
        channel_layout: u64,
        volume: f32,
    },
}

#[derive(Debug)]
pub struct IOSysEntry {
    pub name: String,
    pub sys_type: IOSysType,
    pub entry_type: IOEntryType,
    pub identifier: u32,
    pub api_id: u32,
    pub is_default: bool,
}
