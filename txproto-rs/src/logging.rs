// Project Kyber: logging.rs
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

use std::borrow::Cow;
use std::ffi::{CStr, CString};

use libc::{c_char, c_void};
use log::warn;

use txproto_sys as sys;

use crate::Result;

mod bindgen_extras {
    // System-V x86_64 (macOS/Linux) exposes va_list as `[__va_list_tag; 1]`,
    // which decays to a pointer; the simpler pointer-shaped va_list on macOS
    // aarch64 and Windows is emitted by bindgen as the `va_list` alias.
    #[cfg(all(any(target_os = "macos", target_os = "linux"), target_arch = "x86_64"))]
    pub type Valist = *mut crate::sys::__va_list_tag;

    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        target_os = "windows"
    ))]
    pub type Valist = crate::sys::va_list;

    unsafe extern "C" {
        pub fn vsnprintf(
            __s: *mut libc::c_char,
            __maxlen: libc::size_t,
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
    unsafe {
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
