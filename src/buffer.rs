use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

pub(crate) struct AudioBuffer {
    data: *mut u8,
    alloc_size: u32,
    sample_size: u32,
    head: AtomicU32,
    tail: AtomicU32,
}

pub(crate) struct AudioBufferRead {
    buf: Arc<AudioBuffer>,
}

pub(crate) struct AudioBufferWrite {
    buf: Arc<AudioBuffer>,
    // The writer can read, but with a mutex
    pub(crate) read: Arc<Mutex<AudioBufferRead>>,
}

impl AudioBuffer {
    // There is only 1 writer (the producer), but 2 readers (the producer, in
    // corner cases, to discard old samples, and the consumer)
    pub(crate) fn new(
        sample_size: u32,
        capacity: u32,
    ) -> (Arc<Mutex<AudioBufferRead>>, AudioBufferWrite) {
        let buf = Arc::new(Self::new_internal(sample_size, capacity));

        let read = Arc::new(Mutex::new(AudioBufferRead { buf: buf.clone() }));

        let write = AudioBufferWrite {
            buf,
            read: read.clone(),
        };

        (read, write)
    }

    fn new_internal(sample_size: u32, capacity: u32) -> Self {
        let alloc_size = capacity + 1;
        let data = unsafe {
            let layout =
                std::alloc::Layout::from_size_align((alloc_size * sample_size) as usize, 8)
                    .unwrap();
            std::alloc::alloc_zeroed(layout)
        };

        Self {
            data,
            alloc_size,
            sample_size,
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
        }
    }

    #[inline(always)]
    pub(crate) fn capacity(&self) -> u32 {
        // The actual capacity is (alloc_size - 1) so that head == tail is
        // non-ambiguous:
        //  - empty: tail == head
        //  - full: ((tail + 1) % alloc_size) == head
        self.alloc_size - 1
    }

    #[inline(always)]
    pub(crate) fn can_read(&self) -> u32 {
        let alloc_size = self.alloc_size;
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        (alloc_size + head - tail) % alloc_size
    }
}

impl AudioBufferRead {
    pub(crate) fn read(&self, dst: &mut [u8]) -> u32 {
        let dst_len: u32 = dst.len().try_into().unwrap();
        let alloc_size = self.buf.alloc_size;
        let sample_size = self.buf.sample_size;
        assert!(dst_len % sample_size == 0);

        // Only the reader can write tail without synchronization, so
        // Ordering::Relaxed is sufficient
        let tail = self.buf.tail.load(Ordering::Relaxed);

        // The head cursor is updated after the data is written to the array
        let head = self.buf.head.load(Ordering::Acquire);

        let can_read = (alloc_size + head - tail) % alloc_size;
        let samples_count = std::cmp::min(dst_len / sample_size, can_read);

        let right_count = std::cmp::min(alloc_size - tail, samples_count);
        let right_bytes = (right_count * sample_size) as usize;
        let right_offset = (tail * sample_size) as isize;
        let src_slice =
            unsafe { std::slice::from_raw_parts(self.buf.data.offset(right_offset), right_bytes) };
        dst[..right_bytes].copy_from_slice(src_slice);

        if samples_count > right_count {
            let left_count = samples_count - right_count;
            let left_bytes = (left_count * sample_size) as usize;
            let src_slice = unsafe { std::slice::from_raw_parts(self.buf.data, left_bytes) };
            dst[right_bytes..right_bytes + left_bytes].copy_from_slice(src_slice);
        }

        let new_tail = (tail + samples_count) % alloc_size;
        self.buf.tail.store(new_tail, Ordering::Release);

        samples_count
    }

    pub(crate) fn skip(&self, samples: u32) -> u32 {
        let alloc_size = self.buf.alloc_size;

        // Only the reader can write tail without synchronization, so
        // Ordering::Relaxed is sufficient
        let tail = self.buf.tail.load(Ordering::Relaxed);

        // The head cursor is updated after the data is written to the array
        let head = self.buf.head.load(Ordering::Acquire);

        let can_read = (alloc_size + head - tail) % alloc_size;
        let samples_count = std::cmp::min(samples, can_read);

        let new_tail = (tail + samples_count) % alloc_size;
        self.buf.tail.store(new_tail, Ordering::Release);

        samples_count
    }

    #[inline(always)]
    pub(crate) fn can_read(&self) -> u32 {
        self.buf.can_read()
    }

    #[allow(unused)]
    #[inline(always)]
    pub(crate) fn capacity(&self) -> u32 {
        self.buf.capacity()
    }
}

impl AudioBufferWrite {
    pub(crate) fn write(&self, src: &[u8]) -> u32 {
        let src_len: u32 = src.len().try_into().unwrap();
        let alloc_size = self.buf.alloc_size;
        let sample_size = self.buf.sample_size;
        assert!(src_len % sample_size == 0);

        // Only the writer thread can write head, so Ordering::Relaxed is
        // sufficient
        let head = self.buf.head.load(Ordering::Relaxed);

        // The tail cursor is updated after the data is consumed by the reader
        let tail = self.buf.tail.load(Ordering::Acquire);

        let can_write = (alloc_size + tail - head - 1) % alloc_size;
        let samples_count = std::cmp::min(src_len / sample_size, can_write);

        let right_count = std::cmp::min(alloc_size - head, samples_count);
        let right_bytes = (right_count * sample_size) as usize;
        let right_offset = (head * sample_size) as isize;
        let dst_slice = unsafe {
            std::slice::from_raw_parts_mut(self.buf.data.offset(right_offset), right_bytes)
        };
        dst_slice.copy_from_slice(&src[..right_bytes]);

        if samples_count > right_count {
            let left_count = samples_count - right_count;
            let left_bytes = (left_count * sample_size) as usize;
            let dst_slice = unsafe { std::slice::from_raw_parts_mut(self.buf.data, left_bytes) };
            dst_slice.copy_from_slice(&src[right_bytes..right_bytes + left_bytes]);
        }

        let new_head = (head + samples_count) % alloc_size;
        self.buf.head.store(new_head, Ordering::Release);

        samples_count
    }

    pub(crate) fn write_silence(&self, samples: u32) -> u32 {
        let alloc_size = self.buf.alloc_size;
        let sample_size = self.buf.sample_size;

        // Only the writer thread can write head, so Ordering::Relaxed is
        // sufficient
        let head = self.buf.head.load(Ordering::Relaxed);

        // The tail cursor is updated after the data is consumed by the reader
        let tail = self.buf.tail.load(Ordering::Acquire);

        let can_write = (alloc_size + tail - head - 1) % alloc_size;
        let samples_count = std::cmp::min(samples, can_write);

        let right_count = std::cmp::min(alloc_size - head, samples_count);
        let right_bytes = (right_count * sample_size) as usize;
        let right_offset = (head * sample_size) as isize;
        let dst_slice = unsafe {
            std::slice::from_raw_parts_mut(self.buf.data.offset(right_offset), right_bytes)
        };
        dst_slice.fill(0);

        if samples_count > right_count {
            let left_count = samples_count - right_count;
            let left_bytes = (left_count * sample_size) as usize;
            let dst_slice = unsafe { std::slice::from_raw_parts_mut(self.buf.data, left_bytes) };
            dst_slice.fill(0);
        }

        let new_head = (head + samples_count) % alloc_size;
        self.buf.head.store(new_head, Ordering::Release);

        samples_count
    }

    #[inline(always)]
    pub(crate) fn can_read(&self) -> u32 {
        self.buf.can_read()
    }

    #[inline(always)]
    pub(crate) fn capacity(&self) -> u32 {
        self.buf.capacity()
    }
}
