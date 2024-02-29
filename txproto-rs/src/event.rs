use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use libc::{c_int, c_void};

use txproto_sys as sys;

use crate::{Handle, HandleEvent, Result};

pub enum EventType {
    Error,
}

impl EventType {
    pub(crate) fn to_sys(&self) -> sys::SPEventType {
        match self {
            Self::Error => sys::SPEventType_SP_EVENT_ON_ERROR,
        }
    }
}

enum EventState {
    Dangling,
    Registered { ctx: AtomicPtr<sys::TXMainContext> },
}

struct HandleEventCbData<F>
where
    F: FnMut(HandleEvent),
{
    cb: F,
}

pub struct Event {
    state: EventState,
    event: *mut sys::AVBufferRef,
}

impl Event {
    pub(crate) fn new_registered(
        ctx: &AtomicPtr<sys::TXMainContext>,
        event: *mut sys::AVBufferRef,
    ) -> Self {
        Self {
            state: EventState::Registered {
                ctx: AtomicPtr::new(ctx.load(Ordering::Relaxed)),
            },
            event,
        }
    }

    pub fn new<F>(type_: EventType, event_cb: F) -> Self
    where
        F: FnMut(HandleEvent),
    {
        let event = unsafe {
            sys::sp_event_create(
                Some(Self::error_cb::<F>),
                Some(Self::destroy_cb::<F>),
                std::mem::size_of::<HandleEventCbData<F>>() as _,
                ptr::null_mut(),
                type_.to_sys(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };

        unsafe {
            let data_ptr = sys::av_buffer_get_opaque(event) as *mut HandleEventCbData<F>;
            ptr::write(data_ptr, HandleEventCbData::<F> { cb: event_cb });
        };

        Self {
            state: EventState::Dangling,
            event,
        }
    }

    extern "C" fn error_cb<F>(
        _event_ref: *mut sys::AVBufferRef,
        callback_ctx: *mut c_void,
        _ctx: *mut c_void,
        _dep_ctx: *mut c_void,
        _data: *mut c_void,
    ) -> c_int
    where
        F: FnMut(HandleEvent),
    {
        let data = unsafe { &mut *(callback_ctx as *mut HandleEventCbData<F>) };
        (data.cb)(HandleEvent::Error);

        0
    }

    unsafe extern "C" fn destroy_cb<F>(
        callback_ctx: *mut libc::c_void,
        _ctx: *mut libc::c_void,
        _dep_ctx: *mut libc::c_void,
    ) where
        F: FnMut(HandleEvent),
    {
        unsafe { ptr::drop_in_place(callback_ctx as *mut HandleEventCbData<F>) };
    }

    pub(crate) fn register(
        &mut self,
        ctx: &AtomicPtr<sys::TXMainContext>,
        h: &Handle,
    ) -> Result<()> {
        let ret = unsafe { sys::tx_event_register(ctx.load(Ordering::Relaxed), h.h, self.event) };
        if ret != 0 {
            return Err(ret.into());
        }

        self.state = EventState::Registered {
            ctx: AtomicPtr::new(ctx.load(Ordering::Relaxed)),
        };

        Ok(())
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        match &self.state {
            EventState::Dangling => {
                unsafe { sys::av_buffer_unref(&mut self.event as *mut _ as *mut _) };
            }
            EventState::Registered { ctx } => {
                unsafe { sys::tx_event_destroy(ctx.load(Ordering::Relaxed), self.event) };
            }
        }
    }
}
