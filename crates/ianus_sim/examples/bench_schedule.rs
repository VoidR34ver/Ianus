//! Measures the three ways to run object updates. Headless, CPU only.
//!
//! ```text
//! cargo run --release -p ianus_sim --example bench_schedule
//! ```
//!
//! 1. **update all**: every object, every tick (no tick rates).
//! 2. **check and skip**: walk every object, update it if `tick % ob_tick == 0`.
//! 3. **buckets**: [`ianus_sim::Scheduler`]; objects that are not due are never
//!    visited.
//!
//! The "update" is a stand-in: one movement step on a body, which is about the
//! cost of a small real update. Numbers are relative, not absolute.

use std::hint::black_box;
use std::time::Instant;

use ianus_sim::{Body, MoveInput, Scheduler, step_body};

const TICKS: u64 = 600;

fn work(body: &mut Body, i: u32) {
    step_body(body, MoveInput::from_bits((i & 15) as u8));
}

/// `mix` is a list of (ob_tick, share out of 100).
fn rates_for(n: usize, mix: &[(u32, u32)]) -> Vec<u32> {
    let mut out = Vec::with_capacity(n);
    let mut i = 0u32;
    while out.len() < n {
        let slot = i.wrapping_mul(2_654_435_761) % 100;
        let mut acc = 0;
        for &(rate, share) in mix {
            acc += share;
            if slot < acc {
                out.push(rate);
                break;
            }
        }
        i += 1;
    }
    out
}

fn best_ms_per_tick(mut f: impl FnMut() -> u64) -> (f64, u64) {
    let mut best = f64::MAX;
    let mut updates = 0;
    for _ in 0..5 {
        let t = Instant::now();
        updates = f();
        best = best.min(t.elapsed().as_secs_f64() * 1e3 / TICKS as f64);
    }
    (best, updates)
}

fn run(label: &str, n: usize, mix: &[(u32, u32)]) {
    let rates = rates_for(n, mix);
    let mut sched = Scheduler::new();
    for (i, &r) in rates.iter().enumerate() {
        sched.insert(i as u32, r);
    }

    let mut bodies = vec![Body::default(); n];
    let (all, _) = best_ms_per_tick(|| {
        let mut u = 0;
        for _ in 0..TICKS {
            for (i, b) in bodies.iter_mut().enumerate() {
                work(b, i as u32);
                u += 1;
            }
        }
        black_box(&bodies);
        u
    });

    let mut bodies = vec![Body::default(); n];
    let (check, check_updates) = best_ms_per_tick(|| {
        let mut u = 0;
        for t in 0..TICKS {
            for (i, b) in bodies.iter_mut().enumerate() {
                let r = rates[i];
                if r != 0 && t % u64::from(r) == 0 {
                    work(b, i as u32);
                    u += 1;
                }
            }
        }
        black_box(&bodies);
        u
    });

    let mut bodies = vec![Body::default(); n];
    let (bucket, bucket_updates) = best_ms_per_tick(|| {
        let mut u = 0;
        for t in 0..TICKS {
            sched.due(t, |i, _| {
                work(&mut bodies[i as usize], i);
                u += 1;
            });
        }
        black_box(&bodies);
        u
    });
    assert_eq!(
        check_updates, bucket_updates,
        "both must run the same number of updates"
    );

    // Worst single tick, which is what causes a visible hitch.
    let worst_check = (0..TICKS)
        .map(|t| {
            rates
                .iter()
                .filter(|&&r| r != 0 && t % u64::from(r) == 0)
                .count()
        })
        .max()
        .unwrap_or(0);
    let worst_bucket = (0..TICKS).map(|t| sched.due_count(t)).max().unwrap_or(0);

    println!(
        "{label}  ({n} objects, {:.1}% due per tick on average)",
        bucket_updates as f64 * 100.0 / (n as f64 * TICKS as f64)
    );
    println!("  update all        {all:8.4} ms/tick");
    println!("  check and skip    {check:8.4} ms/tick   busiest tick: {worst_check} updates");
    println!("  buckets           {bucket:8.4} ms/tick   busiest tick: {worst_bucket} updates");
    println!(
        "  buckets vs check: {:.1}x faster, vs update-all: {:.1}x\n",
        check / bucket,
        all / bucket
    );
}

fn main() {
    if cfg!(debug_assertions) {
        println!("WARNING: debug build. Run with --release or these numbers are meaningless.\n");
    }
    // A room: mostly walls and floor that never update, some slow thinkers,
    // a few things that need every tick.
    let room = [(0, 80), (30, 12), (6, 5), (1, 3)];
    run(
        "typical room (80% never, 12% every 30, 5% every 6, 3% every tick)",
        100_000,
        &room,
    );
    run("same mix, small", 2_000, &room);
    run("everything slow (all every 60)", 100_000, &[(60, 100)]);
    run(
        "everything every tick (worst case for buckets)",
        100_000,
        &[(1, 100)],
    );
}
