use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use super::format::SAMPLE_SIZE;

/// Smallest cushion kept in the queue behind the reader, about 1.3 ms at
/// 48 kHz. A tiny output quantum would otherwise leave no room at all for
/// scheduling jitter between the capture and the playback callback.
const MIN_CUSHION_SAMPLES: usize = 64;

/// Pulls the reader forward when the queue has drifted into a backlog.
///
/// Capture and playback are driven by the same graph clock, so in the steady
/// state the backlog is constant. What it settles on, however, is decided by
/// the start up transient: the capture stream produces samples from the moment
/// it connects, while the playback side only starts draining once its own
/// stream is running. Without this resync that head start is never given back
/// and every later sample inherits it as latency.
///
/// The reader is only moved once the leftover grows past twice the cushion, so
/// ordinary jitter never triggers a resync and the correction stays rare
/// enough to be inaudible.
fn resynced_read(read: usize, write: usize, n_frames: usize, capacity: usize) -> usize {
    let cushion = n_frames.max(MIN_CUSHION_SAMPLES);
    let target = (n_frames + cushion).min(capacity);
    let limit = (n_frames + cushion * 2).min(capacity);

    let read = if write.saturating_sub(read) > limit {
        write - target
    } else {
        read
    };

    // Anything older than the ring itself has already been overwritten.
    read.max(write.saturating_sub(capacity))
}

#[derive(Debug)]
pub(super) struct SampleQueue {
    samples: Box<[AtomicU32]>,
    read: AtomicUsize,
    write: AtomicUsize,
}

impl SampleQueue {
    pub(super) fn with_capacity(_capacity: usize, max_samples: usize) -> Self {
        let capacity = max_samples.max(1);
        let samples = (0..capacity)
            .map(|_| AtomicU32::new(0.0_f32.to_bits()))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            samples,
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
        }
    }

    pub(super) fn push_sample(&self, sample: f32) {
        let write = self.write.load(Ordering::Relaxed);

        self.samples[write % self.capacity()].store(sample.to_bits(), Ordering::Relaxed);
        self.write.store(write.saturating_add(1), Ordering::Release);
    }

    pub(super) fn fill_bytes(&self, output: &mut [u8]) -> usize {
        let n_frames = output.len() / SAMPLE_SIZE;
        let write = self.write.load(Ordering::Acquire);
        let read = self.read.load(Ordering::Relaxed);
        let mut read = resynced_read(read, write, n_frames, self.capacity());

        for frame in output.chunks_exact_mut(SAMPLE_SIZE) {
            let sample = if read < write {
                let bits = self.samples[read % self.capacity()].load(Ordering::Relaxed);
                read = read.saturating_add(1);
                f32::from_bits(bits)
            } else {
                0.0
            };
            frame.copy_from_slice(&sample.to_le_bytes());
        }

        self.read.store(read, Ordering::Release);
        n_frames
    }

    fn capacity(&self) -> usize {
        self.samples.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::f32_from_le_slice;

    #[test]
    fn drops_oldest_samples_when_full() {
        let queue = SampleQueue::with_capacity(2, 2);
        queue.push_sample(0.25);
        queue.push_sample(0.5);
        queue.push_sample(0.75);

        let mut output = vec![0; SAMPLE_SIZE * 2];
        assert_eq!(queue.fill_bytes(&mut output), 2);

        let first = f32_from_le_slice(&output[..SAMPLE_SIZE]);
        let second = f32_from_le_slice(&output[SAMPLE_SIZE..]);

        assert_eq!(first, 0.5);
        assert_eq!(second, 0.75);
    }

    /// Regression for the latency reported in issue #1: the reader used to be
    /// pulled forward only once the backlog passed the whole ring, which is a
    /// full second of audio, so a start up burst stayed in front of the reader
    /// forever and every later sample was delivered a second late.
    #[test]
    fn a_start_up_burst_does_not_become_permanent_latency() {
        let queue = SampleQueue::with_capacity(0, 48_000);
        // A second of capture before the playback side ever ran.
        for index in 0..48_000 {
            queue.push_sample(index as f32);
        }

        let n_frames = 256;
        let mut output = vec![0; SAMPLE_SIZE * n_frames];
        queue.fill_bytes(&mut output);

        let first = f32_from_le_slice(&output[..SAMPLE_SIZE]);
        let backlog = 48_000.0 - first;

        assert!(
            backlog < 1_024.0,
            "expected the reader to skip to the newest samples, it was {backlog} behind"
        );
    }

    #[test]
    fn small_jitter_does_not_trigger_a_resync() {
        let queue = SampleQueue::with_capacity(0, 48_000);
        let n_frames = 256;
        // One quantum plus one cushion of head start is the intended steady
        // state, so nothing may be dropped here.
        for index in 0..(n_frames * 2) {
            queue.push_sample(index as f32);
        }

        let mut output = vec![0; SAMPLE_SIZE * n_frames];
        queue.fill_bytes(&mut output);

        assert_eq!(f32_from_le_slice(&output[..SAMPLE_SIZE]), 0.0);
    }

    #[test]
    fn resync_keeps_a_cushion_for_the_next_block() {
        // 8 000 samples of backlog against a 256 frame quantum.
        let read = resynced_read(0, 8_000, 256, 48_000);
        let leftover = 8_000 - read - 256;

        assert!(
            (64..=1_024).contains(&leftover),
            "expected a small cushion to survive the resync, got {leftover}"
        );
    }

    #[test]
    fn zero_fills_when_empty() {
        let queue = SampleQueue::with_capacity(2, 2);
        let mut output = vec![255; SAMPLE_SIZE * 2];

        assert_eq!(queue.fill_bytes(&mut output), 2);

        assert!(output.iter().all(|byte| *byte == 0));
    }
}
