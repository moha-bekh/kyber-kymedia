use super::{
    AudioRegulatorMutexGuard, AudioRegulatorMutexGuardDriver, AudioRegulatorSharedMemoryDriver,
};

use std::sync::atomic::Ordering;

#[derive(Debug)]
pub(crate) struct AudioRegulatorJSSharedMemoryDriver {
    _sab: js_sys::SharedArrayBuffer,
    // Atomic fields
    fields: js_sys::Int32Array,
    // Circular buffer
    data: js_sys::Uint8Array,

    mutex: JSMutex,
    sample_size: u32,
    alloc_size: u32, // in samples
}

impl AudioRegulatorJSSharedMemoryDriver {
    #[allow(unused)] // Reimplemented in the AudioWorklet
    const FIELD_MUTEX: u32 = 0;

    const FIELD_HEAD: u32 = 1;
    const FIELD_TAIL: u32 = 2;
    const FIELD_UNDERFLOW: u32 = 3;
    const FIELD_RECEIVED: u32 = 4;
    const FIELD_PLAYED: u32 = 5;

    const FIELDS: u32 = 6;
    const FIELDS_SIZE: u32 = Self::FIELDS * std::mem::size_of::<i32>() as u32;

    pub(crate) fn create_sab(alloc_size: u32, sample_size: u32) -> js_sys::SharedArrayBuffer {
        let memory_size = Self::FIELDS_SIZE + alloc_size * sample_size;
        js_sys::SharedArrayBuffer::new(memory_size)
    }

    pub(crate) fn from(sab: js_sys::SharedArrayBuffer, alloc_size: u32, sample_size: u32) -> Self {
        let fields =
            js_sys::Int32Array::new_with_byte_offset_and_length(&sab, 0, Self::FIELDS as _);
        let mutex = JSMutex::new(fields.subarray(0, 1));
        let data = js_sys::Uint8Array::new_with_byte_offset_and_length(
            &sab,
            Self::FIELDS_SIZE as _,
            sample_size * alloc_size,
        );

        Self {
            _sab: sab,
            fields,
            data,
            mutex,
            sample_size,
            alloc_size,
        }
    }
}

impl AudioRegulatorSharedMemoryDriver for AudioRegulatorJSSharedMemoryDriver {
    #[inline]
    fn get_alloc_size(&self) -> u32 {
        self.alloc_size
    }

    #[inline]
    fn get_sample_size(&self) -> u32 {
        self.sample_size
    }

    fn buffer_read(&self, offset_bytes: usize, dst: &mut [u8]) {
        assert!(offset_bytes % self.sample_size as usize == 0);
        assert!(dst.len() % self.sample_size as usize == 0);
        assert!((offset_bytes + dst.len()) / self.sample_size as usize <= self.alloc_size as usize);

        let offset_bytes = offset_bytes.try_into().unwrap();
        let len: u32 = dst.len().try_into().unwrap();

        let src_slice = self.data.subarray(offset_bytes, offset_bytes + len);
        src_slice.copy_to(dst);
    }

    fn buffer_write(&self, offset_bytes: usize, src: &[u8]) {
        assert!(offset_bytes % self.sample_size as usize == 0);
        assert!(src.len() % self.sample_size as usize == 0);
        assert!((offset_bytes + src.len()) / self.sample_size as usize <= self.alloc_size as usize);

        let offset_bytes = offset_bytes.try_into().unwrap();
        let len: u32 = src.len().try_into().unwrap();

        let dst_slice = self.data.subarray(offset_bytes, offset_bytes + len);
        dst_slice.copy_from(src);
    }

    fn buffer_write_zeros(&self, offset_bytes: usize, len: usize) {
        assert!(offset_bytes % self.sample_size as usize == 0);
        assert!(len % self.sample_size as usize == 0);
        assert!((offset_bytes + len) / self.sample_size as usize <= self.alloc_size as usize);

        let offset_bytes = offset_bytes.try_into().unwrap();
        let len: u32 = len.try_into().unwrap();

        let dst_slice = self.data.subarray(offset_bytes, offset_bytes + len);
        dst_slice.fill(0, 0, len);
    }

    #[inline]
    fn get_buffer_head(&self, _ordering: Ordering) -> u32 {
        js_sys::Atomics::load(&self.fields, Self::FIELD_HEAD)
            .unwrap()
            .try_into()
            .unwrap()
    }

    #[inline]
    fn set_buffer_head(&self, head: u32, _ordering: Ordering) {
        js_sys::Atomics::store(&self.fields, Self::FIELD_HEAD, head.try_into().unwrap()).unwrap();
    }

    #[inline]
    fn get_buffer_tail(&self, _ordering: Ordering) -> u32 {
        js_sys::Atomics::load(&self.fields, Self::FIELD_TAIL)
            .unwrap()
            .try_into()
            .unwrap()
    }

    #[inline]
    fn set_buffer_tail(&self, tail: u32, _ordering: Ordering) {
        js_sys::Atomics::store(&self.fields, Self::FIELD_TAIL, tail.try_into().unwrap()).unwrap();
    }

    #[inline]
    fn consume_underflow(&self, _ordering: Ordering) -> u32 {
        js_sys::Atomics::exchange(&self.fields, Self::FIELD_UNDERFLOW, 0)
            .unwrap()
            .try_into()
            .unwrap()
    }

    #[inline]
    fn reset_underflow(&self, _ordering: Ordering) {
        js_sys::Atomics::store(&self.fields, Self::FIELD_UNDERFLOW, 0).unwrap();
    }

    #[inline]
    #[allow(unused)] // Reimplemented in the AudioWorklet
    fn add_underflow(&self, underflow: u32, _ordering: Ordering) {
        js_sys::Atomics::add(
            &self.fields,
            Self::FIELD_UNDERFLOW,
            underflow.try_into().unwrap(),
        )
        .unwrap();
    }

    #[inline]
    fn get_received(&self, _ordering: Ordering) -> bool {
        js_sys::Atomics::load(&self.fields, Self::FIELD_RECEIVED).unwrap() != 0
    }

    #[inline]
    fn set_received(&self, received: bool, _ordering: Ordering) {
        js_sys::Atomics::store(
            &self.fields,
            Self::FIELD_RECEIVED,
            if received { 1 } else { 0 },
        )
        .unwrap();
    }

    #[inline]
    fn get_played(&self, _ordering: Ordering) -> bool {
        js_sys::Atomics::load(&self.fields, Self::FIELD_PLAYED).unwrap() != 0
    }

    #[inline]
    fn set_played(&self, played: bool, _ordering: Ordering) {
        js_sys::Atomics::store(&self.fields, Self::FIELD_PLAYED, if played { 1 } else { 0 })
            .unwrap();
    }

    fn lock(&self) -> AudioRegulatorMutexGuard<'_> {
        let mutex_guard = JSMutexGuard::lock(&self.mutex);
        AudioRegulatorMutexGuard::new(mutex_guard)
    }
}

#[derive(Debug)]
pub(crate) struct JSMutex {
    data: js_sys::Int32Array,
}

impl JSMutex {
    const UNLOCKED: i32 = 0;
    const LOCKED: i32 = 1;

    pub(crate) fn new(data: js_sys::Int32Array) -> Self {
        Self { data }
    }

    pub(crate) fn lock(&self) {
        loop {
            if js_sys::Atomics::compare_exchange(&self.data, 0, Self::UNLOCKED, Self::LOCKED)
                .unwrap()
                == Self::UNLOCKED
            {
                return;
            }
            js_sys::Atomics::wait(&self.data, 0, Self::LOCKED).unwrap();
        }
    }

    pub(crate) fn unlock(&self) {
        if js_sys::Atomics::compare_exchange(&self.data, 0, Self::LOCKED, Self::UNLOCKED).unwrap()
            != Self::LOCKED
        {
            panic!("Inconsistent mutex state: unlock() unlocked mutex");
        }
        js_sys::Atomics::notify_with_count(&self.data, 0, 1).unwrap();
    }
}

pub(crate) struct JSMutexGuard<'a> {
    mutex: &'a JSMutex,
}

impl<'a> JSMutexGuard<'a> {
    pub(crate) fn lock(mutex: &'a JSMutex) -> Self {
        mutex.lock();
        Self { mutex: &mutex }
    }
}

impl Drop for JSMutexGuard<'_> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

impl AudioRegulatorMutexGuardDriver for JSMutexGuard<'_> {}
