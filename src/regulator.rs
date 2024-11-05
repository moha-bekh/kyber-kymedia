use crate::average::Average;
use crate::buffer::AudioBuffer;
use crate::compensator::AudioCompensator;
use crate::memory::AudioRegulatorSharedMemory;

#[allow(unused)]
use log::{debug, error, info, warn};
use std::sync::atomic::Ordering;
use std::time::Duration;

#[cfg(not(target_family = "wasm"))]
use std::time::Instant;

#[cfg(target_family = "wasm")]
use wasmtimer::std::Instant;

pub struct AudioRegulatorPush {
    memory: AudioRegulatorSharedMemory,
    buf: AudioBuffer,
    sample_rate: u32,
    target_buffering: u32,

    compensator: AudioCompensator,
    avg_buffering: Average,
    samples_since_resync: u32,
    compensation_active: bool,
    tmp: Vec<u8>,
    next_expected_push: Option<Instant>,
}

pub struct AudioRegulatorPull {
    memory: AudioRegulatorSharedMemory,
    buf: AudioBuffer,
    target_buffering: u32,
}

impl AudioRegulatorPull {
    pub fn new(memory: AudioRegulatorSharedMemory, target_buffering: u32) -> Self {
        let buf = AudioBuffer::new(memory.clone());
        Self {
            memory,
            buf,
            target_buffering,
        }
    }

    pub fn pull(&mut self, dst: &mut [u8]) {
        let _lock = self.memory.lock();
        let sample_size = self.memory.get_sample_size() as usize;
        assert!(dst.len() % sample_size == 0);
        let count = (dst.len() / sample_size) as u32;

        let played = self.memory.get_played(Ordering::Relaxed);
        if !played {
            let buffered_samples = self.buf.can_read();
            // Wait util the buffer is filled up to at least target_buffering
            // before playing
            if buffered_samples < self.target_buffering {
                // Delay playback starting to reach the target buffering. Fill the
                // whole buffer with silence (len is small compared to the
                // arbitrary margin value).
                dst.fill(0);
                return;
            }
        }

        let read = self.buf.read(dst);
        if read < count {
            let silence = count - read;
            // Insert silence. In theory, the inserted silent samples replace the
            // missing real samples, which will arrive later, so they should be
            // dropped to keep the latency minimal. However, this would cause very
            // audible glitches, so let the clock compensation restore the target
            // latency.
            dst[read as usize * sample_size..].fill(0);

            let received = self.memory.get_received(Ordering::Relaxed);
            if received {
                self.memory.add_underflow(silence, Ordering::Relaxed);
            }
        }

        self.memory.set_played(true, Ordering::Relaxed);
    }
}

impl AudioRegulatorPush {
    pub fn new(
        memory: AudioRegulatorSharedMemory,
        sample_rate: u32,
        target_buffering: u32,
    ) -> Self {
        let buf = AudioBuffer::new(memory.clone());
        let sample_size = memory.get_sample_size() as usize;
        let compensator = AudioCompensator::new(sample_size);
        let avg_buffering = Average::new(128);
        let tmp = vec![0; 16 * 1024 * sample_size];
        Self {
            memory,
            buf,
            sample_rate,
            target_buffering,
            compensator,
            avg_buffering,
            samples_since_resync: 0,
            compensation_active: false,
            tmp,
            next_expected_push: None,
        }
    }

    pub fn push(&mut self, missing_samples: u32, data: &[u8]) {
        let sample_size = self.memory.get_sample_size() as usize;
        assert!(data.len() % sample_size == 0);
        let input_samples = (data.len() / sample_size) as u32;

        let now = Instant::now();
        if let Some(next_expected_push) = self.next_expected_push {
            // Instant::duration_since() does not support now < next_expected_push
            if now > next_expected_push
                && now.duration_since(next_expected_push) > Duration::from_millis(200)
            {
                // Consider it is a discontinuity (typically because silence packets
                // were not captured/transmitted)
                let can_read = self.buf.can_read();
                if input_samples + can_read < self.target_buffering {
                    let silence = self.target_buffering - can_read - input_samples;
                    self.buf.write_silence(silence);
                }

                // Reset state
                self.avg_buffering.avg = self.target_buffering as f32;
                self.compensator.set_compensation(0, 0);
                self.compensation_active = false;
                self.samples_since_resync = 0;
                self.memory.reset_underflow(Ordering::Relaxed);
            }
        }

        let packet_duration =
            Duration::from_micros(input_samples as u64 * 1_000_000 / self.sample_rate as u64);
        self.next_expected_push = Some(now + packet_duration);

        let mut samples: u32 = self
            .compensator
            .filter(&mut self.tmp, data)
            .try_into()
            .unwrap();
        let mut packet = &self.tmp[..samples as usize * sample_size];

        let cap = self.buf.capacity();
        if samples > cap {
            // Very very unlikely: a single resampled frame should never
            // exceed the audio buffer size (or something is very wrong).
            // Ignore the first bytes anyway.
            packet = &self.tmp[(samples - cap) as usize * sample_size..];
            samples = cap;
        }

        let mut skipped_samples = 0;

        let mut underflow = 0;
        let played = self.memory.get_played(Ordering::Relaxed);
        let max_buffered_samples = if played {
            underflow = self.memory.consume_underflow(Ordering::Relaxed);
            self.target_buffering * 11 / 10 + 60 * 48
        } else {
            // Playback not started yet, do not accumulate too much, this would
            // cause unnecessary delay (and glitches to compensate) on start
            self.target_buffering + 10 * 48
        };

        if missing_samples > 0 {
            let preconsumed = std::cmp::min(missing_samples, underflow);
            let missing_samples = missing_samples - preconsumed;
            underflow -= preconsumed;
            debug!("Audio preconsumed missing samples = {preconsumed}");

            let can_read = self.buf.can_read();
            if can_read + samples < self.target_buffering {
                let silence =
                    std::cmp::min(missing_samples, self.target_buffering - can_read - samples);
                self.buf.write_silence(silence);
            }
        }

        let written = self.buf.write(packet);
        if written < samples {
            // Should never happen in practice, samples are already dropped if
            // buffering exceeds a threshold which is very small compared to
            // the whole buffer size
            let dropped = samples - written;
            warn!("Audio buffer full, {dropped} samples dropped");
        }

        let mut can_read = self.buf.can_read();
        if can_read > max_buffered_samples {
            let mut skip_samples = 0;

            let _lock = self.memory.lock();

            can_read = self.buf.can_read();
            if can_read > max_buffered_samples {
                skip_samples = can_read - max_buffered_samples;
                let r = self.buf.skip(skip_samples);
                assert_eq!(r, skip_samples);
                skipped_samples += skip_samples;
            }

            if skip_samples > 0 {
                if played {
                    debug!("[Audio] Buffering threshold exceeded, skipping {skip_samples} samples");
                } else {
                    debug!("[Audio] Playback not started, skipping {skip_samples} samples");
                }
            }
        }

        self.memory.set_received(true, Ordering::Relaxed);

        if played {
            if underflow > 0 {
                info!("[Audio] Buffer underflow: {underflow} samples");
            }

            let instant_compensation = written as i32 - input_samples as i32;
            let _inserted_silence = underflow as i32;
            let _dropped = skipped_samples as i32;

            // The compensation must apply instantly, it must not be smoothed
            //self.avg_buffering.avg += (instant_compensation + inserted_silence - dropped) as f32;
            self.avg_buffering.avg += instant_compensation as f32;
            if self.avg_buffering.avg < 0.0 {
                // Since dropping samples instantly reduces buffering, the
                // difference is applied immediately to the average value,
                // assuming that the delay between the producer and the
                // consumer will be caught up.
                //
                // However, when this assumption is not valid, the average
                // buffering may decrease indefinitely. Prevent it to become
                // negative to limit the consequences.
                self.avg_buffering.avg = 0.0;
            }

            // However, the buffering level must be smoothed
            self.avg_buffering.push(can_read as f32);

            self.samples_since_resync += written;
            if self.samples_since_resync >= self.sample_rate {
                // Recompute compensation every second
                self.samples_since_resync = 0;

                let mut diff = self.target_buffering as i32 - self.avg_buffering.avg as i32;
                let threshold = if self.compensation_active {
                    self.sample_rate / 1000 // 1ms
                } else {
                    self.sample_rate * 4 / 1000 // 4ms
                };

                if diff.unsigned_abs() < threshold || diff < 0 && can_read < self.target_buffering {
                    // Do not compensate for small values, the error is just noise.
                    // Do not accelerate if the instant buffering level is
                    // below the target, this would increase underflow.
                    diff = 0;
                }

                // Compensate the diff over 4 seconds (but will be recomputed after 1 second)
                let distance = 4 * self.sample_rate;
                // Limit compensation rate to 2%
                let abs_max_diff = distance as i32 / 50;
                let diff = std::cmp::max(-abs_max_diff, std::cmp::min(abs_max_diff, diff));
                info!(
                    "[Audio] Buffering: target={} avg={} cur={} compensation={}",
                    self.target_buffering, self.avg_buffering.avg, can_read, diff
                );

                self.compensator.set_compensation(diff, distance);
                self.compensation_active = diff != 0;
            }
        }
    }
}
