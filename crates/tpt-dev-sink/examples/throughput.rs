//! Host throughput check for the router. Run with:
//! `cargo run --release -p tpt-dev-sink --example throughput`

use std::time::Instant;

use tpt_dev_lock::Spsc;
use tpt_dev_sink::{Busy, Envelope, Lane, Policy, Router, Rule, Sink};

struct Null(u64);

impl Sink<u32> for Null {
    fn send(&mut self, batch: &[u32]) -> Result<(), Busy> {
        self.0 = self.0.wrapping_add(batch.len() as u64);
        Ok(())
    }
}

fn main() {
    const MESSAGES: u64 = 20_000_000;
    const BATCH: usize = 64;

    let mut radio = Null(0);
    let mut flash = Null(0);
    let mut router: Router<u32, 2, 2, BATCH> = Router::new(
        [
            Lane::new(&mut radio, Policy::DropOldest),
            Lane::new(&mut flash, Policy::DropNewest),
        ],
        [
            Rule {
                topic: Some(1),
                lane: 0,
            },
            Rule {
                topic: None,
                lane: 1,
            },
        ],
    )
    .expect("rules name valid lanes");

    // Route straight from the caller, without the ingress queue.
    let start = Instant::now();
    for i in 0..MESSAGES {
        router.route((i & 1) as u16, i as u32);
    }
    router.flush();
    let direct = start.elapsed();
    let direct_total = radio.0 + flash.0;

    // Same work through a lock-free ingress queue, pumped in chunks.
    let mut radio = Null(0);
    let mut flash = Null(0);
    let mut router: Router<u32, 2, 2, BATCH> = Router::new(
        [
            Lane::new(&mut radio, Policy::DropOldest),
            Lane::new(&mut flash, Policy::DropNewest),
        ],
        [
            Rule {
                topic: Some(1),
                lane: 0,
            },
            Rule {
                topic: None,
                lane: 1,
            },
        ],
    )
    .expect("rules name valid lanes");
    let mut q: Spsc<Envelope<u32>, 1024> = Spsc::new();
    let (mut tx, mut rx) = q.split();
    let start = Instant::now();
    let mut sent = 0u64;
    while sent < MESSAGES {
        let mut chunk = 0;
        while chunk < 512 && sent < MESSAGES {
            let env = Envelope {
                topic: (sent & 1) as u16,
                value: sent as u32,
            };
            if tx.push(env).is_ok() {
                sent += 1;
                chunk += 1;
            } else {
                break;
            }
        }
        router.pump(&mut rx);
    }
    router.pump(&mut rx);
    router.flush();
    let queued = start.elapsed();
    let queued_total = radio.0 + flash.0;

    let mps = |n: u64, d: std::time::Duration| n as f64 / d.as_secs_f64() / 1e6;
    println!(
        "direct route : {:>8.1} M msgs/s ({} deliveries)",
        mps(MESSAGES, direct),
        direct_total
    );
    println!(
        "spsc ingress : {:>8.1} M msgs/s ({} deliveries)",
        mps(MESSAGES, queued),
        queued_total
    );
}
