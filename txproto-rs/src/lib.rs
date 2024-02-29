mod avtypes;
mod error;
mod event;
mod io;
mod logging;

use std::ffi::{CStr, CString};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use libc::{c_int, c_void};
use log::warn;

use txproto_sys as sys;

pub use avtypes::{Dictionary, DictionaryEntry, HWDeviceType, PixelFormat, Rational};
pub use error::{Error, Result};
pub use event::Event;
pub use io::{IOEntryType, IOSysEntry, IOSysType};
pub use logging::init_logger;

pub struct EncoderConfig<'a> {
    pub enc_name: &'a str,
    pub name: Option<&'a str>,
    pub options: Option<Dictionary>,
    pub init_opts: Option<Dictionary>,
    pub pix_fmt: PixelFormat,
}

impl<'a> Default for EncoderConfig<'a> {
    fn default() -> Self {
        Self {
            enc_name: "",
            name: None,
            options: None,
            init_opts: None,
            pix_fmt: PixelFormat::None,
        }
    }
}

#[derive(Clone)]
pub struct Handle {
    h: *mut sys::AVBufferRef,
}

struct IoCbData<F>
where
    F: FnMut(IOSysEntry),
{
    cb: F,
}

pub struct Txproto {
    ctx: AtomicPtr<sys::TXMainContext>,
}

impl Txproto {
    pub fn new() -> Result<Self> {
        let ctx = unsafe { sys::tx_new() };

        let ret = unsafe { sys::tx_init(ctx) };
        if ret != 0 {
            unsafe { sys::tx_free(ctx) };
            return Err(ret.into());
        }

        Ok(Self {
            ctx: AtomicPtr::new(ctx),
        })
    }

    pub fn set_log_level(&self, component: &str, level: &str) -> Result<()> {
        logging::set_log_level(component, level)
    }

    fn inner(&self) -> *mut sys::TXMainContext {
        self.ctx.load(Ordering::Relaxed)
    }

    pub fn set_epoch(&self, value: i64) -> Result<()> {
        let ret = unsafe { sys::tx_epoch_set(self.inner(), value) };
        if ret != 0 {
            return Err(ret.into());
        }

        Ok(())
    }

    extern "C" fn source_event_cb<F>(c_entry: *mut sys::IOSysEntry, userdata: *mut c_void) -> c_int
    where
        F: FnMut(IOSysEntry),
    {
        let entry = unsafe { &*c_entry };
        let data: &mut IoCbData<F> = unsafe { &mut *(userdata as *mut _) };

        let c_name = unsafe { sys::sp_class_get_name(c_entry as *mut _) };
        let Ok(name) = unsafe { CStr::from_ptr(c_name) }.to_str() else {
            warn!("source_event_cb: Failed to convert name");
            return 0;
        };

        let Some(sys_type) = IOSysType::from(entry.type_) else {
            warn!("source_event_cb: Failed to convert type");
            return 0;
        };

        let entry_type = match sys_type {
            IOSysType::None => IOEntryType::None,
            IOSysType::VideoDisplay => IOEntryType::Video {
                x: entry.x,
                y: entry.y,
                width: entry.width,
                height: entry.height,
                scale: entry.scale,
                framerate: entry.framerate.into(),
            },
            IOSysType::AudioMicrophone | IOSysType::AudioMonitor | IOSysType::AudioOutput => {
                IOEntryType::Audio {
                    sample_rate: entry.sample_rate,
                    sample_fmt: entry.sample_fmt,
                    channels: entry.channels,
                    channel_layout: entry.channel_layout,
                    volume: entry.volume,
                }
            }
        };

        let entry = IOSysEntry {
            name: name.into(),
            sys_type,
            entry_type,
            api_id: entry.api_id,
            identifier: entry.identifier,
            is_default: entry.is_default != 0,
        };

        (data.cb)(entry);

        0
    }

    extern "C" fn source_free_cb<F>(userdata: *mut c_void)
    where
        F: FnMut(IOSysEntry),
    {
        let _ = unsafe { Box::from_raw(userdata as *mut IoCbData<F>) };
    }

    pub fn register_io_cb<F>(&self, cb: F) -> Result<Event>
    where
        F: FnMut(IOSysEntry),
    {
        let data = Box::into_raw(Box::new(IoCbData { cb }));

        let event = unsafe {
            sys::tx_io_register_cb(
                self.inner(),
                ptr::null_mut(),
                Some(Self::source_event_cb::<F>),
                Some(Self::source_free_cb::<F>),
                data as *mut _,
            )
        };
        if event.is_null() {
            return Err(Error::TxprotoComponentCreateFailed {
                name: "IO Callback".into(),
            });
        }

        Ok(Event::new(&self.ctx, event))
    }

    pub fn create_io(&self, identifier: u32, options: Option<Dictionary>) -> Result<Handle> {
        let options = options.map_or(ptr::null_mut(), |d| d.take());

        let h = unsafe { sys::tx_io_create(self.inner(), identifier, options) };
        if h.is_null() {
            return Err(Error::TxprotoComponentCreateFailed { name: "IO".into() });
        }

        Ok(Handle { h })
    }

    pub fn create_encoder(&self, config: EncoderConfig) -> Result<Handle> {
        let enc_name = CString::new(config.enc_name)?;
        let enc_name_ptr = enc_name.as_ptr();

        let name = if let Some(name) = config.name {
            Some(CString::new(name)?)
        } else {
            None
        };

        let name_ptr = name.map_or(ptr::null(), |s| s.as_ptr());

        let options = config.options.map_or(ptr::null_mut(), |d| d.take());
        let init_opts = config.init_opts.map_or(ptr::null_mut(), |d| d.take());

        let config = sys::TxEncoderOptions {
            enc_name: enc_name_ptr,
            name: name_ptr,
            options,
            init_opts,
            pix_fmt: config.pix_fmt.to_sys(),
        };

        let h = unsafe { sys::tx_encoder_create(self.inner(), &config as *const _) };
        if h.is_null() {
            return Err(Error::TxprotoComponentCreateFailed {
                name: "Encoder".into(),
            });
        }

        Ok(Handle { h })
    }

    pub fn create_muxer(
        &self,
        out_url: &str,
        out_format: &str,
        options: Option<Dictionary>,
        init_opts: Option<Dictionary>,
    ) -> Result<Handle> {
        let out_url = CString::new(out_url)?;
        let out_format = CString::new(out_format)?;

        let options = options.map_or(ptr::null_mut(), |d| d.take());
        let init_opts = init_opts.map_or(ptr::null_mut(), |d| d.take());

        let h = unsafe {
            sys::tx_muxer_create(
                self.inner(),
                out_url.as_ptr(),
                out_format.as_ptr(),
                options,
                init_opts,
            )
        };
        if h.is_null() {
            return Err(Error::TxprotoComponentCreateFailed {
                name: "Muxer".into(),
            });
        }

        Ok(Handle { h })
    }

    pub fn create_filtergraph(
        &self,
        graph: &str,
        hwctx_type: HWDeviceType,
        init_opts: Option<Dictionary>,
    ) -> Result<Handle> {
        let graph = CString::new(graph)?;
        let init_opts = init_opts.map_or(ptr::null_mut(), |d| d.take());

        let h = unsafe {
            sys::tx_filtergraph_create(self.inner(), graph.as_ptr(), hwctx_type.to_sys(), init_opts)
        };
        if h.is_null() {
            return Err(Error::TxprotoComponentCreateFailed {
                name: "Filtergraph".into(),
            });
        }

        Ok(Handle { h })
    }

    pub fn create_packet_sink(&self, uri: &str) -> Result<Handle> {
        let uri = CString::new(uri)?;

        let h = unsafe { sys::tx_packetsink_create(self.inner(), uri.as_ptr()) };
        if h.is_null() {
            return Err(Error::TxprotoComponentCreateFailed {
                name: "PacketSink".into(),
            });
        }

        Ok(Handle { h })
    }

    pub fn link(&self, src: &Handle, dst: &Handle, src_stream_id: i32) -> Result<()> {
        let ret = unsafe { sys::tx_link(self.inner(), src.h, dst.h, src_stream_id) };
        if ret != 0 {
            return Err(ret.into());
        }

        Ok(())
    }

    pub fn commit(&self) -> Result<()> {
        let ret = unsafe { sys::tx_commit(self.inner()) };
        if ret != 0 {
            return Err(ret.into());
        }

        Ok(())
    }
}

impl Drop for Txproto {
    fn drop(&mut self) {
        unsafe { sys::tx_free(self.inner()) };
    }
}
