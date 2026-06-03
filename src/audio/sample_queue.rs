use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use super::format::SAMPLE_SIZE;

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
        let mut read = self.read.load(Ordering::Relaxed);
        if write.saturating_sub(read) > self.capacity() {
            read = write - self.capacity();
        }

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

    #[test]
    fn zero_fills_when_empty() {
        let queue = SampleQueue::with_capacity(2, 2);
        let mut output = vec![255; SAMPLE_SIZE * 2];

        assert_eq!(queue.fill_bytes(&mut output), 2);

        assert!(output.iter().all(|byte| *byte == 0));
    }
}
