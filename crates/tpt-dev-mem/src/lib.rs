//! Typed, zero-cost access to memory-mapped hardware registers.
//!
//! The crate is built on three ideas:
//!
//! * [`Word`] is the register width (`u8`, `u16`, `u32` or `u64`).
//! * The access mode is a type parameter, so writing to a read-only register
//!   is a compile error. See [`RO`], [`WO`], [`RW`] and [`W1C`].
//! * [`Field`] describes a bit field with const generics, so its mask is
//!   computed at compile time.
//!
//! All hardware access uses volatile reads and writes.
//!
//! # Safety model
//!
//! Creating a [`Reg`] (or a block made with [`register_block!`]) is `unsafe`.
//! The caller promises that the address is a valid, aligned register for the
//! whole lifetime of the value, and that no other code accesses it in a way
//! that breaks the program's assumptions. Once that holds, reads and writes
//! through the typed API are safe.

#![no_std]
#![doc = include_str!("../README.md")]
// The register primitives need raw volatile access. Every `unsafe` block below
// has a `SAFETY` comment that refers to the contract of `Reg::from_addr`.
#![allow(unsafe_code)]

#[cfg(any(test, feature = "std"))]
extern crate std;

use core::marker::PhantomData;
use core::ops::{BitAnd, BitOr, Not, Shl, Shr};
use core::ptr;

mod sealed {
    pub trait Sealed {}
}

/// An unsigned integer type that can be used as a register width.
///
/// Implemented for `u8`, `u16`, `u32` and `u64`. The trait is sealed.
pub trait Word:
    Copy
    + Eq
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
    + Not<Output = Self>
    + Shl<u32, Output = Self>
    + Shr<u32, Output = Self>
    + sealed::Sealed
{
    /// Width of the type in bits.
    const BITS: u32;
    /// All bits clear.
    const ZERO: Self;
    /// Only the least significant bit set.
    const ONE: Self;
    /// All bits set.
    const ALL_ONES: Self;
}

macro_rules! impl_word {
    ($($t:ty),*) => {$(
        impl sealed::Sealed for $t {}
        impl Word for $t {
            const BITS: u32 = <$t>::BITS;
            const ZERO: Self = 0;
            const ONE: Self = 1;
            const ALL_ONES: Self = <$t>::MAX;
        }
    )*};
}
impl_word!(u8, u16, u32, u64);

/// Marker trait for access modes that allow reads.
pub trait Readable: sealed::Sealed {}
/// Marker trait for access modes that allow writes.
pub trait Writable: sealed::Sealed {}

/// Read-only register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RO;
/// Write-only register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WO;
/// Read-write register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RW;
/// Write-1-to-clear register: a write of `1` clears the matching bit, and a
/// write of `0` leaves it unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct W1C;

impl sealed::Sealed for RO {}
impl sealed::Sealed for WO {}
impl sealed::Sealed for RW {}
impl sealed::Sealed for W1C {}

impl Readable for RO {}
impl Readable for RW {}
impl Readable for W1C {}

impl Writable for WO {}
impl Writable for RW {}
impl Writable for W1C {}

/// A bit field of `WIDTH` bits starting at bit `OFFSET`.
///
/// Masks and shifts come from const generics, so they are constants at
/// compile time. Use it with [`Reg::read_field`] and [`Reg::modify_field`],
/// or call [`Field::get`] and [`Field::set`] on a value directly.
///
/// ```
/// use tpt_dev_mem::Field;
///
/// const BAUD: Field<4, 3> = Field::new();
/// assert_eq!(BAUD.get(0b0101_0000u32), 0b101);
/// assert_eq!(BAUD.set(0u32, 0b011), 0b0011_0000);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field<const OFFSET: u32, const WIDTH: u32>;

impl<const OFFSET: u32, const WIDTH: u32> Default for Field<OFFSET, WIDTH> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const OFFSET: u32, const WIDTH: u32> Field<OFFSET, WIDTH> {
    /// Creates the field. Fails to compile if the field is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::Field;
    ///
    /// let field: Field<0, 8> = Field::new();
    /// assert_eq!(field.get(0x1234u32), 0x34);
    /// ```
    pub const fn new() -> Self {
        const { assert!(WIDTH > 0, "a field must be at least one bit wide") };
        Self
    }

    /// The field mask, shifted into place, for register type `W`.
    ///
    /// Fails to compile if the field does not fit in `W`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::Field;
    ///
    /// assert_eq!(Field::<4, 3>::new().mask::<u32>(), 0b111_0000);
    /// ```
    pub fn mask<W: Word>(self) -> W {
        Self::check::<W>();
        Self::low_mask::<W>() << OFFSET
    }

    /// Extracts the field value from `reg`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::Field;
    ///
    /// assert_eq!(Field::<4, 3>::new().get(0b0101_0000u32), 0b101);
    /// ```
    pub fn get<W: Word>(self, reg: W) -> W {
        Self::check::<W>();
        (reg >> OFFSET) & Self::low_mask::<W>()
    }

    /// Returns `reg` with the field replaced by `value`.
    ///
    /// Bits of `value` above the field width are discarded.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::Field;
    ///
    /// assert_eq!(Field::<4, 3>::new().set(0u32, 0b011), 0b0011_0000);
    /// // Bits of the value above the field width are dropped.
    /// assert_eq!(Field::<0, 2>::new().set(0u8, 0b111), 0b11);
    /// ```
    pub fn set<W: Word>(self, reg: W, value: W) -> W {
        Self::check::<W>();
        let mask = Self::low_mask::<W>() << OFFSET;
        (reg & !mask) | ((value & Self::low_mask::<W>()) << OFFSET)
    }

    fn low_mask<W: Word>() -> W {
        W::ALL_ONES >> (W::BITS - WIDTH)
    }

    fn check<W: Word>() {
        const { assert!(WIDTH > 0) };
        assert!(
            OFFSET + WIDTH <= W::BITS,
            "field does not fit in the register"
        );
    }
}

/// A typed handle to one memory-mapped register of width `W` and access mode `A`.
///
/// The handle is a single pointer, so it has no runtime overhead beyond the
/// volatile access itself. Use [`register_block!`] to declare a peripheral's
/// registers.
///
/// Writing to a read-only register does not compile:
///
/// ```compile_fail
/// use tpt_dev_mem::{Reg, RO};
///
/// let reg: Reg<u32, RO> = unsafe { Reg::from_addr(0x1000) };
/// reg.write(1);
/// ```
pub struct Reg<W: Word, A> {
    addr: *mut W,
    _access: PhantomData<(W, A)>,
}

impl<W: Word, A> Clone for Reg<W, A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<W: Word, A> Copy for Reg<W, A> {}

impl<W: Word, A> core::fmt::Debug for Reg<W, A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Reg({:#x})", self.addr as usize)
    }
}

impl<W: Word, A> Reg<W, A> {
    /// Creates a register handle at `addr`.
    ///
    /// # Safety
    ///
    /// For the whole lifetime of the handle and all copies of it:
    ///
    /// * `addr` must be valid for volatile reads and writes of `W`, and must
    ///   be aligned for `W`.
    /// * The memory must stay mapped and must not be used as ordinary memory.
    /// * Accesses must not race with other accesses to the same register
    ///   that the program has not synchronised.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::{Reg, RW};
    ///
    /// let mut word: u32 = 0;
    /// let addr = &mut word as *mut u32 as usize;
    /// // SAFETY: `word` is a live, aligned u32 that nothing else touches in this example.
    /// let reg: Reg<u32, RW> = unsafe { Reg::from_addr(addr) };
    /// reg.write(7);
    /// assert_eq!(reg.read(), 7);
    /// ```
    pub unsafe fn from_addr(addr: usize) -> Self {
        debug_assert!(
            addr.is_multiple_of(core::mem::align_of::<W>()),
            "unaligned register address"
        );
        Self {
            addr: addr as *mut W,
            _access: PhantomData,
        }
    }

    /// The address of the register.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::{Reg, RW};
    ///
    /// let mut word: u32 = 0;
    /// let addr = &mut word as *mut u32 as usize;
    /// // SAFETY: `word` is a live, aligned u32 that nothing else touches in this example.
    /// let reg: Reg<u32, RW> = unsafe { Reg::from_addr(addr) };
    /// assert_eq!(reg.address(), addr);
    /// ```
    pub fn address(&self) -> usize {
        self.addr as usize
    }
}

impl<W: Word, A: Readable> Reg<W, A> {
    /// Reads the register with a volatile load.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::{Reg, RO};
    ///
    /// let mut word: u32 = 5;
    /// let addr = &mut word as *mut u32 as usize;
    /// // SAFETY: `word` is a live, aligned u32 that nothing else touches in this example.
    /// let reg: Reg<u32, RO> = unsafe { Reg::from_addr(addr) };
    /// assert_eq!(reg.read(), 5);
    /// ```
    pub fn read(&self) -> W {
        // SAFETY: `from_addr` guarantees that `addr` is valid and aligned for `W`.
        unsafe { ptr::read_volatile(self.addr) }
    }

    /// Reads the field `F` from the register.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::{Field, Reg, RO};
    ///
    /// let mut word: u32 = 0b0101_0000;
    /// let addr = &mut word as *mut u32 as usize;
    /// // SAFETY: `word` is a live, aligned u32 that nothing else touches in this example.
    /// let reg: Reg<u32, RO> = unsafe { Reg::from_addr(addr) };
    /// assert_eq!(reg.read_field(Field::<4, 3>::new()), 0b101);
    /// ```
    pub fn read_field<const OFFSET: u32, const WIDTH: u32>(
        &self,
        field: Field<OFFSET, WIDTH>,
    ) -> W {
        field.get(self.read())
    }
}

impl<W: Word, A: Writable> Reg<W, A> {
    /// Writes `value` to the register with a volatile store.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::{Reg, RW};
    ///
    /// let mut word: u32 = 0;
    /// let addr = &mut word as *mut u32 as usize;
    /// // SAFETY: `word` is a live, aligned u32 that nothing else touches in this example.
    /// let reg: Reg<u32, RW> = unsafe { Reg::from_addr(addr) };
    /// reg.write(0xAB);
    /// assert_eq!(reg.read(), 0xAB);
    /// ```
    pub fn write(&self, value: W) {
        // SAFETY: `from_addr` guarantees that `addr` is valid and aligned for `W`.
        unsafe { ptr::write_volatile(self.addr, value) }
    }
}

impl<W: Word, A: Readable + Writable> Reg<W, A> {
    /// Reads the register, applies `f`, and writes the result back.
    ///
    /// This is a read-modify-write and is not atomic with respect to other
    /// masters on the bus.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::{Reg, RW};
    ///
    /// let mut word: u32 = 0b001;
    /// let addr = &mut word as *mut u32 as usize;
    /// // SAFETY: `word` is a live, aligned u32 that nothing else touches in this example.
    /// let reg: Reg<u32, RW> = unsafe { Reg::from_addr(addr) };
    /// reg.modify(|v| v | 0b100);
    /// assert_eq!(reg.read(), 0b101);
    /// ```
    pub fn modify(&self, f: impl FnOnce(W) -> W) {
        self.write(f(self.read()));
    }

    /// Replaces the field `F` in the register with `value`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::{Field, Reg, RW};
    ///
    /// let mut word: u32 = 0;
    /// let addr = &mut word as *mut u32 as usize;
    /// // SAFETY: `word` is a live, aligned u32 that nothing else touches in this example.
    /// let reg: Reg<u32, RW> = unsafe { Reg::from_addr(addr) };
    /// reg.modify_field(Field::<4, 3>::new(), 0b011);
    /// assert_eq!(reg.read(), 0b0011_0000);
    /// ```
    pub fn modify_field<const OFFSET: u32, const WIDTH: u32>(
        &self,
        field: Field<OFFSET, WIDTH>,
        value: W,
    ) {
        self.modify(|reg| field.set(reg, value));
    }
}

impl<W: Word> Reg<W, W1C> {
    /// Clears the bits set in `mask`. Bits that are `0` in `mask` are left unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_mem::{Reg, W1C};
    ///
    /// let mut word: u32 = 0b1111;
    /// let addr = &mut word as *mut u32 as usize;
    /// // SAFETY: `word` is a live, aligned u32 that nothing else touches in this example.
    /// let reg: Reg<u32, W1C> = unsafe { Reg::from_addr(addr) };
    /// reg.clear_bits(0b0101);
    /// // The hardware clears bits 0 and 2. Plain host memory keeps the written mask.
    /// assert_eq!(reg.read(), 0b0101);
    /// ```
    pub fn clear_bits(&self, mask: W) {
        self.write(mask);
    }
}

/// Declares a peripheral's register block as a typed struct.
///
/// Each register is a method that returns a [`Reg`]. The block itself is
/// created with an `unsafe` constructor that takes the base address. That
/// call is the one place where the caller states the safety contract.
///
/// ```
/// use tpt_dev_mem::{register_block, RO, RW};
///
/// register_block! {
///     /// A small UART register block.
///     pub struct Uart {
///         0x00 => pub data: u32, RW;
///         0x04 => pub status: u32, RO;
///     }
/// }
///
/// // Backed by ordinary memory here, so the example runs on the host.
/// let mut mem = [0u32; 2];
/// let uart = unsafe { Uart::new(mem.as_mut_ptr() as usize) };
/// uart.data().write(0x41);
/// assert_eq!(uart.data().read(), 0x41);
/// ```
///
/// The generated constructor is `unsafe`. Its contract is the same as
/// [`Reg::from_addr`] for every register in the block.
#[macro_export]
macro_rules! register_block {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$fmeta:meta])*
                $offset:literal => $fvis:vis $field:ident : $word:ty, $access:ty ;
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $vis struct $name {
            base: usize,
        }

        impl $name {
            /// Creates the block at `base`.
            ///
            /// # Safety
            ///
            /// `base` plus every register offset in the block must satisfy the
            /// contract of [`Reg::from_addr`] for as long as the block is used.
            pub const unsafe fn new(base: usize) -> Self {
                Self { base }
            }

            /// The base address of the block.
            pub const fn base(&self) -> usize {
                self.base
            }

            /// Builds the handle for the register at `offset` from the base.
            #[doc(hidden)]
            pub fn __reg_at<W: $crate::Word, A>(&self, offset: usize) -> $crate::Reg<W, A> {
                // SAFETY: `new` required the block, and therefore every offset
                // the block's accessors pass in, to satisfy `Reg::from_addr`.
                unsafe { $crate::Reg::from_addr(self.base + offset) }
            }

            $(
                $(#[$fmeta])*
                $fvis fn $field(&self) -> $crate::Reg<$word, $access> {
                    self.__reg_at($offset)
                }
            )*
        }
    };
}

#[cfg(test)]
#[allow(unreachable_pub, clippy::undocumented_unsafe_blocks)]
mod tests {
    use super::*;

    /// Aligned backing memory for host tests.
    #[repr(align(8))]
    struct Mem([u64; 4]);

    impl Mem {
        fn base(&mut self) -> usize {
            self.0.as_mut_ptr() as usize
        }
    }

    #[test]
    fn field_mask_and_get_set() {
        const F: Field<4, 3> = Field::new();
        assert_eq!(F.mask::<u32>(), 0b0111_0000);
        assert_eq!(F.get(0b0101_0000u32), 0b101);
        assert_eq!(F.set(0xFFFF_FFFFu32, 0), 0xFFFF_FF8F);
        // Bits above the field width in `value` are ignored.
        assert_eq!(F.set(0u32, 0b1111), 0b0111_0000);
    }

    #[test]
    fn field_full_width_u8() {
        const ALL: Field<0, 8> = Field::new();
        assert_eq!(ALL.mask::<u8>(), 0xFF);
        assert_eq!(ALL.get(0xABu8), 0xAB);
    }

    #[test]
    fn read_write_modify_on_mock_memory() {
        let mut mem = Mem([0; 4]);
        let reg: Reg<u32, RW> = unsafe { Reg::from_addr(mem.base()) };
        reg.write(0x1234);
        assert_eq!(reg.read(), 0x1234);
        reg.modify(|v| v | 0x1);
        assert_eq!(reg.read(), 0x1235);
        assert_eq!(mem.0[0] as u32, 0x1235);
    }

    #[test]
    fn field_access_on_register() {
        let mut mem = Mem([0; 4]);
        let base = mem.base();
        let reg: Reg<u32, RW> = unsafe { Reg::from_addr(base + 4) };
        const MODE: Field<8, 2> = Field::new();
        reg.modify_field(MODE, 0b10);
        assert_eq!(reg.read_field(MODE), 0b10);
        assert_eq!(reg.read(), 0b10 << 8);
    }

    #[test]
    fn write_one_to_clear() {
        let mut mem = Mem([0; 4]);
        let base = mem.base();
        let reg: Reg<u32, W1C> = unsafe { Reg::from_addr(base) };
        // Plain memory cannot clear bits on write, so the mock stores the
        // written value. On hardware, a 1 written to a W1C bit clears it.
        reg.write(0b1001);
        reg.clear_bits(0b0001);
        assert_eq!(reg.read(), 0b0001);
    }

    #[test]
    fn copies_share_the_same_address() {
        let mut mem = Mem([0; 4]);
        let base = mem.base();
        let a: Reg<u16, RW> = unsafe { Reg::from_addr(base) };
        let b = a;
        a.write(7);
        assert_eq!(b.read(), 7);
        assert_eq!(b.address(), base);
    }

    register_block! {
        struct Block {
            0x00 => data: u32, RW;
            0x08 => status: u64, RO;
            0x10 => clear: u32, W1C;
            0x14 => cmd: u8, WO;
        }
    }

    #[test]
    fn register_block_maps_offsets() {
        let mut mem = Mem([0; 4]);
        let base = mem.base();
        let blk = unsafe { Block::new(base) };
        assert_eq!(blk.base(), base);
        blk.data().write(5);
        assert_eq!(blk.data().read(), 5);
        assert_eq!(blk.data().address(), base);
        assert_eq!(blk.status().address(), base + 0x08);
        assert_eq!(blk.clear().address(), base + 0x10);
        assert_eq!(blk.cmd().address(), base + 0x14);
        blk.cmd().write(9);
        assert_eq!(unsafe { ptr::read_volatile((base + 0x14) as *const u8) }, 9);
    }
}
