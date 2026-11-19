//! A decode-style KV-cache workload on the host: many sequences grow token by
//! token, some are forked (as in beam search), and the pool evicts under
//! pressure. Prints utilisation, evictions and the operation rate.
//!
//! Run with: `cargo run --release -p tpt-dev-paged --example kv_workload`

use std::time::Instant;

use tpt_dev_paged::Pager;

const PAGES: usize = 256;
const TOKENS_PER_PAGE: usize = 16;
const BYTES_PER_TOKEN: usize = 32;
const SLOTS: usize = 64;
const MAX_PAGES_PER_SEQ: usize = 32;
const STEPS: u32 = 200_000;

fn main() {
    let mut storage = vec![0u8; PAGES * TOKENS_PER_PAGE * BYTES_PER_TOKEN];
    let mut pager: Pager<'_, PAGES, SLOTS, MAX_PAGES_PER_SEQ> =
        Pager::new(&mut storage, TOKENS_PER_PAGE, BYTES_PER_TOKEN).expect("valid configuration");

    let token = [7u8; BYTES_PER_TOKEN];
    let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };

    let mut live: Vec<usize> = Vec::new();
    let mut next_id = 1u32;
    let mut ops = 0u64;
    let mut frag_sum = 0usize;
    let mut frag_count = 0usize;

    let start = Instant::now();
    for _ in 0..STEPS {
        let r = next() % 100;
        if live.len() < SLOTS / 2 && r < 5 {
            if let Ok(slot) = pager.open(next_id) {
                live.push(slot);
                next_id += 1;
            }
        } else if !live.is_empty() && r < 8 {
            // Fork a sequence, as beam search does.
            let src = live[(next() as usize) % live.len()];
            if let Ok(dst) = pager.fork(src, next_id) {
                live.push(dst);
                next_id += 1;
            }
        } else if !live.is_empty() && r < 10 {
            let i = (next() as usize) % live.len();
            let slot = live.swap_remove(i);
            let _ = pager.close(slot);
        } else if !live.is_empty() {
            // Decode one token on a random live sequence.
            let i = (next() as usize) % live.len();
            let slot = live[i];
            if pager.append(slot, 1).is_ok() {
                if let Some(n) = pager.tokens(slot) {
                    let _ = pager.write(slot, n - 1, &token);
                }
                frag_sum += pager.internal_fragmentation(slot).unwrap_or(0);
                frag_count += 1;
            }
        }
        ops += 1;
    }
    let elapsed = start.elapsed();

    let stats = pager.stats();
    let used = PAGES - stats.free_pages;
    println!(
        "pages in use   : {used} of {PAGES} ({:.1}%)",
        100.0 * used as f64 / PAGES as f64
    );
    println!("active sequences: {}", stats.active_sequences);
    println!("evictions      : {}", stats.evictions);
    if frag_count > 0 {
        println!(
            "mean unused token slots per sequence (last page): {:.2}",
            frag_sum as f64 / frag_count as f64
        );
    }
    println!(
        "operations     : {ops} in {:.1} ms ({:.2} M ops/s)",
        elapsed.as_secs_f64() * 1e3,
        ops as f64 / elapsed.as_secs_f64() / 1e6
    );
}
