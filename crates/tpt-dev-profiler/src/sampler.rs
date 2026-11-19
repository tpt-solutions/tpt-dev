//! The sample buffer, filled from a timer or interrupt hook.
//!
//! Each sample is a stack of program counters, innermost first. The buffer has
//! room for `N` samples of up to `D` frames each. Samples are never overwritten:
//! once the buffer is full, further samples are counted as dropped.
//!
//! Recording is lock-free and does not allocate. Every slot is written with
//! atomic stores, so a reader can take a snapshot while the hook keeps running.
//! A hook may be preempted by a higher-priority one. The two calls then take
//! different slots, and the reader skips a slot until its write is published.

use core::sync::atomic::{AtomicUsize, Ordering};

/// A fixed buffer of stack samples: room for `N` samples of up to `D` frames each.
pub struct Sampler<const N: usize, const D: usize> {
    next: AtomicUsize,
    ready: [AtomicUsize; N],
    depth: [AtomicUsize; N],
    frames: [[AtomicUsize; D]; N],
}

impl<const N: usize, const D: usize> Default for Sampler<N, D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize, const D: usize> Sampler<N, D> {
    /// An empty buffer.
    pub const fn new() -> Self {
        Self {
            next: AtomicUsize::new(0),
            ready: [const { AtomicUsize::new(0) }; N],
            depth: [const { AtomicUsize::new(0) }; N],
            frames: [const { [const { AtomicUsize::new(0) }; D] }; N],
        }
    }

    /// Records one sample. `frames` holds program counters, innermost first.
    /// Frames beyond `D` are ignored.
    ///
    /// Returns `false` when the buffer is full, and the sample is dropped.
    pub fn record(&self, frames: &[usize]) -> bool {
        let slot = match self
            .next
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            }) {
            Ok(previous) | Err(previous) => previous,
        };
        if slot >= N {
            return false;
        }
        let len = frames.len().min(D);
        self.depth[slot].store(len, Ordering::Relaxed);
        for (cell, &pc) in self.frames[slot].iter().zip(frames) {
            cell.store(pc, Ordering::Relaxed);
        }
        self.ready[slot].store(1, Ordering::Release);
        true
    }

    /// Calls `f` with each published sample, oldest first, innermost frame first.
    ///
    /// Stops at the first slot whose write has not been published yet.
    pub fn for_each(&self, mut f: impl FnMut(&[usize])) {
        let claimed = self.next.load(Ordering::Relaxed).min(N);
        let mut buf = [0usize; D];
        for slot in 0..claimed {
            if self.ready[slot].load(Ordering::Acquire) == 0 {
                break;
            }
            let len = self.depth[slot].load(Ordering::Relaxed).min(D);
            for (dst, src) in buf.iter_mut().zip(&self.frames[slot]).take(len) {
                *dst = src.load(Ordering::Relaxed);
            }
            f(&buf[..len]);
        }
    }

    /// The number of samples dropped because the buffer was full.
    pub fn dropped(&self) -> usize {
        self.next.load(Ordering::Relaxed).saturating_sub(N)
    }

    /// The number of samples the buffer can hold.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Empties the buffer. It takes `&mut self`, so no hook can be writing.
    pub fn clear(&mut self) {
        *self.next.get_mut() = 0;
        for ready in &mut self.ready {
            *ready.get_mut() = 0;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn collect<const N: usize, const D: usize>(s: &Sampler<N, D>) -> Vec<Vec<usize>> {
        let mut out = Vec::new();
        s.for_each(|f| out.push(f.to_vec()));
        out
    }

    #[test]
    fn samples_come_back_in_order_with_their_frames() {
        let s: Sampler<4, 3> = Sampler::new();
        assert!(s.record(&[0x10, 0x20]));
        assert!(s.record(&[0x30]));
        assert_eq!(collect(&s), [alloc::vec![0x10, 0x20], alloc::vec![0x30]]);
    }

    #[test]
    fn frames_beyond_the_depth_are_ignored() {
        let s: Sampler<1, 2> = Sampler::new();
        s.record(&[1, 2, 3, 4]);
        assert_eq!(collect(&s), [alloc::vec![1, 2]]);
    }

    #[test]
    fn a_full_buffer_drops_and_counts_samples() {
        let s: Sampler<2, 1> = Sampler::new();
        assert!(s.record(&[1]));
        assert!(s.record(&[2]));
        assert!(!s.record(&[3]));
        assert!(!s.record(&[4]));
        assert_eq!(s.dropped(), 2);
        assert_eq!(collect(&s), [alloc::vec![1], alloc::vec![2]]);
        assert_eq!(s.capacity(), 2);
    }

    #[test]
    fn clear_makes_room_again() {
        let mut s: Sampler<1, 1> = Sampler::new();
        s.record(&[7]);
        s.record(&[8]);
        assert_eq!(s.dropped(), 1);
        s.clear();
        assert_eq!(s.dropped(), 0);
        assert!(s.record(&[9]));
        assert_eq!(collect(&s), [alloc::vec![9]]);
    }

    #[test]
    fn an_unpublished_slot_hides_the_samples_after_it() {
        let s: Sampler<3, 1> = Sampler::new();
        s.record(&[1]);
        // Simulate a hook that has reserved slot 1 but not yet published it.
        s.next.store(2, Ordering::Relaxed);
        s.record(&[3]);
        assert_eq!(collect(&s), [alloc::vec![1]]);
    }
}
