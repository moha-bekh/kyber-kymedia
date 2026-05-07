// Project Kyber: native.rs
// Copyright © 2022-2026 Kyber SAS
// SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0-or-later
//
// This file is both under dual license: AGPLv3 and a Commercial one.
//
// ----
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use crate::memory::{
    AudioRegulatorMutexGuard, AudioRegulatorMutexGuardDriver, AudioRegulatorSharedMemoryDriver,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug)]
pub(crate) struct AudioRegulatorNativeSharedMemoryDriver {
    data: *mut u8,
    alloc_size: u32,
    sample_size: u32,
    head: AtomicU32,
    tail: AtomicU32,
    underflow: AtomicU32,
    received: AtomicBool,
    played: AtomicBool,
    mutex: Arc<Mutex<()>>,
}

impl AudioRegulatorNativeSharedMemoryDriver {
    pub(crate) fn new(alloc_size: u32, sample_size: u32) -> Self {
        let data = unsafe {
            let bytes = alloc_size as usize * sample_size as usize;
            let layout = std::alloc::Layout::from_size_align(bytes, 8).unwrap();
            std::alloc::alloc_zeroed(layout)
        };

        Self {
            data,
            alloc_size,
            sample_size,
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            underflow: AtomicU32::new(0),
            received: AtomicBool::new(false),
            played: AtomicBool::new(false),
            mutex: Arc::new(Mutex::new(())),
        }
    }
}

impl AudioRegulatorMutexGuardDriver for MutexGuard<'_, ()> {}

impl AudioRegulatorSharedMemoryDriver for AudioRegulatorNativeSharedMemoryDriver {
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
        let src_slice =
            unsafe { std::slice::from_raw_parts(self.data.add(offset_bytes), dst.len()) };
        dst.copy_from_slice(src_slice);
    }

    fn buffer_write(&self, offset_bytes: usize, src: &[u8]) {
        assert!(offset_bytes % self.sample_size as usize == 0);
        assert!(src.len() % self.sample_size as usize == 0);
        assert!((offset_bytes + src.len()) / self.sample_size as usize <= self.alloc_size as usize);
        let dst_slice =
            unsafe { std::slice::from_raw_parts_mut(self.data.add(offset_bytes), src.len()) };
        dst_slice.copy_from_slice(src);
    }

    fn buffer_write_zeros(&self, offset_bytes: usize, len: usize) {
        assert!(offset_bytes % self.sample_size as usize == 0);
        assert!(len % self.sample_size as usize == 0);
        assert!((offset_bytes + len) / self.sample_size as usize <= self.alloc_size as usize);
        let dst_slice = unsafe { std::slice::from_raw_parts_mut(self.data.add(offset_bytes), len) };
        dst_slice.fill(0);
    }

    #[inline]
    fn get_buffer_head(&self, ordering: Ordering) -> u32 {
        self.head.load(ordering)
    }

    #[inline]
    fn set_buffer_head(&self, head: u32, ordering: Ordering) {
        self.head.store(head, ordering);
    }

    #[inline]
    fn get_buffer_tail(&self, ordering: Ordering) -> u32 {
        self.tail.load(ordering)
    }

    #[inline]
    fn set_buffer_tail(&self, tail: u32, ordering: Ordering) {
        self.tail.store(tail, ordering);
    }

    #[inline]
    fn consume_underflow(&self, ordering: Ordering) -> u32 {
        self.underflow.swap(0, ordering)
    }

    #[inline]
    fn reset_underflow(&self, ordering: Ordering) {
        self.underflow.store(0, ordering);
    }

    #[inline]
    fn add_underflow(&self, underflow: u32, ordering: Ordering) {
        self.underflow.fetch_add(underflow, ordering);
    }

    #[inline]
    fn get_received(&self, ordering: Ordering) -> bool {
        self.received.load(ordering)
    }

    #[inline]
    fn set_received(&self, received: bool, ordering: Ordering) {
        self.received.store(received, ordering);
    }

    #[inline]
    fn get_played(&self, ordering: Ordering) -> bool {
        self.played.load(ordering)
    }

    #[inline]
    fn set_played(&self, played: bool, ordering: Ordering) {
        self.played.store(played, ordering);
    }

    fn lock(&self) -> AudioRegulatorMutexGuard<'_> {
        let mutex_guard = self.mutex.lock().unwrap();
        AudioRegulatorMutexGuard::new(mutex_guard)
    }
}

impl Drop for AudioRegulatorNativeSharedMemoryDriver {
    fn drop(&mut self) {
        let bytes = self.alloc_size as usize * self.sample_size as usize;
        let layout = std::alloc::Layout::from_size_align(bytes, 8).unwrap();
        unsafe { std::alloc::dealloc(self.data, layout) }
    }
}
