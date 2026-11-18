//! Recording and replaying a run of an executor.
//!
//! A [`Session`] is shared by the driver ([`step`]) and the tasks. The driver
//! chooses interrupts and scheduling decisions, and tasks read inputs through
//! [`Session::input`]. Each decision and each input is appended to the log.
//! A replay session reads the same decisions and inputs back from a recorded
//! log, and it checks that the program asks for them in the same order.

use core::cell::RefCell;
use core::pin::Pin;

use tpt_dev_async::Executor;

use crate::event::Event;
use crate::log::Log;

/// The replay did not match the recording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Divergence {
    /// Index in the recorded log where the mismatch was found.
    pub at: usize,
    /// What the replay needed at that point.
    pub expected: &'static str,
}

/// What the driver did in one step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    /// Task `slot` was polled. `finished` says whether it completed.
    Polled {
        /// The slot polled.
        slot: usize,
        /// Whether the task finished.
        finished: bool,
    },
    /// No task is ready, or the recording has no more steps.
    Idle,
}

#[derive(Clone, Copy)]
enum Action {
    Interrupt(usize),
    Schedule(usize),
    Idle,
}

/// The state of one recording or replay.
pub struct Session<const N: usize> {
    log: Log<N>,
    script: Option<Log<N>>,
    cursor: usize,
    rng: u64,
    interrupt_permille: u32,
}

impl<const N: usize> Session<N> {
    /// Records a new run. The seed determines the inputs and the interrupts.
    pub const fn recording(seed: u64) -> Self {
        Self {
            log: Log::new(),
            script: None,
            cursor: 0,
            // A zero state would make the generator stick at zero.
            rng: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
            interrupt_permille: 0,
        }
    }

    /// Replays a recorded run. The program must ask for the same inputs in the same order.
    pub const fn replaying(script: Log<N>) -> Self {
        Self {
            log: Log::new(),
            script: Some(script),
            cursor: 0,
            rng: 1,
            interrupt_permille: 0,
        }
    }

    /// Chance, in thousandths, that the driver fires an interrupt before a step (recording only).
    pub fn set_interrupt_permille(&mut self, permille: u32) {
        self.interrupt_permille = permille.min(1000);
    }

    /// The events so far. A replay produces the same log as the recording it followed.
    pub fn log(&self) -> &Log<N> {
        &self.log
    }

    /// Reads input `channel`. A recording draws a fresh value. A replay returns the logged one.
    pub fn input(&mut self, channel: u8) -> Result<u64, Divergence> {
        let value = match self.script {
            None => self.next_rng(),
            Some(script) => match script.as_slice().get(self.cursor) {
                Some(Event::Input { channel: c, value }) if *c == channel => {
                    let v = *value;
                    self.cursor += 1;
                    v
                }
                _ => {
                    return Err(Divergence {
                        at: self.cursor,
                        expected: "an input on the requested channel",
                    });
                }
            },
        };
        self.record(Event::Input { channel, value })?;
        Ok(value)
    }

    fn record(&mut self, e: Event) -> Result<(), Divergence> {
        self.log.push(e).map_err(|_| Divergence {
            at: self.log.len(),
            expected: "room in the log",
        })
    }

    fn next_rng(&mut self) -> u64 {
        // xorshift64
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    /// Decides the next driver action, recording it.
    fn next_action<const S: usize>(
        &mut self,
        exec: &Executor<'_, S>,
    ) -> Result<Action, Divergence> {
        if self.script.is_some() {
            return self.replay_action();
        }
        // Recording: maybe an interrupt first.
        let occupied: u32 = (0..S)
            .filter(|&i| exec.priority(i).is_some())
            .fold(0, |m, i| m | (1 << i));
        if occupied != 0
            && u32::try_from(self.next_rng() % 1000).unwrap_or(0) < self.interrupt_permille
        {
            let count = occupied.count_ones();
            let pick = (self.next_rng() % u64::from(count)) as u32;
            let slot = nth_set_bit(occupied, pick);
            self.record(Event::Interrupt { slot: slot as u8 })?;
            return Ok(Action::Interrupt(slot));
        }
        let ready = exec.ready_mask();
        if ready == 0 {
            return Ok(Action::Idle);
        }
        // Highest priority ready slot; the lowest index breaks ties.
        let mut best: Option<(usize, u8)> = None;
        for i in 0..S {
            if ready & (1 << i) == 0 {
                continue;
            }
            if let Some(p) = exec.priority(i)
                && best.is_none_or(|(_, bp)| p > bp)
            {
                best = Some((i, p));
            }
        }
        match best {
            Some((slot, _)) => {
                self.record(Event::Schedule { slot: slot as u8 })?;
                Ok(Action::Schedule(slot))
            }
            None => Ok(Action::Idle),
        }
    }

    fn replay_action(&mut self) -> Result<Action, Divergence> {
        let Some(script) = self.script else {
            return Ok(Action::Idle);
        };
        let Some(&event) = script.as_slice().get(self.cursor) else {
            return Ok(Action::Idle);
        };
        match event {
            Event::Interrupt { slot } => {
                self.cursor += 1;
                self.record(event)?;
                Ok(Action::Interrupt(usize::from(slot)))
            }
            Event::Schedule { slot } => {
                self.cursor += 1;
                self.record(event)?;
                Ok(Action::Schedule(usize::from(slot)))
            }
            Event::Input { .. } => Err(Divergence {
                at: self.cursor,
                expected: "an interrupt or a schedule step",
            }),
        }
    }
}

fn nth_set_bit(mask: u32, n: u32) -> usize {
    let mut seen = 0;
    for i in 0..32 {
        if mask & (1 << i) != 0 {
            if seen == n {
                return i;
            }
            seen += 1;
        }
    }
    0
}

/// Runs one driver step: applies any interrupts, then polls one ready task.
///
/// In a replay, the decisions come from the log, and a mismatch is a [`Divergence`].
/// A recording makes its own choices and logs them. Tasks must not hold a
/// borrow of the session across a poll, because the driver borrows it too.
pub fn step<const N: usize, const S: usize>(
    session: &RefCell<Session<N>>,
    exec: Pin<&mut Executor<'_, S>>,
) -> Result<Progress, Divergence> {
    let mut exec = exec;
    let slot = loop {
        let action = {
            let mut s = session.borrow_mut();
            s.next_action(exec.as_ref().get_ref())?
        };
        match action {
            Action::Interrupt(slot) => exec.as_ref().wake_slot(slot),
            Action::Schedule(slot) => break slot,
            Action::Idle => return Ok(Progress::Idle),
        }
    };
    match exec.as_mut().poll_slot(slot) {
        Some(finished) => Ok(Progress::Polled { slot, finished }),
        None => Err(Divergence {
            at: session.borrow().log().len(),
            expected: "a ready task in the scheduled slot",
        }),
    }
}
