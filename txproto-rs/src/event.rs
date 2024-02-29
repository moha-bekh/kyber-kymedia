use std::sync::atomic::{AtomicPtr, Ordering};

use txproto_sys as sys;

enum EventState {
    Registered { ctx: AtomicPtr<sys::TXMainContext> },
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
}

impl Drop for Event {
    fn drop(&mut self) {
        match &self.state {
            EventState::Registered { ctx } => {
                unsafe { sys::tx_event_destroy(ctx.load(Ordering::Relaxed), self.event) };
            }
        }
    }
}
