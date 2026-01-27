use crate::memory::AudioRegulatorSharedMemory;
use crate::regulator::{AudioRegulatorPull, AudioRegulatorPush};

use std::ffi;
use std::sync::atomic::{AtomicPtr, Ordering};

#[repr(C)]
pub enum KyAudioRegLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<log::Level> for KyAudioRegLogLevel {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => Self::Error,
            log::Level::Warn => Self::Warn,
            log::Level::Info => Self::Info,
            log::Level::Debug => Self::Debug,
            log::Level::Trace => Self::Trace,
        }
    }
}

#[repr(C)]
pub enum KyAudioRegLogLevelFilter {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<KyAudioRegLogLevelFilter> for log::LevelFilter {
    fn from(level: KyAudioRegLogLevelFilter) -> Self {
        match level {
            KyAudioRegLogLevelFilter::Off => Self::Off,
            KyAudioRegLogLevelFilter::Error => Self::Error,
            KyAudioRegLogLevelFilter::Warn => Self::Warn,
            KyAudioRegLogLevelFilter::Info => Self::Info,
            KyAudioRegLogLevelFilter::Debug => Self::Debug,
            KyAudioRegLogLevelFilter::Trace => Self::Trace,
        }
    }
}

type KyAudioRegLogCb = extern "C" fn(
    level: KyAudioRegLogLevel,
    file: *const ffi::c_char,
    line: ffi::c_uint,
    target: *const ffi::c_char,
    msg: *const ffi::c_char,
    userdata: *mut ffi::c_void,
);

struct Logger {
    log_cb: KyAudioRegLogCb,
    userdata: AtomicPtr<ffi::c_void>,
}

impl log::Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let file = ffi::CString::new(record.file().unwrap_or("<unknown file>"))
            .expect("Invalid log file reference");
        let line = record.line().unwrap_or(0) as ffi::c_uint;
        let target = ffi::CString::new(record.target()).expect("Invalid log target");
        let msg = ffi::CString::new(format!("{}", record.args())).expect("Invalid log msg");
        let userdata = self.userdata.load(Ordering::Relaxed);
        (self.log_cb)(
            record.level().into(),
            file.as_ptr(),
            line,
            target.as_ptr(),
            msg.as_ptr(),
            userdata,
        );
    }

    fn flush(&self) {}
}

#[no_mangle]
pub unsafe extern "C" fn kyaudioreg_init_logger(
    max_level: KyAudioRegLogLevelFilter,
    log_cb: KyAudioRegLogCb,
    userdata: *mut ffi::c_void,
) -> bool {
    let userdata = AtomicPtr::new(userdata);
    let logger = Box::new(Logger { log_cb, userdata });

    if log::set_boxed_logger(logger).is_ok() {
        log::set_max_level(max_level.into());
        true
    } else {
        false
    }
}

pub struct KyAudioRegulator {
    push: AudioRegulatorPush,
    pull: AudioRegulatorPull,
}

#[no_mangle]
pub unsafe extern "C" fn kyaudioreg_new(
    sample_rate: u32,
    sample_size: u32,
    alloc_size: u32,
    target_buffering: u32,
) -> Box<KyAudioRegulator> {
    let memory = AudioRegulatorSharedMemory::native(alloc_size, sample_size);
    let push = AudioRegulatorPush::new(memory.clone(), sample_rate, target_buffering);
    let pull = AudioRegulatorPull::new(memory, target_buffering);
    Box::new(KyAudioRegulator { push, pull })
}

#[no_mangle]
#[allow(unused_variables)]
pub unsafe extern "C" fn kyaudioreg_delete(audioreg: Box<KyAudioRegulator>) {
    // implicitly drop audioreg
}

#[no_mangle]
pub unsafe extern "C" fn kyaudioreg_push(
    audioreg: &mut KyAudioRegulator,
    missing_samples: u32,
    src: *const u8,
    len: libc::size_t,
) {
    let data = std::slice::from_raw_parts(src, len);
    audioreg.push.push(missing_samples, data);
}

#[no_mangle]
pub unsafe extern "C" fn kyaudioreg_pull(
    audioreg: &mut KyAudioRegulator,
    dst: *mut u8,
    len: libc::size_t,
) {
    let data = std::slice::from_raw_parts_mut(dst, len);
    audioreg.pull.pull(data);
}
