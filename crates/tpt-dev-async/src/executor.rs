//! A fixed-capacity, priority-ordered executor with no heap allocation.

use core::future::Future;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::task::{Context, RawWaker, RawWakerVTable, Waker};

use crate::sync::{AtomicU32, Ordering};

/// Returned by [`Executor::spawn`] when every slot is in use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Full;

/// Counters for the work the executor has done. Use them to check worst-case
/// behaviour on the target or in a simulator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Rounds run.
    pub rounds: u64,
    /// Task polls, across all rounds.
    pub polls: u64,
    /// The most polls in any single round.
    pub max_polls_per_round: usize,
}

/// Where a task's waker reports its wake-up. Lives in the executor for its whole life,
/// so a waker kept after its task finished still refers to valid memory.
struct Wake {
    ready: *const AtomicU32,
    bit: u32,
}

struct Slot<'a> {
    fut: Pin<&'a mut (dyn Future<Output = ()> + 'a)>,
    priority: u8,
}

/// A fixed-capacity executor for up to `N` tasks (`N` at most 32).
///
/// Tasks are borrowed for the executor's lifetime, so their storage can be
/// static or on the stack. The executor sets a bit in a ready word when a task
/// is woken, and each round polls the ready tasks in priority order. Each task
/// is polled at most once per round, so a round does at most `N` polls.
///
/// Once tasks are spawned the executor must not move. For that reason the
/// methods take `Pin<&mut Self>`.
///
/// ```
/// use core::future::Future;
/// use core::pin::pin;
/// use core::task::Poll;
/// use tpt_dev_async::Executor;
///
/// struct Yield(bool);
/// impl Future for Yield {
///     type Output = ();
///     fn poll(mut self: core::pin::Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<()> {
///         if self.0 {
///             return Poll::Ready(());
///         }
///         self.0 = true;
///         cx.waker().wake_by_ref();
///         Poll::Pending
///     }
/// }
///
/// let mut exec = pin!(Executor::<2>::new());
/// let mut task = pin!(async { Yield(false).await });
/// exec.as_mut().spawn(task, 1).unwrap();
/// assert_eq!(exec.as_mut().run_until_idle(10), 2);
/// ```
pub struct Executor<'a, const N: usize> {
    slots: [Option<Slot<'a>>; N],
    wakes: [Wake; N],
    ready: AtomicU32,
    stats: Stats,
    _pinned: PhantomPinned,
}

impl<const N: usize> Default for Executor<'_, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, const N: usize> Executor<'a, N> {
    /// Creates an executor with no tasks. Fails to compile unless `1 <= N <= 32`.
    #[cfg(not(loom))]
    pub const fn new() -> Self {
        const {
            assert!(
                N > 0 && N <= 32,
                "the executor holds between 1 and 32 tasks"
            )
        };
        Self {
            slots: [const { None }; N],
            wakes: [const {
                Wake {
                    ready: core::ptr::null(),
                    bit: 0,
                }
            }; N],
            ready: AtomicU32::new(0),
            stats: Stats {
                rounds: 0,
                polls: 0,
                max_polls_per_round: 0,
            },
            _pinned: PhantomPinned,
        }
    }

    /// Creates an executor with no tasks.
    #[cfg(loom)]
    pub fn new() -> Self {
        const {
            assert!(
                N > 0 && N <= 32,
                "the executor holds between 1 and 32 tasks"
            )
        };
        Self {
            slots: core::array::from_fn(|_| None),
            wakes: core::array::from_fn(|_| Wake {
                ready: core::ptr::null(),
                bit: 0,
            }),
            ready: AtomicU32::new(0),
            stats: Stats {
                rounds: 0,
                polls: 0,
                max_polls_per_round: 0,
            },
            _pinned: PhantomPinned,
        }
    }

    /// Adds a task. Larger `priority` values run first. The task is ready from the start.
    pub fn spawn(
        self: Pin<&mut Self>,
        fut: Pin<&'a mut (dyn Future<Output = ()> + 'a)>,
        priority: u8,
    ) -> Result<usize, Full> {
        // SAFETY: the executor is only mutated field by field and never moved out of.
        let this = unsafe { self.get_unchecked_mut() };
        let i = this.slots.iter().position(Option::is_none).ok_or(Full)?;
        this.wakes[i] = Wake {
            ready: &this.ready,
            bit: 1 << i,
        };
        this.slots[i] = Some(Slot { fut, priority });
        this.ready.fetch_or(1 << i, Ordering::Release);
        Ok(i)
    }

    /// Runs one round: polls every task that is ready when the round starts,
    /// highest priority first. Returns the number of polls.
    pub fn run_round(self: Pin<&mut Self>) -> usize {
        // SAFETY: as in `spawn`.
        let this = unsafe { self.get_unchecked_mut() };
        // Take the ready set. Tasks woken during this round run in the next one.
        let mut remaining = this.ready.swap(0, Ordering::AcqRel);
        let mut polled = 0usize;
        loop {
            // Drop bits of finished tasks, then pick the highest-priority ready task.
            let mut best: Option<usize> = None;
            for i in 0..N {
                if remaining & (1 << i) == 0 {
                    continue;
                }
                match &this.slots[i] {
                    None => remaining &= !(1 << i),
                    Some(s) => {
                        let better = match best {
                            None => true,
                            Some(b) => this.slots[b]
                                .as_ref()
                                .is_some_and(|bs| s.priority > bs.priority),
                        };
                        if better {
                            best = Some(i);
                        }
                    }
                }
            }
            let Some(i) = best else {
                break;
            };
            remaining &= !(1 << i);
            polled += 1;
            let wake: *const Wake = &this.wakes[i];
            // SAFETY: `wake` points into `this.wakes`, which lives as long as the executor.
            // The vtable functions only read it and set a bit in the atomic ready word.
            let waker = unsafe { Waker::from_raw(RawWaker::new(wake.cast(), &VTABLE)) };
            let mut cx = Context::from_waker(&waker);
            if let Some(slot) = this.slots[i].as_mut()
                && slot.fut.as_mut().poll(&mut cx).is_ready()
            {
                this.slots[i] = None;
            }
        }
        this.stats.rounds += 1;
        this.stats.polls += polled as u64;
        this.stats.max_polls_per_round = this.stats.max_polls_per_round.max(polled);
        polled
    }

    /// Runs rounds until no task is ready, or until `max_rounds` have run.
    /// Returns the number of rounds run.
    pub fn run_until_idle(mut self: Pin<&mut Self>, max_rounds: u32) -> u32 {
        let mut rounds = 0;
        while rounds < max_rounds && self.ready.load(Ordering::Acquire) != 0 {
            self.as_mut().run_round();
            rounds += 1;
        }
        rounds
    }

    /// Polls exactly one task, slot `i`, if it is ready. Returns `Some(true)`
    /// if the task finished, `Some(false)` if it is still pending, and `None`
    /// if the slot is empty or not ready.
    ///
    /// This is for drivers, such as replayers, that choose the schedule themselves.
    pub fn poll_slot(self: Pin<&mut Self>, i: usize) -> Option<bool> {
        // SAFETY: as in `spawn`.
        let this = unsafe { self.get_unchecked_mut() };
        if i >= N
            || this.slots[i].is_none()
            || this.ready.fetch_and(!(1 << i), Ordering::AcqRel) & (1 << i) == 0
        {
            return None;
        }
        let wake: *const Wake = &this.wakes[i];
        // SAFETY: as in `run_round`.
        let waker = unsafe { Waker::from_raw(RawWaker::new(wake.cast(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let slot = this.slots[i].as_mut()?;
        let done = slot.fut.as_mut().poll(&mut cx).is_ready();
        if done {
            this.slots[i] = None;
        }
        this.stats.polls += 1;
        Some(done)
    }

    /// Marks slot `i` ready, as an interrupt handler would. Does nothing for an empty slot.
    pub fn wake_slot(&self, i: usize) {
        if i < N && self.slots[i].is_some() {
            self.ready.fetch_or(1 << i, Ordering::Release);
        }
    }

    /// The set of ready slots, one bit per slot.
    pub fn ready_mask(&self) -> u32 {
        self.ready.load(Ordering::Acquire)
    }

    /// The priority of slot `i`, or `None` if the slot is empty.
    pub fn priority(&self, i: usize) -> Option<u8> {
        self.slots.get(i)?.as_ref().map(|s| s.priority)
    }

    /// Number of tasks that have not finished.
    pub fn pending(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// Work counters for this executor.
    pub fn stats(&self) -> Stats {
        self.stats
    }
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(clone_waker, wake, wake_by_ref, drop_waker);

unsafe fn clone_waker(p: *const ()) -> RawWaker {
    RawWaker::new(p, &VTABLE)
}

unsafe fn wake(p: *const ()) {
    // SAFETY: the pointer came from `run_round` and refers to a live `Wake`.
    unsafe { wake_by_ref(p) }
}

unsafe fn wake_by_ref(p: *const ()) {
    // SAFETY: as above. Setting a bit is one atomic operation, so this is
    // safe to call from an interrupt handler.
    let w = unsafe { &*p.cast::<Wake>() };
    if !w.ready.is_null() {
        // SAFETY: `ready` points at the executor's ready word, which outlives the wake.
        unsafe { (*w.ready).fetch_or(w.bit, Ordering::Release) };
    }
}

unsafe fn drop_waker(_: *const ()) {}
