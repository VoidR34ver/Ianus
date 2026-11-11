//! Measures hitbox detection. Headless: no window, only the CPU.
//!
//! ```text
//! cargo run --release -p ianus_sim --example bench_hitboxes
//! ```
//!
//! Always use `--release`; debug numbers mean nothing. Results depend on the
//! machine, so treat them as relative (which variant is faster, how it scales)
//! rather than absolute.

use std::hint::black_box;
use std::time::Instant;

use ianus_sim::{Aabb, BroadPhase, SUBUNITS_PER_TILE as T};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo) as u32) as i32
    }
}

/// Boxes of about 0.4 to 1.0 tiles scattered over a `world` x `world` tile map.
fn scatter(n: usize, world: i32, seed: u64) -> Vec<Aabb> {
    let mut r = Rng(seed);
    (0..n)
        .map(|_| {
            let (x, y) = (r.range(0, world * T), r.range(0, world * T));
            let (w, h) = (r.range(T * 4 / 10, T), r.range(T * 4 / 10, T));
            Aabb::new(x, y, x + w, y + h)
        })
        .collect()
}

fn ns_per(count: u64, f: impl FnOnce() -> u64) -> (f64, u64) {
    let t = Instant::now();
    let result = f();
    (t.elapsed().as_nanos() as f64 / count as f64, result)
}

// ---- overlap formulas -------------------------------------------------

#[inline(always)]
fn short_circuit(a: &Aabb, b: &Aabb) -> bool {
    a.min_x < b.max_x && b.min_x < a.max_x && a.min_y < b.max_y && b.min_y < a.max_y
}

#[inline(always)]
fn branchless(a: &Aabb, b: &Aabb) -> bool {
    a.overlaps(b)
}

#[inline(always)]
fn lanes(a: &Aabb, b: &Aabb) -> bool {
    let l = [a.min_x, a.min_y, b.min_x, b.min_y];
    let r = [b.max_x, b.max_y, a.max_x, a.max_y];
    (l[0] < r[0]) & (l[1] < r[1]) & (l[2] < r[2]) & (l[3] < r[3])
}

/// Centre and half-size form: `|ax - bx| < ahx + bhx` on each axis.
#[derive(Clone, Copy)]
struct CenterHalf {
    cx: i32,
    cy: i32,
    hx: i32,
    hy: i32,
}

#[inline(always)]
fn center_half(a: &CenterHalf, b: &CenterHalf) -> bool {
    ((a.cx - b.cx).abs() < a.hx + b.hx) & ((a.cy - b.cy).abs() < a.hy + b.hy)
}

fn formulas() {
    println!("== overlap formula: ns per test, 4M random pairs (share that overlap is printed) ==");
    let n = 4_000_000usize;
    // A small world so a decent share of pairs actually overlap, which is the
    // case where branch prediction hurts the short-circuit version.
    let a = scatter(n, 14, 1);
    let b = scatter(n, 14, 2);
    let ach: Vec<CenterHalf> = a
        .iter()
        .map(|x| CenterHalf {
            cx: x.min_x / 2 + x.max_x / 2,
            cy: x.min_y / 2 + x.max_y / 2,
            hx: x.width() / 2,
            hy: x.height() / 2,
        })
        .collect();
    let bch: Vec<CenterHalf> = b
        .iter()
        .map(|x| CenterHalf {
            cx: x.min_x / 2 + x.max_x / 2,
            cy: x.min_y / 2 + x.max_y / 2,
            hx: x.width() / 2,
            hy: x.height() / 2,
        })
        .collect();

    let run = |name: &str, f: &dyn Fn(usize) -> bool| {
        // Warm up once, then take the best of five so noise does not decide.
        let mut best = f64::MAX;
        let mut hits = 0;
        for _ in 0..6 {
            let (ns, h) = ns_per(n as u64, || {
                (0..n).filter(|&i| f(black_box(i))).count() as u64
            });
            best = best.min(ns);
            hits = h;
        }
        println!(
            "  {name:<18} {best:6.3} ns   ({:.1}% overlap)",
            hits as f64 * 100.0 / n as f64
        );
        hits
    };
    let h1 = run("short-circuit &&", &|i| short_circuit(&a[i], &b[i]));
    let h2 = run("branchless &", &|i| branchless(&a[i], &b[i]));
    let h3 = run("lanes (array)", &|i| lanes(&a[i], &b[i]));
    let h4 = run("centre + half", &|i| center_half(&ach[i], &bch[i]));
    assert!(h1 == h2 && h2 == h3, "min/max variants must agree");
    // Centre/half rounds odd sizes, so it may differ by a few edge cases.
    println!(
        "  (centre+half disagrees on {} pairs from rounding)",
        h4.abs_diff(h1)
    );
}

// ---- broad phase ------------------------------------------------------

fn all_pairs(boxes: &[Aabb]) -> u64 {
    let mut count = 0;
    for i in 0..boxes.len() {
        for j in i + 1..boxes.len() {
            count += u64::from(boxes[i].overlaps(&boxes[j]));
        }
    }
    count
}

fn broadphase_pairs(
    boxes: &[Aabb],
    world: i32,
    shift: u32,
    out: &mut Vec<(u32, u32)>,
) -> (f64, f64, u64) {
    let mut bp = BroadPhase::new(Aabb::new(0, 0, world * T, world * T), shift);
    let t = Instant::now();
    bp.rebuild(boxes.iter().enumerate().map(|(i, b)| (i as u32, *b)));
    let build = t.elapsed().as_nanos() as f64;
    out.clear();
    let t = Instant::now();
    bp.pairs(out);
    let pairs = t.elapsed().as_nanos() as f64;
    (
        build / boxes.len() as f64,
        pairs / boxes.len() as f64,
        out.len() as u64,
    )
}

fn scaling() {
    println!(
        "\n== all pairs: brute force vs spatial hash (constant density: 1 box per 4 tiles^2) =="
    );
    println!(
        "  {:>7}  {:>14}  {:>16}  {:>10}",
        "boxes", "brute force", "hash build+pairs", "speed-up"
    );
    for n in [500usize, 2_000, 10_000, 50_000, 200_000] {
        let world = ((n as f64 * 4.0).sqrt()) as i32 + 1;
        let boxes = scatter(n, world, 7);
        let mut out = Vec::new();
        // Warm-up so allocation is not measured.
        broadphase_pairs(&boxes, world, 13, &mut out);
        let (b, p, hash_pairs) = broadphase_pairs(&boxes, world, 13, &mut out);
        let hash_ms = (b + p) * n as f64 / 1e6;
        if n <= 10_000 {
            let t = Instant::now();
            let brute_pairs = all_pairs(black_box(&boxes));
            let brute_ms = t.elapsed().as_nanos() as f64 / 1e6;
            assert_eq!(brute_pairs, hash_pairs, "the two must find the same pairs");
            println!(
                "  {n:>7}  {brute_ms:>11.2} ms  {hash_ms:>13.3} ms  {:>9.0}x",
                brute_ms / hash_ms
            );
        } else {
            // Brute force would take n^2 / 2 tests; estimate from the 10k run.
            let est_ms = (n as f64 * n as f64 / 2.0) * 0.4e-6; // ~0.4 ns per test
            println!(
                "  {n:>7}  {est_ms:>9.0} ms*  {hash_ms:>13.3} ms  {:>9.0}x*",
                est_ms / hash_ms
            );
        }
    }
    println!("  (* brute force estimated at 0.4 ns per test, not run)");
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Cost of one player-sized query at a given cell size.
fn query_ns(boxes: &[Aabb], world: i32, shift: u32, seed: u64) -> f64 {
    let mut bp = BroadPhase::new(Aabb::new(0, 0, world * T, world * T), shift);
    bp.rebuild(boxes.iter().enumerate().map(|(i, b)| (i as u32, *b)));
    let mut r = Rng(seed);
    let qs: Vec<Aabb> = (0..400_000)
        .map(|_| Aabb::from_center(r.range(0, world * T), r.range(0, world * T), 1536, 1536))
        .collect();
    let mut runs = Vec::new();
    for _ in 0..5 {
        let (ns, hits) = ns_per(qs.len() as u64, || {
            let mut h = 0u64;
            for q in &qs {
                bp.query(black_box(q), |_, _| h += 1);
            }
            h
        });
        black_box(hits);
        runs.push(ns);
    }
    median(&mut runs)
}

fn cell_sizes() {
    // Three different random worlds, median of 7 runs each, so one lucky or
    // unlucky layout cannot pick the winner.
    println!(
        "\n== cell size: 50,000 boxes (0.4 to 1 tile), 1 per 4 tiles^2; 3 worlds, median of 7 =="
    );
    println!(
        "  {:>5}  {:>9}  {:>9}  {:>10}  {:>11}",
        "shift", "cell", "build+pairs", "per box", "query (ns)"
    );
    let n = 50_000usize;
    let world = ((n as f64 * 4.0).sqrt()) as i32 + 1;
    let worlds: Vec<Vec<Aabb>> = [9u64, 21, 33]
        .iter()
        .map(|&s| scatter(n, world, s))
        .collect();
    let mut out = Vec::new();
    for shift in 10..=16u32 {
        let mut totals = Vec::new();
        for boxes in &worlds {
            for _ in 0..7 {
                let (b, p, _) = broadphase_pairs(boxes, world, shift, &mut out);
                totals.push(b + p);
            }
        }
        let per_box = median(&mut totals);
        let q = (0..3)
            .map(|i| query_ns(&worlds[i], world, shift, 77 + i as u64))
            .sum::<f64>()
            / 3.0;
        let cell = format!("{} tiles", (1 << shift) as f64 / T as f64);
        println!(
            "  {shift:>5}  {cell:>9}  {:>8.2} ms  {per_box:>7.1} ns  {q:>9.1}",
            per_box * n as f64 / 1e6
        );
    }
}

fn point_queries() {
    println!("\n== one player-sized query against a static world (50,000 boxes) ==");
    let n = 50_000usize;
    let world = ((n as f64 * 4.0).sqrt()) as i32 + 1;
    let boxes = scatter(n, world, 11);
    let mut bp = BroadPhase::new(Aabb::new(0, 0, world * T, world * T), 13);
    bp.rebuild(boxes.iter().enumerate().map(|(i, b)| (i as u32, *b)));
    let mut r = Rng(5);
    let queries: Vec<Aabb> = (0..1_000_000)
        .map(|_| {
            let (x, y) = (r.range(0, world * T), r.range(0, world * T));
            Aabb::from_center(x, y, 1536, 1536)
        })
        .collect();

    let (ns, hits) = ns_per(queries.len() as u64, || {
        let mut h = 0;
        for q in &queries {
            bp.query(black_box(q), |_, _| h += 1);
        }
        h
    });
    println!(
        "  query (visit all hits)   {ns:7.1} ns   {:.2} hits/query",
        hits as f64 / queries.len() as f64
    );
    let (ns, hits) = ns_per(queries.len() as u64, || {
        queries.iter().filter(|q| bp.any(black_box(q))).count() as u64
    });
    println!(
        "  any (stop at first hit)  {ns:7.1} ns   {:.1}% of queries hit",
        hits as f64 * 100.0 / queries.len() as f64
    );

    // A sample against brute force, to show what the grid replaces. The result
    // goes through black_box: without it the compiler deletes the whole loop.
    let sample = &queries[..2_000];
    let (ns, total) = ns_per(sample.len() as u64, || {
        let total: u64 = sample
            .iter()
            .map(|q| boxes.iter().filter(|b| q.overlaps(b)).count() as u64)
            .sum();
        black_box(total)
    });
    black_box(total);
    println!("  brute force over all     {ns:7.1} ns   (the same question without the grid)");
}

fn rebuild_cost() {
    println!("\n== rebuild a moving world every tick (allocation reused) ==");
    for n in [1_000usize, 10_000, 100_000] {
        let world = ((n as f64 * 4.0).sqrt()) as i32 + 1;
        let boxes = scatter(n, world, 13);
        let mut bp = BroadPhase::new(Aabb::new(0, 0, world * T, world * T), 13);
        bp.rebuild(boxes.iter().enumerate().map(|(i, b)| (i as u32, *b))); // warm-up
        let t = Instant::now();
        let reps = 20;
        for _ in 0..reps {
            bp.rebuild(boxes.iter().enumerate().map(|(i, b)| (i as u32, *b)));
        }
        let per = t.elapsed().as_secs_f64() * 1e3 / reps as f64;
        println!(
            "  {n:>7} boxes: {per:7.3} ms per rebuild   ({:.1}% of a 16.7 ms tick)",
            per / 16.667 * 100.0
        );
    }
}

fn main() {
    if cfg!(debug_assertions) {
        println!("WARNING: debug build. Run with --release or these numbers are meaningless.\n");
    }
    formulas();
    scaling();
    cell_sizes();
    point_queries();
    rebuild_cost();
}
