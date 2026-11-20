# tpt-dev-mem

Safe, zero-cost MMIO framework for hardware register access.

## Overview

Firmware reads and writes memory-mapped registers through raw pointers, and mistakes such as writing a read-only register, using the wrong register width or shifting a field by hand show up on the device. `tpt-dev-mem` moves those rules into the type system. The access mode (`RO`, `WO`, `RW`, `W1C`) is a type parameter, and bit fields are described with const generics, so their masks and shifts are computed at compile time.

Creating a register or a block is `unsafe`, because the caller must vouch for the address. After that, the typed read and write methods are safe. The crate is `no_std`, allocates nothing and has no dependencies.

It is the base layer for hardware-facing crates in the workspace. [`tpt-dev-sim`](https://docs.rs/tpt-dev-sim) depends on it.

## Features

- **Access modes in the type:** `RO`, `WO`, `RW` and `W1C`. Writing a read-only register does not compile.
- **Register widths:** `Word` is implemented for `u8`, `u16`, `u32` and `u64`. The trait is sealed.
- **Bit fields in const generics:** `Field<OFFSET, WIDTH>` computes masks and shifts at compile time. A zero-width field fails to compile, and a field that does not fit the register width panics when it is used.
- **Peripheral blocks:** `register_block!` declares a block of registers at fixed offsets. Each register is a method that returns a `Reg`.
- **Volatile access only:** hardware reads and writes use `read_volatile` and `write_volatile`.
- **Write-1-to-clear:** `Reg<W, W1C>::clear_bits` writes a mask, so `1` bits are cleared and `0` bits are left alone.

## Installation

```sh
cargo add tpt-dev-mem
```

The crate is `#![no_std]` and does not need `alloc`.

| Feature | Enables | Default |
|---|---|---|
| `std` | Links `std`. It adds no public API. | off |

## Quick start

```rust
use tpt_dev_mem::{register_block, Field, RO, RW};

register_block! {
    /// A small timer.
    pub struct Timer {
        0x00 => pub ctrl: u32, RW;
        0x04 => pub count: u32, RO;
    }
}

const ENABLE: Field<0, 1> = Field::new();

// Backed by ordinary memory here, so the example runs on the host. On a device,
// `base` is the address of the real peripheral.
let mut mem = [0u32; 2];
// SAFETY: `mem` is valid, aligned and not otherwise used while `timer` is in use.
let timer = unsafe { Timer::new(mem.as_mut_ptr() as usize) };
timer.ctrl().modify_field(ENABLE, 1);
assert_eq!(timer.ctrl().read(), 1);
assert_eq!(timer.ctrl().read_field(ENABLE), 1);
```

`Field` also works on plain values: `Field::<4, 3>::new().set(0u32, 0b011)` gives `0b0011_0000`.

## Design notes and guarantees

- **Zero cost.** A `Reg` is a single pointer plus a zero-sized access marker, and a `Field` is a zero-sized type. Masks and shifts come from const generics.
- **Volatile access.** `Reg::read` and `Reg::write` use `read_volatile` and `write_volatile`.
- **Access modes are checked at compile time.** `write` needs a `Writable` mode and `read` needs a `Readable` mode. `modify` and `modify_field` need both.
- **Safety contract.** `Reg::from_addr` requires an address that is valid for volatile reads and writes of `W`, aligned for `W`, kept mapped, not used as ordinary memory, and not accessed in a racy way. A block made with `register_block!` has the same contract for every register in it. A `debug_assert!` checks alignment in debug builds only.
- **Documented `unsafe`.** The crate allows `unsafe_code`, and each `unsafe` block carries a `SAFETY` comment.

## Limitations and scope

- **Read-modify-write is not atomic.** `modify` and `modify_field` read, change and write back. Other bus masters or interrupt handlers can interleave.
- **The crate cannot check addresses.** It does not know the device's memory map. The one `unsafe` call is where you state that the address is right.
- **No memory barriers.** Volatile access keeps the compiler from reordering or removing accesses, but the crate does not insert fences or cache maintenance.
- **Register blocks are plain offsets.** `register_block!` takes literal offsets and does not check for overlap or alignment.
- **Fields that do not fit panic at run time.** A field wider than the register is rejected when a method such as `mask`, `get` or `set` is used.

## Related crates

- [`tpt-dev-sim`](https://docs.rs/tpt-dev-sim): depends on this crate.

This crate depends on no other workspace crate.

## Status and MSRV

Pre-release (0.0.0, not yet published). Minimum supported Rust version: 1.98.

## License

Licensed under either of MIT or Apache-2.0, at your option.
