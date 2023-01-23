use std::ffi::CString;
use std::ptr;

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
}
