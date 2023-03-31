use std::ffi::{CStr, CString};
use std::ptr;

use libc::{c_char, c_void};
use log::warn;

use txproto_sys as sys;

fn to_cstr(s: &str) -> CString {
    CString::new(s.to_owned()).expect("Error: Unexpected null byte")
}

pub struct Context {
    ptr: *mut sys::TXMainContext,
}

impl Context {
    pub fn new(
        script_name: &str,
        script_entrypoint: &str,
        args: Option<Vec<&str>>,
    ) -> Option<Self> {
        let args: Vec<_> = if let Some(argv) = args {
            argv.into_iter()
                .map(|x| CString::new(x).expect("Error: Unexpected null byte"))
                .collect()
        } else {
            vec![]
        };

        let script_name = to_cstr(script_name);
        let script_entrypoint = to_cstr(script_entrypoint);

        let mut ptr = ptr::null_mut();

        let ret = if args.is_empty() {
            unsafe {
                sys::txproto_start(
                    script_name.as_ptr(),
                    script_entrypoint.as_ptr(),
                    0,
                    &ptr::null(),
                    &mut ptr as *mut _,
                )
            }
        } else {
            let args_c_ptr: Vec<_> = args.iter().map(|x| x.as_ptr()).collect();

            unsafe {
                sys::txproto_start(
                    script_name.as_ptr(),
                    script_entrypoint.as_ptr(),
                    args_c_ptr.len() as i32,
                    args_c_ptr.as_ptr(),
                    &mut ptr as *mut _,
                )
            }
        };

        if ret == 0 {
            Some(Self { ptr })
        } else {
            None
        }
    }

    pub fn stop(&self) {
        unsafe { sys::txproto_stop(self.ptr) };
    }

    pub fn execute(&self, chunk: &str) {
        let chunk = to_cstr(chunk);

        let ret = unsafe { sys::txproto_execute(self.ptr, chunk.as_ptr()) };
        println!("Execute. ret={ret}");
    }

    pub fn get_global(&self, name: &str) -> Option<String> {
        let name = to_cstr(name);
        let mut out_string: *mut c_char = std::ptr::null_mut();

        unsafe { sys::txproto_getglobal(self.ptr, name.as_ptr(), &mut out_string) };

        if !out_string.is_null() {
            let s = unsafe { CStr::from_ptr(out_string) };
            let s = s.to_string_lossy().to_string();

            unsafe { libc::free(out_string as *mut _) };
            Some(s)
        } else {
            None
        }
    }

    extern "C" fn callback_trampoline<F>(json: *const c_char, opaque: *mut c_void)
    where
        F: FnMut(String),
    {
        let ret = unsafe { CStr::from_ptr(json).to_str() };
        let json = match ret {
            Ok(json) => json,
            Err(err) => {
                warn!("Failed to convert JSON to &str: {err:?}");
                return;
            }
        };

        let cb = unsafe { &mut *(opaque as *mut F) };
        cb(json.to_string());
    }

    pub fn set_callback<F>(&self, varname: &str, f: F)
    where
        F: FnMut(String),
    {
        let varname = to_cstr(varname);

        unsafe {
            sys::txproto_set_cb(
                self.ptr,
                varname.as_ptr(),
                Some(Self::callback_trampoline::<F>),
                Box::into_raw(Box::new(f)) as *mut _,
            )
        };
    }
}
