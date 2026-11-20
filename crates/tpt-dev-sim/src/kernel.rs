//! The signal kernel: signals, clock cycles, and the component contract.
//!
//! Each cycle has two phases. In the evaluate phase every component reads the
//! committed values from the previous cycle and writes next values. In the
//! commit phase the next values become current. A component therefore cannot
//! see another component's writes in the same cycle, so the result does not
//! depend on the order components are listed in.

use core::fmt;

/// Identifies a signal allocated on a [`Kernel`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SignalId(usize);

impl SignalId {
    /// The signal's index, in allocation order from zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::Kernel;
    ///
    /// let mut k: Kernel<2> = Kernel::new();
    /// let a = k.signal("a", 1, 0).unwrap();
    /// let b = k.signal("b", 8, 0).unwrap();
    /// assert_eq!(a.index(), 0);
    /// assert_eq!(b.index(), 1);
    /// ```
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Why a signal could not be allocated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimError {
    /// The kernel already holds all `N` of its signals.
    Full,
    /// The width is zero, or more than 64 bits.
    Width,
}

impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SimError::Full => f.write_str("the kernel has no free signal slots"),
            SimError::Width => f.write_str("a signal must be 1 to 64 bits wide"),
        }
    }
}

/// A failed check, as reported by a component or by the kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The cycle in which the failure was recorded.
    pub cycle: u64,
    /// The component that reported it, by its position in the list given to [`Kernel::step`].
    pub component: usize,
    /// What went wrong.
    pub message: &'static str,
}

const NO_DRIVER: usize = usize::MAX;

const fn mask(width: u8) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

/// What a component can reach while it ticks.
pub(crate) trait Bus {
    fn cycle(&self) -> u64;
    fn read(&self, signal: SignalId) -> u64;
    fn write(&mut self, component: usize, signal: SignalId, value: u64);
    fn fail(&mut self, component: usize, message: &'static str);
}

/// The view a component gets during one tick.
pub struct Ctx<'a> {
    bus: &'a mut dyn Bus,
    component: usize,
}

impl Ctx<'_> {
    /// The cycle being evaluated. The first cycle is zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Component, Ctx, Kernel, SignalId};
    ///
    /// struct CycleOut(SignalId);
    /// impl Component for CycleOut {
    ///     fn tick(&mut self, ctx: &mut Ctx<'_>) {
    ///         let cycle = ctx.cycle();
    ///         ctx.write(self.0, cycle);
    ///     }
    /// }
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// let out = k.signal("out", 64, 0).unwrap();
    /// let mut c = CycleOut(out);
    /// // Cycle 0 writes 0 and cycle 1 writes 1. The committed value is one cycle behind.
    /// k.step(&mut [&mut c]);
    /// k.step(&mut [&mut c]);
    /// assert_eq!(k.value(out), 1);
    /// ```
    pub fn cycle(&self) -> u64 {
        self.bus.cycle()
    }

    /// The committed value of `signal` from the previous cycle.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Component, Ctx, Kernel, SignalId};
    ///
    /// struct Forward {
    ///     from: SignalId,
    ///     to: SignalId,
    /// }
    /// impl Component for Forward {
    ///     fn tick(&mut self, ctx: &mut Ctx<'_>) {
    ///         let v = ctx.read(self.from);
    ///         ctx.write(self.to, v);
    ///     }
    /// }
    ///
    /// let mut k: Kernel<2> = Kernel::new();
    /// let from = k.signal("from", 8, 5).unwrap();
    /// let to = k.signal("to", 8, 0).unwrap();
    /// let mut f = Forward { from, to };
    /// k.step(&mut [&mut f]);
    /// assert_eq!(k.value(to), 5);
    /// ```
    pub fn read(&self, signal: SignalId) -> u64 {
        self.bus.read(signal)
    }

    /// Sets the value `signal` takes at the end of this cycle. Values wider than
    /// the signal are truncated. A signal that is not written keeps its value.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Component, Ctx, Kernel, SignalId};
    ///
    /// struct Forward {
    ///     from: SignalId,
    ///     to: SignalId,
    /// }
    /// impl Component for Forward {
    ///     fn tick(&mut self, ctx: &mut Ctx<'_>) {
    ///         let v = ctx.read(self.from);
    ///         ctx.write(self.to, v);
    ///     }
    /// }
    ///
    /// let mut k: Kernel<2> = Kernel::new();
    /// let from = k.signal("from", 8, 0xAB).unwrap();
    /// // The destination is 4 bits wide, so the written value is truncated to 0xB.
    /// let to = k.signal("to", 4, 0).unwrap();
    /// let mut f = Forward { from, to };
    /// k.step(&mut [&mut f]);
    /// assert_eq!(k.value(to), 0xB);
    /// ```
    pub fn write(&mut self, signal: SignalId, value: u64) {
        self.bus.write(self.component, signal, value);
    }

    /// Records a failed check at this cycle.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Component, Ctx, Kernel, SignalId};
    ///
    /// struct Limit(SignalId);
    /// impl Component for Limit {
    ///     fn tick(&mut self, ctx: &mut Ctx<'_>) {
    ///         if ctx.read(self.0) > 3 {
    ///             ctx.fail("too high");
    ///         }
    ///     }
    /// }
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// let level = k.signal("level", 8, 0).unwrap();
    /// let mut limit = Limit(level);
    /// k.poke(level, 9);
    /// k.step(&mut [&mut limit]);
    /// assert_eq!(k.failures(), 1);
    /// ```
    pub fn fail(&mut self, message: &'static str) {
        self.bus.fail(self.component, message);
    }
}

/// Something that changes state once per cycle.
///
/// A component reads signals with [`Ctx::read`] and drives signals with
/// [`Ctx::write`]. It must not keep state that depends on another component's
/// writes in the same cycle, because it cannot observe them.
pub trait Component {
    /// Evaluates one cycle.
    fn tick(&mut self, ctx: &mut Ctx<'_>);
}

/// A simulation with room for `N` signals. It allocates nothing.
///
/// The kernel is single-clock. Signals are up to 64 bits wide.
pub struct Kernel<const N: usize> {
    cycle: u64,
    count: usize,
    names: [&'static str; N],
    widths: [u8; N],
    current: [u64; N],
    next: [u64; N],
    driver: [usize; N],
    failures: u64,
    first: Option<Failure>,
}

impl<const N: usize> Default for Kernel<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Kernel<N> {
    /// An empty kernel at cycle zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::Kernel;
    ///
    /// let k: Kernel<4> = Kernel::new();
    /// assert_eq!(k.cycle(), 0);
    /// assert_eq!(k.signals().count(), 0);
    /// ```
    pub const fn new() -> Self {
        Self {
            cycle: 0,
            count: 0,
            names: [""; N],
            widths: [0; N],
            current: [0; N],
            next: [0; N],
            driver: [NO_DRIVER; N],
            failures: 0,
            first: None,
        }
    }

    /// Allocates a signal of `width` bits, starting at `init`.
    ///
    /// The name is used in waveform output and should be a plain identifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Kernel, SimError};
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// let ready = k.signal("ready", 1, 1).unwrap();
    /// assert_eq!(k.value(ready), 1);
    /// // The kernel has one slot, now used up.
    /// assert_eq!(k.signal("extra", 1, 0), Err(SimError::Full));
    /// ```
    pub fn signal(
        &mut self,
        name: &'static str,
        width: u8,
        init: u64,
    ) -> Result<SignalId, SimError> {
        if self.count == N {
            return Err(SimError::Full);
        }
        if width == 0 || width > 64 {
            return Err(SimError::Width);
        }
        let i = self.count;
        self.count += 1;
        let value = init & mask(width);
        self.names[i] = name;
        self.widths[i] = width;
        self.current[i] = value;
        self.next[i] = value;
        Ok(SignalId(i))
    }

    /// The committed value of `signal`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::Kernel;
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// // The initial value is masked to the 4-bit width.
    /// let nibble = k.signal("nibble", 4, 0x1F).unwrap();
    /// assert_eq!(k.value(nibble), 0xF);
    /// ```
    pub fn value(&self, signal: SignalId) -> u64 {
        self.current[signal.0]
    }

    /// Sets `signal` between cycles, for stimulus from the host. The new value is
    /// visible to the next cycle.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::Kernel;
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// let nibble = k.signal("nibble", 4, 0).unwrap();
    /// k.poke(nibble, 0x1F);
    /// assert_eq!(k.value(nibble), 0xF);
    /// ```
    pub fn poke(&mut self, signal: SignalId, value: u64) {
        let v = value & mask(self.widths[signal.0]);
        self.current[signal.0] = v;
        self.next[signal.0] = v;
    }

    /// The number of completed cycles.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Component, Kernel};
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// let mut none: [&mut dyn Component; 0] = [];
    /// k.step(&mut none);
    /// assert_eq!(k.cycle(), 1);
    /// ```
    pub fn cycle(&self) -> u64 {
        self.cycle
    }

    /// The signals allocated so far, in allocation order.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Kernel, SignalId};
    ///
    /// let mut k: Kernel<3> = Kernel::new();
    /// k.signal("a", 1, 0).unwrap();
    /// k.signal("b", 2, 0).unwrap();
    /// let ids: Vec<SignalId> = k.signals().collect();
    /// assert_eq!(ids.len(), 2);
    /// ```
    pub fn signals(&self) -> impl Iterator<Item = SignalId> {
        (0..self.count).map(SignalId)
    }

    /// The name given to `signal`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::Kernel;
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// let ready = k.signal("ready", 1, 0).unwrap();
    /// assert_eq!(k.name(ready), "ready");
    /// ```
    pub fn name(&self, signal: SignalId) -> &'static str {
        self.names[signal.0]
    }

    /// The width of `signal`, in bits.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::Kernel;
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// let count = k.signal("count", 32, 0).unwrap();
    /// assert_eq!(k.width(count), 32);
    /// ```
    pub fn width(&self, signal: SignalId) -> u8 {
        self.widths[signal.0]
    }

    /// The number of failures recorded so far.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Component, Ctx, Kernel, SignalId};
    ///
    /// struct Limit(SignalId);
    /// impl Component for Limit {
    ///     fn tick(&mut self, ctx: &mut Ctx<'_>) {
    ///         if ctx.read(self.0) > 3 {
    ///             ctx.fail("too high");
    ///         }
    ///     }
    /// }
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// let level = k.signal("level", 8, 0).unwrap();
    /// let mut limit = Limit(level);
    /// assert_eq!(k.failures(), 0);
    /// k.poke(level, 9);
    /// k.step(&mut [&mut limit]);
    /// assert_eq!(k.failures(), 1);
    /// ```
    pub fn failures(&self) -> u64 {
        self.failures
    }

    /// The first failure recorded, if any.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Component, Ctx, Kernel, SignalId};
    ///
    /// struct Limit(SignalId);
    /// impl Component for Limit {
    ///     fn tick(&mut self, ctx: &mut Ctx<'_>) {
    ///         if ctx.read(self.0) > 3 {
    ///             ctx.fail("too high");
    ///         }
    ///     }
    /// }
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// let level = k.signal("level", 8, 0).unwrap();
    /// let mut limit = Limit(level);
    /// assert!(k.first_failure().is_none());
    /// k.poke(level, 9);
    /// k.step(&mut [&mut limit]);
    /// let failure = k.first_failure().unwrap();
    /// assert_eq!(failure.message, "too high");
    /// assert_eq!(failure.component, 0);
    /// assert_eq!(failure.cycle, 0);
    /// ```
    pub fn first_failure(&self) -> Option<Failure> {
        self.first
    }

    /// Runs one cycle: every component ticks, then the values are committed.
    ///
    /// Two components that write the same signal in one cycle record a failure.
    /// The later write wins, so the result is still deterministic.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Component, Ctx, Kernel, SignalId};
    ///
    /// struct Forward {
    ///     from: SignalId,
    ///     to: SignalId,
    /// }
    /// impl Component for Forward {
    ///     fn tick(&mut self, ctx: &mut Ctx<'_>) {
    ///         let v = ctx.read(self.from);
    ///         ctx.write(self.to, v);
    ///     }
    /// }
    ///
    /// let mut k: Kernel<2> = Kernel::new();
    /// let a = k.signal("a", 8, 1).unwrap();
    /// let b = k.signal("b", 8, 0).unwrap();
    /// let mut f = Forward { from: a, to: b };
    /// // Each step moves the value one signal along.
    /// k.step(&mut [&mut f]);
    /// assert_eq!(k.value(b), 1);
    /// k.poke(a, 7);
    /// k.step(&mut [&mut f]);
    /// assert_eq!(k.value(b), 7);
    /// assert_eq!(k.cycle(), 2);
    /// ```
    pub fn step(&mut self, components: &mut [&mut dyn Component]) {
        let n = self.count;
        self.next[..n].copy_from_slice(&self.current[..n]);
        self.driver[..n].fill(NO_DRIVER);
        for (index, component) in components.iter_mut().enumerate() {
            let mut ctx = Ctx {
                bus: &mut *self,
                component: index,
            };
            component.tick(&mut ctx);
        }
        self.current[..n].copy_from_slice(&self.next[..n]);
        self.cycle += 1;
    }

    fn record(&mut self, component: usize, message: &'static str) {
        self.failures += 1;
        if self.first.is_none() {
            self.first = Some(Failure {
                cycle: self.cycle,
                component,
                message,
            });
        }
    }
}

impl<const N: usize> Bus for Kernel<N> {
    fn cycle(&self) -> u64 {
        self.cycle
    }

    fn read(&self, signal: SignalId) -> u64 {
        self.current[signal.0]
    }

    fn write(&mut self, component: usize, signal: SignalId, value: u64) {
        let i = signal.0;
        match self.driver[i] {
            NO_DRIVER => self.driver[i] = component,
            owner if owner != component => {
                self.record(component, "two components drive the same signal");
            }
            _ => {}
        }
        self.next[i] = value & mask(self.widths[i]);
    }

    fn fail(&mut self, component: usize, message: &'static str) {
        self.record(component, message);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Adds one to a signal every cycle.
    struct Counter(SignalId);

    impl Component for Counter {
        fn tick(&mut self, ctx: &mut Ctx<'_>) {
            let next = ctx.read(self.0) + 1;
            ctx.write(self.0, next);
        }
    }

    /// Copies one signal into another.
    struct Copy {
        from: SignalId,
        to: SignalId,
    }

    impl Component for Copy {
        fn tick(&mut self, ctx: &mut Ctx<'_>) {
            let v = ctx.read(self.from);
            ctx.write(self.to, v);
        }
    }

    #[test]
    fn counter_advances_once_per_cycle() {
        let mut sim: Kernel<2> = Kernel::new();
        let s = sim.signal("count", 8, 0).unwrap();
        let mut c = Counter(s);
        for _ in 0..3 {
            sim.step(&mut [&mut c]);
        }
        assert_eq!(sim.value(s), 3);
        assert_eq!(sim.cycle(), 3);
    }

    #[test]
    fn writes_are_truncated_to_the_width() {
        let mut sim: Kernel<1> = Kernel::new();
        let s = sim.signal("nibble", 4, 15).unwrap();
        let mut c = Counter(s);
        sim.step(&mut [&mut c]);
        assert_eq!(sim.value(s), 0);
        assert_eq!(sim.failures(), 0);
    }

    #[test]
    fn reads_see_the_previous_cycle_so_swaps_work() {
        // Each run lists the components in a different order. Both must give the
        // same result, because reads see only committed values.
        let run = |reversed: bool| {
            let mut sim: Kernel<2> = Kernel::new();
            let a = sim.signal("a", 8, 1).unwrap();
            let b = sim.signal("b", 8, 2).unwrap();
            let mut ab = Copy { from: a, to: b };
            let mut ba = Copy { from: b, to: a };
            if reversed {
                sim.step(&mut [&mut ba, &mut ab]);
            } else {
                sim.step(&mut [&mut ab, &mut ba]);
            }
            (sim.value(a), sim.value(b))
        };
        assert_eq!(run(false), (2, 1));
        assert_eq!(run(false), run(true));
    }

    #[test]
    fn two_drivers_on_one_signal_are_reported() {
        let mut sim: Kernel<1> = Kernel::new();
        let s = sim.signal("contended", 8, 0).unwrap();
        let mut a = Counter(s);
        let mut b = Counter(s);
        sim.step(&mut [&mut a, &mut b]);
        assert_eq!(sim.failures(), 1);
        let f = sim.first_failure().unwrap();
        assert_eq!(f.component, 1);
        assert_eq!(f.cycle, 0);
    }

    #[test]
    fn poke_is_visible_to_the_next_cycle() {
        let mut sim: Kernel<1> = Kernel::new();
        let s = sim.signal("count", 8, 0).unwrap();
        sim.poke(s, 41);
        let mut c = Counter(s);
        sim.step(&mut [&mut c]);
        assert_eq!(sim.value(s), 42);
    }

    #[test]
    fn allocation_errors_are_reported() {
        let mut sim: Kernel<1> = Kernel::new();
        assert_eq!(sim.signal("zero", 0, 0), Err(SimError::Width));
        assert_eq!(sim.signal("wide", 65, 0), Err(SimError::Width));
        sim.signal("one", 1, 0).unwrap();
        assert_eq!(sim.signal("two", 1, 0), Err(SimError::Full));
    }

    #[test]
    fn a_64_bit_signal_keeps_every_bit() {
        let mut sim: Kernel<1> = Kernel::new();
        let s = sim.signal("wide", 64, u64::MAX - 1).unwrap();
        assert_eq!(sim.value(s), u64::MAX - 1);
    }
}
