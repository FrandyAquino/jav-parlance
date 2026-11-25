use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};

/// Producer half, owned by the native capture thread.
///
/// `push` never blocks and never allocates: when the consumer falls behind,
/// the newest samples are dropped and counted instead of stalling the audio
/// device (which would cause glitches for every other app on the endpoint).
#[derive(Debug)]
pub struct AudioSink {
    producer: Producer<f32>,
    dropped: Arc<AtomicU64>,
}

/// Consumer half, owned by the DSP stage.
#[derive(Debug)]
pub struct AudioSource {
    consumer: Consumer<f32>,
    dropped: Arc<AtomicU64>,
}

pub fn channel(capacity_samples: usize) -> (AudioSink, AudioSource) {
    let (producer, consumer) = RingBuffer::new(capacity_samples);
    let dropped = Arc::new(AtomicU64::new(0));
    (AudioSink { producer, dropped: Arc::clone(&dropped) }, AudioSource { consumer, dropped })
}

impl AudioSink {
    /// Returns the number of samples written; the rest were dropped.
    pub fn push(&mut self, samples: &[f32]) -> usize {
        let writable = samples.len().min(self.producer.slots());
        if let Ok(chunk) = self.producer.write_chunk_uninit(writable) {
            chunk.fill_from_iter(samples[..writable].iter().copied());
        }
        let lost = samples.len() - writable;
        if lost > 0 {
            self.dropped.fetch_add(lost as u64, Ordering::Relaxed);
        }
        writable
    }

    pub fn is_consumer_alive(&self) -> bool {
        !self.producer.is_abandoned()
    }
}

impl AudioSource {
    /// Moves every available sample into `out`, returning how many were read.
    pub fn drain_into(&mut self, out: &mut Vec<f32>) -> usize {
        let available = self.consumer.slots();
        let Ok(chunk) = self.consumer.read_chunk(available) else { return 0 };
        let (first, second) = chunk.as_slices();
        out.extend_from_slice(first);
        out.extend_from_slice(second);
        chunk.commit_all();
        available
    }

    pub fn dropped_samples(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn is_producer_alive(&self) -> bool {
        !self.consumer.is_abandoned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfers_samples_in_order() {
        let (mut sink, mut source) = channel(8);
        assert_eq!(sink.push(&[1.0, 2.0, 3.0]), 3);
        let mut out = Vec::new();
        assert_eq!(source.drain_into(&mut out), 3);
        assert_eq!(out, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn overflow_drops_and_counts_instead_of_blocking() {
        let (mut sink, source) = channel(4);
        assert_eq!(sink.push(&[0.0; 6]), 4);
        assert_eq!(source.dropped_samples(), 2);
    }

    #[test]
    fn wraps_around_capacity() {
        let (mut sink, mut source) = channel(4);
        let mut out = Vec::new();
        for round in 0..5 {
            let value = round as f32;
            sink.push(&[value, value, value]);
            out.clear();
            source.drain_into(&mut out);
            assert_eq!(out, [value; 3]);
        }
    }

    #[test]
    fn concurrent_producer_consumer_preserves_every_sample() {
        let (mut sink, mut source) = channel(1024);
        let total = 100_000usize;
        let producer = std::thread::spawn(move || {
            let mut next = 0usize;
            while next < total {
                let block: Vec<f32> = (next..(next + 64).min(total)).map(|v| v as f32).collect();
                let mut offset = 0;
                while offset < block.len() {
                    offset += sink.push(&block[offset..]);
                }
                next += block.len();
            }
        });
        let mut received = Vec::with_capacity(total);
        while received.len() < total {
            source.drain_into(&mut received);
        }
        producer.join().unwrap();
        assert!(received.iter().enumerate().all(|(i, v)| *v == i as f32));
    }

    #[test]
    fn reports_abandoned_halves() {
        let (sink, source) = channel(4);
        drop(source);
        assert!(!sink.is_consumer_alive());
    }
}
