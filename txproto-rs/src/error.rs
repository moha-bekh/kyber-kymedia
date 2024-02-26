use std::ffi::CStr;

use thiserror::Error;

use txproto_sys as sys;

#[derive(Debug, Error)]
pub enum Error {
    #[error("txproto error: {value} ({description:?}")]
    Txproto {
        value: i32,
        description: Option<String>,
    },
    #[error("txproto component {name} creation failed")]
    TxprotoComponentCreateFailed { name: String },
    #[error("NulError: {source:?}")]
    NulError {
        #[from]
        source: std::ffi::NulError,
    },
}

impl From<libc::c_int> for Error {
    fn from(value: libc::c_int) -> Self {
        const AV_ERROR_MAX_STRING_SIZE: usize = 64;
        let mut buf = vec![0i8; AV_ERROR_MAX_STRING_SIZE];

        let ret = unsafe {
            sys::av_strerror(
                value,
                buf.as_mut_slice().as_mut_ptr() as *mut _,
                buf.len() as _,
            )
        };

        let description = if ret == 0 {
            let s = unsafe { CStr::from_ptr(buf.as_ptr()) };
            s.to_str().map(|s| s.into()).ok()
        } else {
            None
        };

        Self::Txproto { value, description }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
