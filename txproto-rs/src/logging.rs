use std::borrow::Cow;
use std::ffi::{CStr, CString};

use libc::{c_char, c_void};
use log::warn;

use txproto_sys as sys;

use crate::Result;

#[cfg(target_os = "windows")]
mod bindgen_extras {
    pub type Valist = crate::sys::va_list;

    extern "C" {
        pub fn vsnprintf(
            __s: *mut libc::c_char,
            __maxlen: libc::c_ulong,
            __format: *const libc::c_char,
            __arg: Valist,
        ) -> libc::c_int;
    }
}

#[cfg(not(target_os = "windows"))]
mod bindgen_extras {
    pub type Valist = *mut crate::sys::__va_list_tag;

    extern "C" {
        pub fn vsnprintf(
            __s: *mut libc::c_char,
            __maxlen: libc::c_ulong,
            __format: *const libc::c_char,
            __arg: Valist,
        ) -> libc::c_int;
    }
}

unsafe extern "C" fn log_trampoline<F>(
    component: *const c_char,
    level: sys::SPLogLevel,
    fmt: *const c_char,
    args: bindgen_extras::Valist,
    userdata: *mut c_void,
) where
    F: Fn(Cow<str>, log::Level, Cow<str>) + Send + 'static,
{
    let f: &F = &*(userdata as *const _);

    let level = match level {
        sys::SPLogLevel_SP_LOG_FATAL | sys::SPLogLevel_SP_LOG_ERROR => log::Level::Error,
        sys::SPLogLevel_SP_LOG_WARN => log::Level::Warn,
        sys::SPLogLevel_SP_LOG_INFO => log::Level::Info,
        sys::SPLogLevel_SP_LOG_VERBOSE | sys::SPLogLevel_SP_LOG_DEBUG => log::Level::Debug,
        sys::SPLogLevel_SP_LOG_TRACE => log::Level::Trace,
        sys::SPLogLevel_SP_NOLOG
        | sys::SPLogLevel_SP_LOG_QUIET
        | sys::SPLogLevel_SP_LOG_LIST
        | sys::SPLogLevel_SP_LOG_LIST_END => return,
        _ => {
            warn!("Unexpected log level {level:?}");
            return;
        }
    };

    const BUF_SIZE: usize = 1024;
    let mut buf: [c_char; BUF_SIZE] = [0; BUF_SIZE];
    bindgen_extras::vsnprintf(buf.as_mut_ptr(), BUF_SIZE as _, fmt, args);

    let component = CStr::from_ptr(component).to_string_lossy();
    let component = Cow::from(format!("txproto::{component}"));

    let line = CStr::from_ptr(buf.as_ptr()).to_string_lossy();

    f(component, level, line);
}

fn set_log_cb<F>(f: F)
where
    F: Fn(Cow<str>, log::Level, Cow<str>) + Send + 'static,
{
    unsafe {
        sys::tx_set_log_cb(
            Some(log_trampoline::<F>),
            Box::into_raw(Box::new(f)) as *mut _,
        );
    }
}

fn rslog_cb(component: Cow<str>, level: log::Level, msg: Cow<str>) {
    let msg = msg.to_string();

    log::log!(target: &component, level, "{}", msg.trim_end());
}

pub fn init_logger() {
    set_log_cb(rslog_cb);
}

pub fn set_log_level(component: &str, level: &str) -> Result<()> {
    let component = CString::new(component)?;
    let level = CString::new(level)?;

    let ret = unsafe { sys::tx_log_set_ctx_lvl_str(component.as_ptr(), level.as_ptr()) };
    if ret != 0 {
        return Err(ret.into());
    }

    Ok(())
}
