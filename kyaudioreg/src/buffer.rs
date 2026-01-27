// Project Kyber: buffer.rs
// Copyright © 2022-2026 Kyber SAS
// SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0
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

use crate::memory::AudioRegulatorSharedMemory;
use std::sync::atomic::Ordering;

pub(crate) struct AudioBuffer {
    memory: AudioRegulatorSharedMemory,
}

impl AudioBuffer {
    pub(crate) fn new(memory: AudioRegulatorSharedMemory) -> Self {
        Self { memory }
    }

    pub(crate) fn read(&self, dst: &mut [u8]) -> u32 {
        let dst_len: u32 = dst.len().try_into().unwrap();
        let alloc_size = self.memory.get_alloc_size();
        let sample_size = self.memory.get_sample_size();
        assert!(dst_len % sample_size == 0);

        // Only the reader can write tail without synchronization, so
        // Ordering::Relaxed is sufficient
        let tail = self.memory.get_buffer_tail(Ordering::Relaxed);

        // The head cursor is updated after the data is written to the array
        let head = self.memory.get_buffer_head(Ordering::Acquire);

        let can_read = (alloc_size + head - tail) % alloc_size;
        let samples_count = std::cmp::min(dst_len / sample_size, can_read);
        if samples_count == 0 {
            return 0;
        }

        let right_count = std::cmp::min(alloc_size - tail, samples_count);
        let right_bytes = (right_count * sample_size) as usize;
        let right_offset = (tail * sample_size) as usize;
        self.memory
            .buffer_read(right_offset, &mut dst[..right_bytes]);

        if samples_count > right_count {
            let left_count = samples_count - right_count;
            let left_bytes = (left_count * sample_size) as usize;
            self.memory
                .buffer_read(0, &mut dst[right_bytes..right_bytes + left_bytes]);
        }

        let new_tail = (tail + samples_count) % alloc_size;
        self.memory.set_buffer_tail(new_tail, Ordering::Release);

        samples_count
    }

    pub(crate) fn skip(&self, samples: u32) -> u32 {
        let alloc_size = self.memory.get_alloc_size();

        // Only the reader can write tail without synchronization, so
        // Ordering::Relaxed is sufficient
        let tail = self.memory.get_buffer_tail(Ordering::Relaxed);

        // The head cursor is updated after the data is written to the array
        let head = self.memory.get_buffer_head(Ordering::Acquire);

        let can_read = (alloc_size + head - tail) % alloc_size;
        let samples_count = std::cmp::min(samples, can_read);

        let new_tail = (tail + samples_count) % alloc_size;
        self.memory.set_buffer_tail(new_tail, Ordering::Release);

        samples_count
    }

    pub(crate) fn write(&self, src: &[u8]) -> u32 {
        let src_len: u32 = src.len().try_into().unwrap();
        let alloc_size = self.memory.get_alloc_size();
        let sample_size = self.memory.get_sample_size();
        assert!(src_len % sample_size == 0);

        // Only the writer thread can write head, so Ordering::Relaxed is
        // sufficient
        let head = self.memory.get_buffer_head(Ordering::Relaxed);

        // The tail cursor is updated after the data is consumed by the reader
        let tail = self.memory.get_buffer_tail(Ordering::Acquire);

        let can_write = (alloc_size + tail - head - 1) % alloc_size;
        let samples_count = std::cmp::min(src_len / sample_size, can_write);
        if samples_count == 0 {
            return 0;
        }

        let right_count = std::cmp::min(alloc_size - head, samples_count);
        let right_bytes = (right_count * sample_size) as usize;
        let right_offset = (head * sample_size) as usize;
        self.memory.buffer_write(right_offset, &src[..right_bytes]);

        if samples_count > right_count {
            let left_count = samples_count - right_count;
            let left_bytes = (left_count * sample_size) as usize;
            self.memory
                .buffer_write(0, &src[right_bytes..right_bytes + left_bytes]);
        }

        let new_head = (head + samples_count) % alloc_size;
        self.memory.set_buffer_head(new_head, Ordering::Release);

        samples_count
    }

    pub(crate) fn write_silence(&self, samples: u32) -> u32 {
        let alloc_size = self.memory.get_alloc_size();
        let sample_size = self.memory.get_sample_size();

        // Only the writer thread can write head, so Ordering::Relaxed is
        // sufficient
        let head = self.memory.get_buffer_head(Ordering::Relaxed);

        // The tail cursor is updated after the data is consumed by the reader
        let tail = self.memory.get_buffer_tail(Ordering::Acquire);

        let can_write = (alloc_size + tail - head - 1) % alloc_size;
        let samples_count = std::cmp::min(samples, can_write);
        if samples_count == 0 {
            return 0;
        }

        let right_count = std::cmp::min(alloc_size - head, samples_count);
        let right_bytes = (right_count * sample_size) as usize;
        let right_offset = (head * sample_size) as usize;
        self.memory.buffer_write_zeros(right_offset, right_bytes);

        if samples_count > right_count {
            let left_count = samples_count - right_count;
            let left_bytes = (left_count * sample_size) as usize;
            self.memory.buffer_write_zeros(0, left_bytes);
        }

        let new_head = (head + samples_count) % alloc_size;
        self.memory.set_buffer_head(new_head, Ordering::Release);

        samples_count
    }

    #[inline(always)]
    pub(crate) fn capacity(&self) -> u32 {
        // The actual capacity is (alloc_size - 1) so that head == tail is
        // non-ambiguous:
        //  - empty: tail == head
        //  - full: ((tail + 1) % alloc_size) == head
        let alloc_size = self.memory.get_alloc_size();
        alloc_size - 1
    }

    #[inline(always)]
    pub(crate) fn can_read(&self) -> u32 {
        let alloc_size = self.memory.get_alloc_size();
        let tail = self.memory.get_buffer_tail(Ordering::Acquire);
        let head = self.memory.get_buffer_head(Ordering::Acquire);
        (alloc_size + head - tail) % alloc_size
    }
}
