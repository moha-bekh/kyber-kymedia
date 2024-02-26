use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("libkypc error: {source:?}")]
    LibypcError {
        #[from]
        source: libkypc::Error,
    },
    #[error("Failed to delete Video SDP file: {reason:?}")]
    RtpVideoSdpFileDeleteFailed { reason: std::io::Error },
    #[error("RTP Video SDP file hasn't been found")]
    RtpVideoSdpFileNotFound,
    #[error("RTP Video SDP file parse failed")]
    RtpVideoSdpParseFailed,
    #[error("txproto error: {source:?}")]
    TxprotoError {
        #[from]
        source: txproto_rs::Error,
    },
    #[error("No default audio monitor found")]
    AudioNoDefaultMonitor,
    #[error("OsString '{s:?}' conversion to string failed")]
    OsStringConversion { s: std::ffi::OsString },
    #[error("Fail to parse configuration file")]
    ConfigParseFail,
    #[error("Fail to load configuration: {reason:?}")]
    ConfigLoadFail { reason: toml::de::Error },
    #[error("Thread has panicked")]
    ThreadPanicked,
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_: std::sync::PoisonError<T>) -> Error {
        Error::ThreadPanicked
    }
}

pub type Result<T> = std::result::Result<T, Error>;
