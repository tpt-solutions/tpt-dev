//! Timing checks, written as components that watch signals.
//!
//! A check does not change the design. It records a failure on the kernel when
//! the timing rule is broken.

use crate::kernel::{Component, Ctx, SignalId};

/// Expects `response` to be high within `within` cycles of each rising edge of
/// `trigger`.
///
/// A response in the trigger's own cycle is on time when `within` is zero. A
/// trigger that rises again before the response arrives does not start a new
/// window.
pub struct Deadline {
    trigger: SignalId,
    response: SignalId,
    within: u64,
    started: Option<u64>,
    was_triggered: bool,
}

impl Deadline {
    /// Checks that `response` follows each rising edge of `trigger` within `within` cycles.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Component, Deadline, Kernel};
    ///
    /// let mut k: Kernel<2> = Kernel::new();
    /// let trigger = k.signal("trigger", 1, 0).unwrap();
    /// let response = k.signal("response", 1, 0).unwrap();
    /// let mut deadline = Deadline::new(trigger, response, 2);
    /// // The trigger rises and no response follows, so the deadline passes at cycle 2.
    /// k.poke(trigger, 1);
    /// for _ in 0..3 {
    ///     k.step(&mut [&mut deadline]);
    /// }
    /// assert_eq!(k.failures(), 1);
    /// ```
    pub const fn new(trigger: SignalId, response: SignalId, within: u64) -> Self {
        Self {
            trigger,
            response,
            within,
            started: None,
            was_triggered: false,
        }
    }
}

impl Component for Deadline {
    fn tick(&mut self, ctx: &mut Ctx<'_>) {
        let now = ctx.cycle();
        let triggered = ctx.read(self.trigger) != 0;
        let responded = ctx.read(self.response) != 0;
        if triggered && !self.was_triggered && self.started.is_none() {
            self.started = Some(now);
        }
        self.was_triggered = triggered;
        if let Some(start) = self.started {
            if responded {
                self.started = None;
            } else if now - start >= self.within {
                ctx.fail("response missed its deadline");
                self.started = None;
            }
        }
    }
}

/// Expects rising edges of `signal` to be at least `gap` cycles apart.
///
/// Use it to check that a pulse train is not faster than its specification.
pub struct MinGap {
    signal: SignalId,
    gap: u64,
    last_rise: Option<u64>,
    was_high: bool,
}

impl MinGap {
    /// Checks that rising edges of `signal` are at least `gap` cycles apart.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Component, Kernel, MinGap};
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// let pulse = k.signal("pulse", 1, 0).unwrap();
    /// let mut check = MinGap::new(pulse, 3);
    /// // Rising edges at cycles 0 and 2 are only two apart, which is under the gap of three.
    /// k.poke(pulse, 1);
    /// k.step(&mut [&mut check]);
    /// k.poke(pulse, 0);
    /// k.step(&mut [&mut check]);
    /// k.poke(pulse, 1);
    /// k.step(&mut [&mut check]);
    /// assert_eq!(k.failures(), 1);
    /// ```
    pub const fn new(signal: SignalId, gap: u64) -> Self {
        Self {
            signal,
            gap,
            last_rise: None,
            was_high: false,
        }
    }
}

impl Component for MinGap {
    fn tick(&mut self, ctx: &mut Ctx<'_>) {
        let now = ctx.cycle();
        let high = ctx.read(self.signal) != 0;
        if high && !self.was_high {
            if let Some(last) = self.last_rise
                && now - last < self.gap
            {
                ctx.fail("pulses closer than the minimum gap");
            }
            self.last_rise = Some(now);
        }
        self.was_high = high;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::kernel::Kernel;

    /// Drives a signal high in the listed cycles, and low otherwise.
    struct Pattern {
        signal: SignalId,
        cycles: &'static [u64],
    }

    impl Component for Pattern {
        fn tick(&mut self, ctx: &mut Ctx<'_>) {
            let high = self.cycles.contains(&ctx.cycle());
            ctx.write(self.signal, u64::from(high));
        }
    }

    #[test]
    fn response_within_the_window_passes() {
        let mut sim: Kernel<2> = Kernel::new();
        let trig = sim.signal("trigger", 1, 0).unwrap();
        let resp = sim.signal("response", 1, 0).unwrap();
        let mut t = Pattern {
            signal: trig,
            cycles: &[0],
        };
        let mut r = Pattern {
            signal: resp,
            cycles: &[5],
        };
        let mut d = Deadline::new(trig, resp, 5);
        for _ in 0..10 {
            sim.step(&mut [&mut t, &mut r, &mut d]);
        }
        assert_eq!(sim.failures(), 0);
    }

    #[test]
    fn late_response_is_reported_when_the_window_closes() {
        let mut sim: Kernel<2> = Kernel::new();
        let trig = sim.signal("trigger", 1, 0).unwrap();
        let resp = sim.signal("response", 1, 0).unwrap();
        let mut t = Pattern {
            signal: trig,
            cycles: &[0],
        };
        let mut r = Pattern {
            signal: resp,
            cycles: &[5],
        };
        let mut d = Deadline::new(trig, resp, 3);
        for _ in 0..10 {
            sim.step(&mut [&mut t, &mut r, &mut d]);
        }
        assert_eq!(sim.failures(), 1);
        // The trigger is written in cycle 0 and seen from cycle 1. The window
        // is 3 cycles from that first observation, so the failure is at cycle 4.
        let f = sim.first_failure().unwrap();
        assert_eq!(f.cycle, 4);
        assert_eq!(f.message, "response missed its deadline");
    }

    #[test]
    fn pulses_closer_than_the_gap_are_reported() {
        let mut sim: Kernel<1> = Kernel::new();
        let irq = sim.signal("irq", 1, 0).unwrap();
        let mut p = Pattern {
            signal: irq,
            cycles: &[0, 3],
        };
        let mut gap = MinGap::new(irq, 5);
        for _ in 0..6 {
            sim.step(&mut [&mut p, &mut gap]);
        }
        assert_eq!(sim.failures(), 1);
        // The second pulse is written in cycle 3 and seen in cycle 4.
        assert_eq!(sim.first_failure().unwrap().cycle, 4);
    }

    #[test]
    fn pulses_at_the_gap_pass() {
        let mut sim: Kernel<1> = Kernel::new();
        let irq = sim.signal("irq", 1, 0).unwrap();
        let mut p = Pattern {
            signal: irq,
            cycles: &[0, 3],
        };
        let mut gap = MinGap::new(irq, 3);
        for _ in 0..6 {
            sim.step(&mut [&mut p, &mut gap]);
        }
        assert_eq!(sim.failures(), 0);
    }
}
