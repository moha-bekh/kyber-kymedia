use crate::average::Average;
use crate::buffer::{AudioBuffer, AudioBufferRead, AudioBufferWrite};
use crate::compensator::AudioCompensator;

#[allow(unused)]
use log::{debug, error, info, warn};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(not(target_family = "wasm"))]
use std::time::Instant;

#[cfg(target_family = "wasm")]
use wasmtimer::std::Instant;

struct AudioRegulator {
    sample_rate: u32,
    sample_size: u32,
    target_buffering: u32,
    underflow: AtomicU32,
    received: AtomicBool,
    played: AtomicBool,
}

pub struct AudioRegulatorPush {
    reg: Arc<AudioRegulator>,
    buf: AudioBufferWrite,
    compensator: AudioCompensator,
    avg_buffering: Average,
    samples_since_resync: u32,
    compensation: i32,
    tmp: Vec<u8>,
    next_expected_push: Option<Instant>,
}

pub struct AudioRegulatorPull {
    reg: Arc<AudioRegulator>,
    buf: Arc<Mutex<AudioBufferRead>>,
}

pub fn audio_regulator_new(
    sample_rate: u32,
    sample_size: u32,
    capacity: u32,
    target_buffering: u32,
) -> (AudioRegulatorPush, AudioRegulatorPull) {
    let reg = Arc::new(AudioRegulator {
        sample_rate,
        sample_size,
        target_buffering,
        underflow: AtomicU32::new(0),
        received: AtomicBool::new(false),
        played: AtomicBool::new(false),
    });

    let (buf_read, buf_write) = AudioBuffer::new(sample_size, capacity);
    let push = AudioRegulatorPush::new(reg.clone(), buf_write, sample_size);
    let pull = AudioRegulatorPull::new(reg, buf_read);
    (push, pull)
}

impl AudioRegulatorPull {
    fn new(reg: Arc<AudioRegulator>, buf: Arc<Mutex<AudioBufferRead>>) -> Self {
        Self { reg, buf }
    }

    pub fn pull(&mut self, dst: &mut [u8]) {
        let buf = self.buf.lock().unwrap();
        let sample_size = self.reg.sample_size;
        assert!(dst.len() % sample_size as usize == 0);
        let count = (dst.len() / sample_size as usize) as u32;

        let played = self.reg.played.load(Ordering::Relaxed);
        if !played {
            let buffered_samples = buf.can_read();
            // Wait util the buffer is filled up to at least target_buffering
            // before playing
            if buffered_samples < self.reg.target_buffering {
                // Delay playback starting to reach the target buffering. Fill the
                // whole buffer with silence (len is small compared to the
                // arbitrary margin value).
                dst.fill(0);
                return;
            }
        }

        let read = buf.read(dst);
        if read < count {
            let silence = count - read;
            // Insert silence. In theory, the inserted silent samples replace the
            // missing real samples, which will arrive later, so they should be
            // dropped to keep the latency minimal. However, this would cause very
            // audible glitches, so let the clock compensation restore the target
            // latency.
            dst[(read * sample_size) as usize..].fill(0);

            let received = self.reg.received.load(Ordering::Relaxed);
            if received {
                self.reg.underflow.fetch_add(silence, Ordering::Relaxed);
            }
        }

        self.reg.played.store(true, Ordering::Relaxed);
    }

    pub fn sample_size(&self) -> u32 {
        self.reg.sample_size
    }
}

impl AudioRegulatorPush {
    fn new(reg: Arc<AudioRegulator>, buf: AudioBufferWrite, sample_size: u32) -> Self {
        let compensator = AudioCompensator::new(sample_size as usize);
        let avg_buffering = Average::new(128);
        let tmp = vec![0; 16 * 1024 * sample_size as usize];
        Self {
            reg,
            buf,
            compensator,
            avg_buffering,
            samples_since_resync: 0,
            compensation: 0,
            tmp,
            next_expected_push: None,
        }
    }

    pub fn push(&mut self, data: &[u8]) {
        let buf = &self.buf;
        let sample_size = self.reg.sample_size;
        assert!(data.len() % sample_size as usize == 0);
        let input_samples = (data.len() / sample_size as usize) as u32;

        let now = Instant::now();
        if let Some(next_expected_push) = self.next_expected_push {
            // Instant::duration_since() does not support now < next_expected_push
            if now > next_expected_push
                && now.duration_since(next_expected_push) > Duration::from_millis(200)
            {
                // Consider it is a discontinuity (typically because silence packets
                // were not captured/transmitted)
                let can_read = buf.can_read();
                if input_samples + can_read < self.reg.target_buffering {
                    let silence = self.reg.target_buffering - can_read - input_samples;
                    buf.write_silence(silence);
                }

                // Reset state
                self.avg_buffering.avg = self.reg.target_buffering as f32;
                self.compensator.set_compensation(0, 0);
                self.compensation = 0;
                self.samples_since_resync = 0;
                self.reg.underflow.store(0, Ordering::Relaxed);
            }
        }

        let packet_duration =
            Duration::from_micros(input_samples as u64 * 1_000_000 / self.reg.sample_rate as u64);
        self.next_expected_push = Some(now + packet_duration);

        let mut samples: u32 = self
            .compensator
            .filter(&mut self.tmp, data)
            .try_into()
            .unwrap();
        let mut packet = &self.tmp[..(samples * sample_size) as usize];

        let cap = buf.capacity();
        if samples > cap {
            // Very very unlikely: a single resampled frame should never
            // exceed the audio buffer size (or something is very wrong).
            // Ignore the first bytes anyway.
            packet = &self.tmp[((samples - cap) * sample_size) as usize..];
            samples = cap;
        }

        let mut skipped_samples = 0;

        let mut written = buf.write(packet);
        if written < samples {
            let mut remaining = samples - written;

            let buf_read = self.buf.read.lock().unwrap();

            // Retry with the lock
            written += self.buf.write(
                &packet[(written * sample_size) as usize
                    ..((written + remaining) * sample_size) as usize],
            );

            if written < samples {
                remaining = samples - written;
                // Still insufficient, drop old samples to make space
                skipped_samples = buf_read.skip(remaining);
                assert_eq!(skipped_samples, remaining);

                // Now there is enough space
                let w = self.buf.write(
                    &packet[(written * sample_size) as usize
                        ..((written + remaining) * sample_size) as usize],
                );
                assert_eq!(w, remaining);
            }
        }

        let mut underflow = 0;
        let played = self.reg.played.load(Ordering::Relaxed);
        let max_buffered_samples = if played {
            underflow = self.reg.underflow.swap(0, Ordering::Relaxed);
            self.reg.target_buffering * 11 / 10 + 60 * 48
        } else {
            // Playback not started yet, do not accumulate too much, this would
            // cause unnecessary delay (and glitches to compensate) on start
            self.reg.target_buffering + 10 * 48
        };

        let mut can_read = self.buf.can_read();
        if can_read > max_buffered_samples {
            let mut skip_samples = 0;

            let buf_read = self.buf.read.lock().unwrap();

            can_read = self.buf.can_read();
            if can_read > max_buffered_samples {
                skip_samples = can_read - max_buffered_samples;
                let r = buf_read.skip(skip_samples);
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

        self.reg.received.store(true, Ordering::Relaxed);

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
            if self.samples_since_resync >= self.reg.sample_rate {
                // Recompute compensation every second
                self.samples_since_resync = 0;

                let mut diff = self.reg.target_buffering as i32 - self.avg_buffering.avg as i32;
                let threshold = if self.compensation != 0 {
                    self.reg.sample_rate / 1000 // 1ms
                } else {
                    self.reg.sample_rate * 4 / 1000 // 4ms
                };

                if diff.unsigned_abs() < threshold
                    || diff < 0 && can_read < self.reg.target_buffering
                {
                    // Do not compensate for small values, the error is just noise.
                    // Do not accelerate if the instant buffering level is
                    // below the target, this would increase underflow.
                    diff = 0;
                }

                // Compensate the diff over 4 seconds (but will be recomputed after 1 second)
                let distance = 4 * self.reg.sample_rate;
                // Limit compensation rate to 2%
                let abs_max_diff = distance as i32 / 50;
                let diff = std::cmp::max(-abs_max_diff, std::cmp::min(abs_max_diff, diff));
                info!(
                    "[Audio] Buffering: target={} avg={} cur={} compensation={}",
                    self.reg.target_buffering, self.avg_buffering.avg, can_read, diff
                );

                if diff != self.compensation {
                    self.compensator.set_compensation(diff, distance);
                    self.compensation = diff;
                }
            }
        }
    }
}
