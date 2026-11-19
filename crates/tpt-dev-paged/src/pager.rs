//! A fixed-size page pool with per-sequence page tables, copy-on-write and LRU eviction.

use core::fmt;

/// Why a pager operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The storage is smaller than the pool needs, or the configuration is invalid.
    BadConfig,
    /// No free page, and nothing can be evicted.
    OutOfPages,
    /// Every sequence slot is in use.
    OutOfSlots,
    /// The sequence would need more pages than its table holds.
    TooLong,
    /// The slot does not hold an active sequence.
    NoSuchSequence,
    /// The sequence was evicted under memory pressure. Its tokens are gone.
    Evicted,
    /// The token index is past the end of the sequence.
    BadToken,
    /// The bytes do not fit one token.
    BadSize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Error::BadConfig => "invalid pager configuration",
            Error::OutOfPages => "no free pages and nothing to evict",
            Error::OutOfSlots => "no free sequence slots",
            Error::TooLong => "sequence exceeds its page table",
            Error::NoSuchSequence => "no such sequence",
            Error::Evicted => "sequence was evicted",
            Error::BadToken => "token index out of range",
            Error::BadSize => "data does not match the token size",
        };
        f.write_str(msg)
    }
}

/// Counts describing the pool's use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    /// Pages with no references.
    pub free_pages: usize,
    /// Total pages.
    pub total_pages: usize,
    /// Sequences that are active (not evicted).
    pub active_sequences: usize,
    /// Sequences evicted so far.
    pub evictions: u64,
}

#[derive(Clone, Copy, Debug)]
struct Seq<const MAXP: usize> {
    id: u32,
    pages: [u16; MAXP],
    npages: usize,
    tokens: usize,
    last_used: u64,
    evicted: bool,
}

/// Manages `N` pages of storage for up to `S` sequences of at most `MAXP` pages each.
///
/// The caller provides the storage, so the pager never allocates. A page holds
/// `tokens_per_page` tokens of `bytes_per_token` bytes each. Pages are shared
/// between sequences by [`Pager::fork`], and a write to a shared page copies it first.
///
/// ```
/// use tpt_dev_paged::Pager;
///
/// let mut storage = [0u8; 4 * 8];
/// // 4 pages, 2 tokens per page, 4 bytes per token, room for 2 sequences of 4 pages.
/// let mut pager: Pager<'_, 4, 2, 4> = Pager::new(&mut storage, 2, 4).unwrap();
/// let a = pager.open(1).unwrap();
/// pager.append(a, 3).unwrap();
/// pager.write(a, 0, &[1, 2, 3, 4]).unwrap();
/// assert_eq!(pager.read(a, 0).unwrap(), &[1, 2, 3, 4]);
/// ```
pub struct Pager<'a, const N: usize, const S: usize, const MAXP: usize> {
    storage: &'a mut [u8],
    tokens_per_page: usize,
    bytes_per_token: usize,
    refs: [u16; N],
    seqs: [Option<Seq<MAXP>>; S],
    tick: u64,
    evictions: u64,
}

impl<'a, const N: usize, const S: usize, const MAXP: usize> Pager<'a, N, S, MAXP> {
    /// Builds a pager over `storage`, which must hold `N` pages.
    pub fn new(
        storage: &'a mut [u8],
        tokens_per_page: usize,
        bytes_per_token: usize,
    ) -> Result<Self, Error> {
        let page_bytes = tokens_per_page
            .checked_mul(bytes_per_token)
            .ok_or(Error::BadConfig)?;
        if N == 0
            || N > u16::MAX as usize
            || S == 0
            || MAXP == 0
            || tokens_per_page == 0
            || bytes_per_token == 0
        {
            return Err(Error::BadConfig);
        }
        if storage.len() < N.checked_mul(page_bytes).ok_or(Error::BadConfig)? {
            return Err(Error::BadConfig);
        }
        Ok(Self {
            storage,
            tokens_per_page,
            bytes_per_token,
            refs: [0; N],
            seqs: [None; S],
            tick: 0,
            evictions: 0,
        })
    }

    fn page_bytes(&self) -> usize {
        self.tokens_per_page * self.bytes_per_token
    }

    /// Opens a new, empty sequence with identifier `id`. Returns its slot.
    pub fn open(&mut self, id: u32) -> Result<usize, Error> {
        let slot = self
            .seqs
            .iter()
            .position(|s| s.is_none())
            .ok_or(Error::OutOfSlots)?;
        self.tick += 1;
        self.seqs[slot] = Some(Seq {
            id,
            pages: [0; MAXP],
            npages: 0,
            tokens: 0,
            last_used: self.tick,
            evicted: false,
        });
        Ok(slot)
    }

    /// Adds `tokens` tokens to the sequence, allocating pages as needed. Evicts
    /// the least recently used other sequence when the pool is full.
    pub fn append(&mut self, slot: usize, tokens: usize) -> Result<(), Error> {
        let seq = self.active(slot)?;
        let new_len = seq.tokens + tokens;
        let need = new_len.div_ceil(self.tokens_per_page);
        if need > MAXP {
            return Err(Error::TooLong);
        }
        let have = seq.npages;
        for _ in have..need {
            let page = self.alloc_page(slot)?;
            if let Some(Some(seq)) = self.seqs.get_mut(slot) {
                seq.pages[seq.npages] = page;
                seq.npages += 1;
            }
        }
        self.touch(slot, new_len);
        Ok(())
    }

    /// Writes one token's bytes. A page shared with another sequence is copied first.
    pub fn write(&mut self, slot: usize, token: usize, data: &[u8]) -> Result<(), Error> {
        if data.len() != self.bytes_per_token {
            return Err(Error::BadSize);
        }
        let seq = self.active(slot)?;
        if token >= seq.tokens {
            return Err(Error::BadToken);
        }
        let index = token / self.tokens_per_page;
        let mut page = seq.pages[index] as usize;
        if self.refs[page] > 1 {
            let fresh = self.alloc_page(slot)? as usize;
            let pb = self.page_bytes();
            self.storage
                .copy_within(page * pb..(page + 1) * pb, fresh * pb);
            self.refs[page] -= 1;
            page = fresh;
            if let Some(Some(seq)) = self.seqs.get_mut(slot) {
                seq.pages[index] = fresh as u16;
            }
        }
        let offset =
            page * self.page_bytes() + (token % self.tokens_per_page) * self.bytes_per_token;
        self.storage[offset..offset + self.bytes_per_token].copy_from_slice(data);
        self.touch(slot, self.seq_tokens(slot));
        Ok(())
    }

    /// Reads one token's bytes.
    pub fn read(&self, slot: usize, token: usize) -> Result<&[u8], Error> {
        let seq = self.active(slot)?;
        if token >= seq.tokens {
            return Err(Error::BadToken);
        }
        let page = seq.pages[token / self.tokens_per_page] as usize;
        let offset =
            page * self.page_bytes() + (token % self.tokens_per_page) * self.bytes_per_token;
        Ok(&self.storage[offset..offset + self.bytes_per_token])
    }

    /// Creates sequence `id` that shares all of `src`'s pages. Writes to either
    /// copy the shared page first, so the two sequences diverge safely.
    pub fn fork(&mut self, src: usize, id: u32) -> Result<usize, Error> {
        let source = *self.active(src)?;
        let dst = self.open(id)?;
        for &page in &source.pages[..source.npages] {
            self.refs[page as usize] += 1;
        }
        if let Some(Some(seq)) = self.seqs.get_mut(dst) {
            seq.pages = source.pages;
            seq.npages = source.npages;
            seq.tokens = source.tokens;
        }
        Ok(dst)
    }

    /// Closes a sequence, releasing its pages that no other sequence shares.
    pub fn close(&mut self, slot: usize) -> Result<(), Error> {
        let seq = match self.seqs.get(slot) {
            Some(Some(s)) => *s,
            _ => return Err(Error::NoSuchSequence),
        };
        if !seq.evicted {
            self.release(&seq);
        }
        self.seqs[slot] = None;
        Ok(())
    }

    /// The identifier a sequence was opened with, or `None` if the slot is empty.
    pub fn sequence_id(&self, slot: usize) -> Option<u32> {
        match self.seqs.get(slot) {
            Some(Some(s)) => Some(s.id),
            _ => None,
        }
    }

    /// Number of tokens in an active sequence.
    pub fn tokens(&self, slot: usize) -> Option<usize> {
        self.active(slot).ok().map(|s| s.tokens)
    }

    /// Token slots reserved but not used, in the sequence's last page. With
    /// fixed-size pages this is the only waste: there is no external fragmentation.
    pub fn internal_fragmentation(&self, slot: usize) -> Option<usize> {
        let seq = self.active(slot).ok()?;
        let capacity = seq.npages * self.tokens_per_page;
        Some(capacity - seq.tokens)
    }

    /// Current usage counts.
    pub fn stats(&self) -> Stats {
        Stats {
            free_pages: self.refs.iter().filter(|&&r| r == 0).count(),
            total_pages: N,
            active_sequences: self.seqs.iter().flatten().filter(|s| !s.evicted).count(),
            evictions: self.evictions,
        }
    }

    fn active(&self, slot: usize) -> Result<&Seq<MAXP>, Error> {
        match self.seqs.get(slot) {
            Some(Some(s)) if s.evicted => Err(Error::Evicted),
            Some(Some(s)) => Ok(s),
            _ => Err(Error::NoSuchSequence),
        }
    }

    fn seq_tokens(&self, slot: usize) -> usize {
        match self.seqs.get(slot) {
            Some(Some(s)) => s.tokens,
            _ => 0,
        }
    }

    fn touch(&mut self, slot: usize, tokens: usize) {
        self.tick += 1;
        let tick = self.tick;
        if let Some(Some(seq)) = self.seqs.get_mut(slot) {
            seq.tokens = tokens;
            seq.last_used = tick;
        }
    }

    /// Finds a free page, evicting the least recently used other sequence if needed.
    fn alloc_page(&mut self, protect: usize) -> Result<u16, Error> {
        loop {
            if let Some(i) = self.refs.iter().position(|&r| r == 0) {
                self.refs[i] = 1;
                return Ok(i as u16);
            }
            if !self.evict_lru(protect) {
                return Err(Error::OutOfPages);
            }
        }
    }

    /// Evicts the least recently used active sequence other than `protect`. Returns `false` if none.
    fn evict_lru(&mut self, protect: usize) -> bool {
        let victim = self
            .seqs
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s {
                Some(seq) if !seq.evicted && i != protect && seq.npages > 0 => {
                    Some((seq.last_used, i))
                }
                _ => None,
            })
            .min();
        let Some((_, slot)) = victim else {
            return false;
        };
        if let Some(seq) = self.seqs[slot] {
            self.release(&seq);
            if let Some(Some(s)) = self.seqs.get_mut(slot) {
                s.evicted = true;
                s.npages = 0;
                s.tokens = 0;
            }
            self.evictions += 1;
        }
        true
    }

    fn release(&mut self, seq: &Seq<MAXP>) {
        for &page in &seq.pages[..seq.npages] {
            let page = page as usize;
            self.refs[page] = self.refs[page].saturating_sub(1);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn pager(storage: &mut [u8]) -> Pager<'_, 4, 3, 4> {
        Pager::new(storage, 2, 1).unwrap()
    }

    #[test]
    fn append_allocates_pages_and_write_read_round_trip() {
        let mut storage = [0u8; 8];
        let mut p = pager(&mut storage);
        let a = p.open(1).unwrap();
        p.append(a, 3).unwrap();
        assert_eq!(p.stats().free_pages, 2);
        p.write(a, 2, &[9]).unwrap();
        assert_eq!(p.read(a, 2).unwrap(), &[9]);
    }

    #[test]
    fn fork_shares_then_copies_on_write() {
        let mut storage = [0u8; 8];
        let mut p = pager(&mut storage);
        let a = p.open(1).unwrap();
        p.append(a, 2).unwrap();
        p.write(a, 0, &[5]).unwrap();
        let b = p.fork(a, 2).unwrap();
        assert_eq!(p.stats().free_pages, 3, "fork shares, so no new page yet");
        p.write(b, 0, &[7]).unwrap();
        assert_eq!(p.read(a, 0).unwrap(), &[5], "the original keeps its data");
        assert_eq!(p.read(b, 0).unwrap(), &[7]);
    }

    #[test]
    fn full_pool_evicts_least_recently_used() {
        let mut storage = [0u8; 8];
        let mut p: Pager<'_, 4, 3, 4> = Pager::new(&mut storage, 2, 1).unwrap();
        let old = p.open(1).unwrap();
        p.append(old, 4).unwrap(); // two pages
        let newer = p.open(2).unwrap();
        p.append(newer, 4).unwrap(); // the other two pages, pool now full
        let third = p.open(3).unwrap();
        p.append(third, 2).unwrap(); // must evict `old`, the least recently used
        assert_eq!(p.stats().evictions, 1);
        assert_eq!(p.read(old, 0), Err(Error::Evicted));
        assert!(p.read(newer, 0).is_ok());
    }

    #[test]
    fn exhaustion_without_victims_is_an_error() {
        // One page in the whole pool. A single sequence that needs a second page
        // has nobody else to evict, so the append fails.
        let mut storage = [0u8; 2];
        let mut p: Pager<'_, 1, 2, 4> = Pager::new(&mut storage, 2, 1).unwrap();
        let a = p.open(1).unwrap();
        p.append(a, 2).unwrap();
        assert_eq!(p.append(a, 1), Err(Error::OutOfPages));
    }

    #[test]
    fn a_second_sequence_evicts_the_first_when_full() {
        let mut storage = [0u8; 2];
        let mut p: Pager<'_, 1, 2, 4> = Pager::new(&mut storage, 2, 1).unwrap();
        let a = p.open(1).unwrap();
        p.append(a, 2).unwrap();
        let b = p.open(2).unwrap();
        p.append(b, 1).unwrap();
        assert_eq!(p.read(a, 0), Err(Error::Evicted));
    }

    #[test]
    fn closing_releases_pages_for_reuse() {
        let mut storage = [0u8; 4];
        let mut p: Pager<'_, 2, 2, 2> = Pager::new(&mut storage, 2, 1).unwrap();
        let a = p.open(1).unwrap();
        p.append(a, 4).unwrap();
        assert_eq!(p.stats().free_pages, 0);
        p.close(a).unwrap();
        assert_eq!(p.stats().free_pages, 2);
    }

    #[test]
    fn internal_fragmentation_counts_unused_slots_in_last_page() {
        let mut storage = [0u8; 8];
        let mut p = pager(&mut storage);
        let a = p.open(1).unwrap();
        p.append(a, 3).unwrap(); // two pages of 2 tokens: one slot unused
        assert_eq!(p.internal_fragmentation(a), Some(1));
        assert_eq!(p.tokens(a), Some(3));
        assert_eq!(p.sequence_id(a), Some(1));
    }

    #[test]
    fn bad_configuration_is_rejected() {
        let mut storage = [0u8; 3];
        assert!(Pager::<'_, 2, 1, 1>::new(&mut storage, 2, 1).is_err());
    }
}
