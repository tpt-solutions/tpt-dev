//! The TrustZone policy and transition model, for Cortex-M NS/S partitions.
//!
//! This module models what the hardware enforces, so it can be tested on the
//! host. It does not program the security attribution unit (SAU), the
//! implementation-defined attribution unit, or the secure gateway veneers. Those
//! are device-specific, and are not written or tested here.
//!
//! Two rules are modelled:
//!
//! * **Partition.** Each address is secure or non-secure. An address no region
//!   covers is secure, which is the safe default. The normal world may access only
//!   non-secure addresses. The secure world may access everything.
//! * **Gate.** The normal world enters the secure world only at a registered
//!   entry point, as with a non-secure callable (NSC) veneer. Entry does not nest,
//!   and leaving is only possible from the secure world.

use crate::traits::{TeeError, World};

/// The security attribution of an address range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Security {
    /// Accessible only from the secure world.
    Secure,
    /// Accessible from both worlds.
    NonSecure,
}

/// A half-open address range, `start..end`, with one attribution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    /// First address in the range.
    pub start: usize,
    /// First address after the range.
    pub end: usize,
    /// The attribution of every address in the range.
    pub security: Security,
}

/// The attribution of the whole address space, as a set of regions.
#[derive(Clone, Copy, Debug)]
pub struct Partition<'a> {
    regions: &'a [Region],
}

impl<'a> Partition<'a> {
    /// A partition over `regions`. Returns `None` if a region is empty, or two overlap.
    pub fn new(regions: &'a [Region]) -> Option<Self> {
        for (i, a) in regions.iter().enumerate() {
            if a.start >= a.end {
                return None;
            }
            for b in &regions[i + 1..] {
                if a.start < b.end && b.start < a.end {
                    return None;
                }
            }
        }
        Some(Self { regions })
    }

    /// The region covering `addr`, if any.
    fn region_at(&self, addr: usize) -> Option<&Region> {
        self.regions
            .iter()
            .find(|r| r.start <= addr && addr < r.end)
    }

    /// Whether `world` may access the `len` bytes at `start`.
    ///
    /// Every byte must be permitted. A range that crosses a boundary is allowed
    /// only if each part is.
    pub fn allows(&self, world: World, start: usize, len: usize) -> bool {
        if world == World::Secure {
            return true;
        }
        let Some(end) = start.checked_add(len) else {
            return false;
        };
        let mut pos = start;
        while pos < end {
            match self.region_at(pos) {
                Some(r) if r.security == Security::NonSecure => pos = r.end.min(end),
                // Uncovered addresses default to secure, and so do secure regions.
                _ => return false,
            }
        }
        true
    }
}

/// The world transition state, with entry only through registered entry points.
#[derive(Clone, Copy, Debug)]
pub struct Gate<'a> {
    entries: &'a [usize],
    world: World,
}

impl<'a> Gate<'a> {
    /// A gate that starts in the normal world, with the given entry points.
    pub const fn new(entries: &'a [usize]) -> Self {
        Self {
            entries,
            world: World::Normal,
        }
    }

    /// The world the processor is running in.
    pub fn world(&self) -> World {
        self.world
    }

    /// Enters the secure world at `entry`. Refused unless `entry` is registered,
    /// and unless the caller is in the normal world, so entry does not nest.
    pub fn enter(&mut self, entry: usize) -> Result<(), TeeError> {
        if self.world == World::Secure || !self.entries.contains(&entry) {
            return Err(TeeError::Denied);
        }
        self.world = World::Secure;
        Ok(())
    }

    /// Returns to the normal world. Refused unless the caller is in the secure world.
    pub fn leave(&mut self) -> Result<(), TeeError> {
        if self.world != World::Secure {
            return Err(TeeError::Denied);
        }
        self.world = World::Normal;
        Ok(())
    }

    /// Checks an access by the current world against `partition`.
    pub fn check(
        &self,
        partition: &Partition<'_>,
        start: usize,
        len: usize,
    ) -> Result<(), TeeError> {
        if partition.allows(self.world, start, len) {
            Ok(())
        } else {
            Err(TeeError::Denied)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const REGIONS: [Region; 2] = [
        Region {
            start: 0x0000,
            end: 0x1000,
            security: Security::NonSecure,
        },
        Region {
            start: 0x1000,
            end: 0x2000,
            security: Security::Secure,
        },
    ];

    #[test]
    fn normal_world_is_limited_to_non_secure_addresses() {
        let p = Partition::new(&REGIONS).unwrap();
        assert!(p.allows(World::Normal, 0x0000, 4));
        assert!(!p.allows(World::Normal, 0x1000, 4));
    }

    #[test]
    fn a_range_crossing_a_boundary_needs_both_sides_to_allow_it() {
        let p = Partition::new(&REGIONS).unwrap();
        assert!(!p.allows(World::Normal, 0x0ffc, 8));
        assert!(p.allows(World::Secure, 0x0ffc, 8));
    }

    #[test]
    fn uncovered_addresses_default_to_secure() {
        let p = Partition::new(&REGIONS).unwrap();
        assert!(!p.allows(World::Normal, 0x2000, 1));
        assert!(p.allows(World::Secure, 0x2000, 1));
    }

    #[test]
    fn overlapping_or_empty_regions_are_rejected() {
        let overlap = [
            Region {
                start: 0,
                end: 0x100,
                security: Security::NonSecure,
            },
            Region {
                start: 0x80,
                end: 0x200,
                security: Security::Secure,
            },
        ];
        assert!(Partition::new(&overlap).is_none());
        let empty = [Region {
            start: 5,
            end: 5,
            security: Security::NonSecure,
        }];
        assert!(Partition::new(&empty).is_none());
    }

    #[test]
    fn the_gate_enters_only_at_registered_entries_and_does_not_nest() {
        const ENTRIES: [usize; 1] = [0x0100];
        let mut gate = Gate::new(&ENTRIES);
        assert_eq!(gate.world(), World::Normal);
        assert_eq!(gate.enter(0x0200), Err(TeeError::Denied));
        gate.enter(0x0100).unwrap();
        assert_eq!(gate.world(), World::Secure);
        assert_eq!(gate.enter(0x0100), Err(TeeError::Denied));
        gate.leave().unwrap();
        assert_eq!(gate.world(), World::Normal);
        assert_eq!(gate.leave(), Err(TeeError::Denied));
    }

    #[test]
    fn checks_follow_the_current_world() {
        const ENTRIES: [usize; 1] = [0x0100];
        let p = Partition::new(&REGIONS).unwrap();
        let mut gate = Gate::new(&ENTRIES);
        assert_eq!(gate.check(&p, 0x1000, 4), Err(TeeError::Denied));
        gate.enter(0x0100).unwrap();
        assert_eq!(gate.check(&p, 0x1000, 4), Ok(()));
        gate.leave().unwrap();
        assert_eq!(gate.check(&p, 0x0000, 4), Ok(()));
    }
}
