//! A reader takes snapshots while a hook records samples on another thread.

#![allow(clippy::unwrap_used)]

use tpt_dev_profiler::Sampler;

const SAMPLES: usize = 5000;

#[test]
fn reader_only_sees_complete_samples_while_the_hook_records() {
    let sampler: Sampler<SAMPLES, 3> = Sampler::new();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for i in 0..SAMPLES {
                assert!(sampler.record(&[i, i + 1, i + 2]));
            }
        });
        loop {
            let mut seen = 0usize;
            sampler.for_each(|frames| {
                // Each sample is written whole, in order, so its frames must match
                // its position in the buffer.
                assert_eq!(frames, [seen, seen + 1, seen + 2]);
                seen += 1;
            });
            if seen == SAMPLES {
                break;
            }
            std::thread::yield_now();
        }
    });
    assert_eq!(sampler.dropped(), 0);
}
