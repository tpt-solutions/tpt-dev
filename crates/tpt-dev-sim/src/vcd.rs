//! Waveform output in the Value Change Dump (VCD) format.
//!
//! The writer goes to any [`core::fmt::Write`], so no allocation is needed. It
//! writes a header from the kernel's signals, then one block per cycle that has
//! changes. Viewers such as GTKWave read the output.

use core::fmt::{self, Write};

use crate::kernel::Kernel;

/// Writes a kernel's signals as VCD.
///
/// Call [`Vcd::sample`] after each [`Kernel::step`] you want recorded. The first
/// sample is written when the writer is created.
pub struct Vcd<W, const N: usize> {
    out: W,
    last: [Option<u64>; N],
}

impl<W: Write, const N: usize> Vcd<W, N> {
    /// Writes the VCD header for `kernel`, under a module named `scope`, and the
    /// kernel's current values.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Kernel, Vcd};
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// k.signal("ready", 1, 0).unwrap();
    /// let vcd = Vcd::new(String::new(), &k, "top").unwrap();
    /// let text = vcd.into_inner();
    /// assert!(text.contains("$scope module top $end"));
    /// assert!(text.contains("$var wire 1 ! ready $end"));
    /// ```
    pub fn new(mut out: W, kernel: &Kernel<N>, scope: &str) -> Result<Self, fmt::Error> {
        writeln!(out, "$timescale 1ns $end")?;
        writeln!(out, "$scope module {scope} $end")?;
        for signal in kernel.signals() {
            let mut buf = [0u8; 8];
            let id = vcd_id(signal.index(), &mut buf);
            writeln!(
                out,
                "$var wire {} {} {} $end",
                kernel.width(signal),
                id,
                kernel.name(signal)
            )?;
        }
        writeln!(out, "$upscope $end")?;
        writeln!(out, "$enddefinitions $end")?;
        let mut vcd = Self {
            out,
            last: [None; N],
        };
        vcd.sample(kernel)?;
        Ok(vcd)
    }

    /// Writes the signals whose values changed since the last sample.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Kernel, Vcd};
    ///
    /// let mut k: Kernel<1> = Kernel::new();
    /// let ready = k.signal("ready", 1, 0).unwrap();
    /// let mut text = String::new();
    /// let mut vcd = Vcd::new(&mut text, &k, "top").unwrap();
    /// k.poke(ready, 1);
    /// vcd.sample(&k).unwrap();
    /// drop(vcd);
    /// // The changed value is the last line of the dump.
    /// assert_eq!(text.lines().last().unwrap().chars().next(), Some('1'));
    /// ```
    pub fn sample(&mut self, kernel: &Kernel<N>) -> fmt::Result {
        let mut stamped = false;
        for signal in kernel.signals() {
            let i = signal.index();
            let value = kernel.value(signal);
            if self.last[i] == Some(value) {
                continue;
            }
            if !stamped {
                writeln!(self.out, "#{}", kernel.cycle())?;
                stamped = true;
            }
            self.last[i] = Some(value);
            let mut buf = [0u8; 8];
            let id = vcd_id(i, &mut buf);
            let width = kernel.width(signal);
            if width == 1 {
                writeln!(self.out, "{}{}", value & 1, id)?;
            } else {
                write!(self.out, "b")?;
                for bit in (0..width).rev() {
                    write!(self.out, "{}", (value >> bit) & 1)?;
                }
                writeln!(self.out, " {id}")?;
            }
        }
        Ok(())
    }

    /// Returns the underlying writer.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_sim::{Kernel, Vcd};
    ///
    /// let k: Kernel<1> = Kernel::new();
    /// let vcd = Vcd::new(String::new(), &k, "top").unwrap();
    /// // Taking the writer back returns the text written so far.
    /// let text: String = vcd.into_inner();
    /// assert!(text.contains("$enddefinitions $end"));
    /// ```
    pub fn into_inner(self) -> W {
        self.out
    }
}

/// Encodes a signal index as a short VCD identifier made of printable ASCII.
fn vcd_id(mut index: usize, buf: &mut [u8; 8]) -> &str {
    let mut len = 0;
    loop {
        buf[len] = b'!' + (index % 94) as u8;
        len += 1;
        index /= 94;
        if index == 0 || len == buf.len() {
            break;
        }
    }
    core::str::from_utf8(&buf[..len]).unwrap_or("")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::kernel::{Component, Ctx, SignalId};
    use std::string::{String, ToString};

    struct Counter(SignalId);

    impl Component for Counter {
        fn tick(&mut self, ctx: &mut Ctx<'_>) {
            let next = ctx.read(self.0) + 1;
            ctx.write(self.0, next);
        }
    }

    #[test]
    fn header_and_changes_match_the_expected_dump() {
        let mut sim: Kernel<2> = Kernel::new();
        let a = sim.signal("a", 1, 0).unwrap();
        let b = sim.signal("b", 4, 0).unwrap();
        let mut counter = Counter(b);
        let mut vcd = Vcd::new(String::new(), &sim, "top").unwrap();
        for _ in 0..2 {
            sim.step(&mut [&mut counter]);
            vcd.sample(&sim).unwrap();
        }
        sim.poke(a, 1);
        vcd.sample(&sim).unwrap();
        let text = vcd.into_inner();
        let expected = concat!(
            "$timescale 1ns $end\n",
            "$scope module top $end\n",
            "$var wire 1 ! a $end\n",
            "$var wire 4 \" b $end\n",
            "$upscope $end\n",
            "$enddefinitions $end\n",
            "#0\n",
            "0!\n",
            "b0000 \"\n",
            "#1\n",
            "b0001 \"\n",
            "#2\n",
            "b0010 \"\n",
            "#2\n",
            "1!\n",
        );
        assert_eq!(text, expected);
    }

    #[test]
    fn identifiers_are_unique_past_one_character() {
        let mut buf = [0u8; 8];
        let first = vcd_id(0, &mut buf).to_string();
        let mut buf2 = [0u8; 8];
        let ninety_fifth = vcd_id(94, &mut buf2).to_string();
        assert_eq!(first, "!");
        assert_eq!(ninety_fifth.len(), 2);
        assert_ne!(first, ninety_fifth);
    }
}
