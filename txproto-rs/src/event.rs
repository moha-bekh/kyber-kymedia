use std::sync::atomic::{AtomicPtr, Ordering};

use txproto_sys as sys;

pub struct Event {
    ctx: AtomicPtr<sys::TXMainContext>,
    event: *mut sys::AVBufferRef,
}

impl Event {
    pub(crate) fn new(ctx: &AtomicPtr<sys::TXMainContext>, event: *mut sys::AVBufferRef) -> Self {
        Self {
            ctx: AtomicPtr::new(ctx.load(Ordering::Relaxed)),
            event,
        }
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe { sys::tx_event_destroy(self.ctx.load(Ordering::Relaxed), self.event) };
    }
}
