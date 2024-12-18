use std::fmt::Debug;
use std::ops::Deref;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[allow(unused_imports)]
use crate::NotAvailable;

#[cfg(all(feature = "js", target_family = "wasm"))]
mod js;
mod native;

#[derive(Debug, Clone)]
pub struct AudioRegulatorSharedMemory {
    driver: Arc<dyn AudioRegulatorSharedMemoryDriver>,
}

impl AudioRegulatorSharedMemory {
    pub fn new(driver: impl AudioRegulatorSharedMemoryDriver + 'static) -> Self {
        let driver = Arc::new(driver);
        Self { driver }
    }

    pub fn native(alloc_size: u32, sample_size: u32) -> Self {
        Self::new(native::AudioRegulatorNativeSharedMemoryDriver::new(
            alloc_size,
            sample_size,
        ))
    }

    #[cfg(all(feature = "js", target_family = "wasm"))]
    pub fn available() -> bool {
        use js_sys::wasm_bindgen::JsCast;

        let global = js_sys::global();
        let global_scope = global.unchecked_ref::<js::GlobalScope>();

        match js_sys::Reflect::has(global_scope, &"SharedArrayBuffer".into()) {
            Ok(defined) => defined,
            Err(err) => {
                log::warn!("Failed to check if SharedArrayBuffer is defined: {err:?}");
                false
            }
        }
    }

    #[cfg(all(feature = "js", target_family = "wasm"))]
    pub fn create_sab(
        alloc_size: u32,
        sample_size: u32,
    ) -> std::result::Result<js_sys::SharedArrayBuffer, NotAvailable> {
        if !Self::available() {
            log::warn!("SharedArrayBuffer is undefined");
            return Err(NotAvailable);
        }

        Ok(js::AudioRegulatorJSSharedMemoryDriver::create_sab(
            alloc_size,
            sample_size,
        ))
    }

    #[cfg(all(feature = "js", target_family = "wasm"))]
    pub fn js(sab: js_sys::SharedArrayBuffer, alloc_size: u32, sample_size: u32) -> Self {
        Self::new(js::AudioRegulatorJSSharedMemoryDriver::from(
            sab,
            alloc_size,
            sample_size,
        ))
    }
}

pub trait AudioRegulatorSharedMemoryDriver: Debug {
    fn get_alloc_size(&self) -> u32; // in samples
    fn get_sample_size(&self) -> u32;

    fn buffer_read(&self, offset_bytes: usize, dst: &mut [u8]);
    fn buffer_write(&self, offset_bytes: usize, src: &[u8]);
    fn buffer_write_zeros(&self, offset_bytes: usize, len: usize);

    fn get_buffer_head(&self, ordering: Ordering) -> u32;
    fn set_buffer_head(&self, head: u32, ordering: Ordering);
    fn get_buffer_tail(&self, ordering: Ordering) -> u32;
    fn set_buffer_tail(&self, tail: u32, ordering: Ordering);

    fn consume_underflow(&self, ordering: Ordering) -> u32;
    fn reset_underflow(&self, ordering: Ordering);
    fn add_underflow(&self, underflow: u32, ordering: Ordering);
    fn get_received(&self, ordering: Ordering) -> bool;
    fn set_received(&self, received: bool, ordering: Ordering);
    fn get_played(&self, ordering: Ordering) -> bool;
    fn set_played(&self, played: bool, ordering: Ordering);

    fn lock(&self) -> AudioRegulatorMutexGuard<'_>;
}

impl Deref for AudioRegulatorSharedMemory {
    type Target = dyn AudioRegulatorSharedMemoryDriver;

    fn deref(&self) -> &Self::Target {
        &*self.driver
    }
}

pub struct AudioRegulatorMutexGuard<'a> {
    _driver: Box<dyn AudioRegulatorMutexGuardDriver + 'a>,
}

pub trait AudioRegulatorMutexGuardDriver {}

impl<'a> AudioRegulatorMutexGuard<'a> {
    pub fn new(driver: impl AudioRegulatorMutexGuardDriver + 'a) -> Self {
        Self {
            _driver: Box::new(driver),
        }
    }
}
