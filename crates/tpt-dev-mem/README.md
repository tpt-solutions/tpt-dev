# tpt-dev-mem

Typed, zero-cost access to memory-mapped hardware registers.

- **Access modes in the type:** `RO`, `WO`, `RW` and `W1C`. Writing a read-only register does not compile.
- **Bit fields in const generics:** `Field<OFFSET, WIDTH>` computes masks and shifts at compile time.
- **Peripheral blocks:** `register_block!` declares a block of registers at fixed offsets.
- **Volatile access only:** every hardware read and write uses `read_volatile` or `write_volatile`.
- **`no_std`:** no allocation and no dependencies.

## Example

```rust
use tpt_dev_mem::{register_block, Field, RO, RW};

register_block! {
    pub struct Timer {
        0x00 => pub ctrl: u32, RW;
        0x04 => pub count: u32, RO;
    }
}

const ENABLE: Field<0, 1> = Field::new();

/// # Safety
/// `base` must be the address of a real timer block on this device.
pub unsafe fn start(base: usize) {
    let timer = unsafe { Timer::new(base) };
    timer.ctrl().modify_field(ENABLE, 1);
}
```

## Safety

Constructing a register or a block is `unsafe`. The caller promises that the address is valid and aligned, that it stays mapped, and that its accesses are not racy. After that, the typed read and write methods are safe. See `Reg::from_addr`.
