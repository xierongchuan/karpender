use std::{collections::VecDeque, sync::Mutex};

use super::format::SAMPLE_SIZE;

#[derive(Debug)]
pub(super) struct SampleQueue {
    samples: Mutex<VecDeque<f32>>,
    max_samples: usize,
}

impl SampleQueue {
    pub(super) fn with_capacity(capacity: usize, max_samples: usize) -> Self {
        Self {
            samples: Mutex::new(VecDeque::with_capacity(capacity)),
            max_samples,
        }
    }

    pub(super) fn push_sample(&self, sample: f32) {
        let Ok(mut samples) = self.samples.try_lock() else {
            return;
        };

        if samples.len() >= self.max_samples {
            samples.pop_front();
        }
        samples.push_back(sample);
    }

    pub(super) fn fill_bytes(&self, output: &mut [u8]) -> usize {
        let n_frames = output.len() / SAMPLE_SIZE;
        let Ok(mut samples) = self.samples.try_lock() else {
            output.fill(0);
            return n_frames;
        };

        for frame in output.chunks_exact_mut(SAMPLE_SIZE) {
            let sample = samples.pop_front().unwrap_or_default();
            frame.copy_from_slice(&sample.to_le_bytes());
        }

        n_frames
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
