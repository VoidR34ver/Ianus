//! How fast is pathfinding? Run with:
//!
//! ```text
//! cargo run --release -p ianus_sim --example bench_nav
//! ```
//!
//! Random rooms (25% walls, 20% hazard tiles with a cost), random start and
//! goal pairs, many searches each. Reports microseconds per search and tiles
//! expanded, for a cost field kept between searches (what the engine does) and
//! for building one every search (the naive way), and the cost of building a
//! field once. Clocks are fine here: this is a benchmark, not simulation.

use std::time::Instant;

use ianus_sim::{NavCosts, NavFilter, NavGrid, PathFinder, PathOutcome, SearchRange};

struct Lcg(u64);
impl Lcg {
    fn below(&mut self, n: u32) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as u32) % n
    }
}

fn room(rng: &mut Lcg, size: u32) -> NavGrid {
    let mut g = NavGrid::new(size, size);
    for y in 0..size as i32 {
        for x in 0..size as i32 {
            if rng.below(100) < 25 {
                g.set_open(x, y, false);
            } else if rng.below(100) < 20 {
                g.set_tags(x, y, 1);
            }
        }
    }
    g
}

fn main() {
    let costs = NavCosts::NONE.with(0, 30);
    let filter = NavFilter::ANY;
    println!("A*, 8 directions, hazard tiles cost +30, 25% walls");
    println!(
        "{:>9} {:>10} {:>12} {:>12} {:>14} {:>12}",
        "grid", "found", "kept field", "rebuild each", "tiles expanded", "field build"
    );
    for size in [64u32, 128, 256, 512] {
        let mut rng = Lcg(0xC0FFEE ^ u64::from(size));
        let g = room(&mut rng, size);
        let mut field = Vec::new();
        let t = Instant::now();
        let builds = 20;
        for _ in 0..builds {
            g.cost_field(&filter, &costs, &mut field);
        }
        let build_us = t.elapsed().as_secs_f64() * 1e6 / f64::from(builds);

        // Walkable endpoints.
        let mut pairs = Vec::new();
        while pairs.len() < 300 {
            let p = |rng: &mut Lcg| (rng.below(size) as i32, rng.below(size) as i32);
            let (a, b) = (p(&mut rng), p(&mut rng));
            if g.walkable(a.0, a.1, &filter) && g.walkable(b.0, b.1, &filter) {
                pairs.push((a, b));
            }
        }

        let mut pf = PathFinder::new();
        let mut out = Vec::new();
        // Warm up buffers.
        for &(a, b) in pairs.iter().take(20) {
            pf.find_in_field(size, size, &field, a, b, u32::MAX, &mut out);
        }
        let (mut found, mut expanded) = (0u32, 0u64);
        let t = Instant::now();
        for &(a, b) in &pairs {
            let r = pf.find_in_field(size, size, &field, a, b, u32::MAX, &mut out);
            found += u32::from(matches!(r, PathOutcome::Found { .. }));
            expanded += u64::from(pf.last_expansions());
        }
        let kept_us = t.elapsed().as_secs_f64() * 1e6 / pairs.len() as f64;

        let t = Instant::now();
        for &(a, b) in &pairs {
            pf.find_costed(&g, &filter, &costs, a, b, u32::MAX, &mut out);
        }
        let rebuilt_us = t.elapsed().as_secs_f64() * 1e6 / pairs.len() as f64;

        println!(
            "{:>6}x{:<3} {:>9}% {:>9.1} us {:>9.1} us {:>14.0} {:>9.1} us",
            size,
            size,
            100 * found / pairs.len() as u32,
            kept_us,
            rebuilt_us,
            expanded as f64 / pairs.len() as f64,
            build_us,
        );
    }

    search_ranges(&costs, &filter);
}

/// What the search window (as wide as the distance to the goal) buys, on a
/// 256x256 room: goals near the agent, and goals sealed in so that no route
/// exists.
fn search_ranges(costs: &NavCosts, filter: &NavFilter) {
    let size = 256u32;
    let mut rng = Lcg(0xBEEF);
    let g = room(&mut rng, size);
    let mut field = Vec::new();
    g.cost_field(filter, costs, &mut field);
    let mut pf = PathFinder::new();
    let mut out = Vec::new();

    let mut near = Vec::new();
    while near.len() < 400 {
        let a = (
            rng.below(size - 60) as i32 + 30,
            rng.below(size - 60) as i32 + 30,
        );
        let b = (
            a.0 + rng.below(49) as i32 - 24,
            a.1 + rng.below(49) as i32 - 24,
        );
        if g.walkable(a.0, a.1, filter) && g.walkable(b.0, b.1, filter) && a != b {
            near.push((a, b));
        }
    }
    println!(
        "\nSearch range on 256x256 (hazards on). Window = square as wide as the distance to the goal."
    );
    println!(
        "{:>34} {:>11} {:>14} {:>10}",
        "case", "us/search", "tiles looked at", "found"
    );
    for (name, range) in [
        ("whole map", SearchRange::Whole),
        ("strict window", SearchRange::Strict),
        ("growing window", SearchRange::Growing),
    ] {
        let (mut found, mut expanded) = (0u32, 0u64);
        let t = Instant::now();
        for &(a, b) in &near {
            let r = pf.find_ranged(size, size, &field, range, a, b, u32::MAX, &mut out);
            found += u32::from(matches!(r, PathOutcome::Found { .. }));
            expanded += u64::from(pf.last_expansions());
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / near.len() as f64;
        println!(
            "{:>34} {:>8.1} us {:>14.0} {:>9}%",
            format!("goal within 24 tiles, {name}"),
            us,
            expanded as f64 / near.len() as f64,
            100 * found / near.len() as u32
        );
    }

    // The same short searches, but flattening the map into a cost field first
    // each time: what a search costs without a kept field.
    let t = Instant::now();
    for &(a, b) in &near {
        pf.find_costed(&g, filter, costs, a, b, u32::MAX, &mut out);
    }
    println!(
        "{:>34} {:>8.1} us",
        "goal within 24 tiles, no kept field",
        t.elapsed().as_secs_f64() * 1e6 / near.len() as f64
    );

    // Sealed goals: a ring of walls round the goal, so no route exists.
    let mut sealed = g.clone();
    let mut pairs = Vec::new();
    while pairs.len() < 60 {
        let a = (
            rng.below(size - 60) as i32 + 30,
            rng.below(size - 60) as i32 + 30,
        );
        let b = (
            a.0 + rng.below(25) as i32 - 12,
            a.1 + rng.below(25) as i32 - 12,
        );
        if a != b && g.walkable(a.0, a.1, filter) && g.walkable(b.0, b.1, filter) {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if (dx, dy) != (0, 0) {
                        sealed.set_open(b.0 + dx, b.1 + dy, false);
                    }
                }
            }
            pairs.push((a, b));
        }
    }
    sealed.cost_field(filter, costs, &mut field);
    for (name, range) in [
        ("whole map", SearchRange::Whole),
        ("strict window", SearchRange::Strict),
        ("growing window", SearchRange::Growing),
    ] {
        let mut expanded = 0u64;
        let t = Instant::now();
        for &(a, b) in &pairs {
            pf.find_ranged(size, size, &field, range, a, b, u32::MAX, &mut out);
            expanded += u64::from(pf.last_expansions());
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / pairs.len() as f64;
        println!(
            "{:>34} {:>8.1} us {:>14.0} {:>9}",
            format!("sealed-in goal, {name}"),
            us,
            expanded as f64 / pairs.len() as f64,
            "none"
        );
    }
}
