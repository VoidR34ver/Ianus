//! Finding which hitboxes touch which, fast.
//!
//! A uniform grid ("spatial hash") laid over the room. Each box is listed in
//! every cell it covers, so a query only looks at the few cells it covers
//! instead of every box in the world. Checking `n` boxes against each other
//! drops from `n * n` tests to roughly `n` times "how many boxes share a cell".
//!
//! Built for the tick loop:
//!
//! * **No allocation after warm-up.** [`BroadPhase::rebuild`] reuses its
//!   buffers; building is two linear passes (count, then fill), the layout is
//!   flat arrays (compressed rows), not a vector per cell.
//! * **No hashing, no sorting.** A cell index is two shifts and a multiply.
//! * **A box found once.** A box covering several cells is reported only from
//!   the cell holding the corner of its overlap with the query, so no
//!   de-duplication set is needed and queries take `&self` (shareable between
//!   threads).
//! * **Deterministic order.** Results come out cell by cell, and in insertion
//!   order within a cell, so two runs on any machine see the same sequence.
//!
//! Boxes outside the grid's bounds are clamped into the edge cells, so
//! nothing is ever lost; it only costs some speed if lots of things live out
//! there.

use crate::Aabb;

/// Cell size as a power of two: `1 << DEFAULT_CELL_SHIFT` subunits. 14 is four
/// tiles.
///
/// Chosen by measurement (`examples/bench_hitboxes.rs`, 50,000 boxes of 0.4 to
/// 1 tile at one box per 4 tiles^2, three random worlds). Per-box cost for
/// finding all overlapping pairs, and for one player-sized query:
///
/// | cell | pairs | query |
/// |------|-------|-------|
/// | 1 tile | 176 ns | 74 ns |
/// | 2 tiles | 126 ns | 58 ns |
/// | **4 tiles** | **97 ns** | **54 ns** |
/// | 8 tiles | 115 ns | 64 ns |
/// | 16 tiles | 149 ns | 112 ns |
///
/// The best size tracks box size and density, so a game with many tiny
/// objects, or a very crowded room, may want [`BroadPhase::new`] with its own
/// shift.
pub const DEFAULT_CELL_SHIFT: u32 = 14;

/// Never allocate more cells than this; a bigger world gets bigger cells.
const MAX_CELLS: u64 = 1 << 22;

/// The cell layout, small and `Copy` so it can be used while buffers are
/// being filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Grid {
    min_x: i32,
    min_y: i32,
    shift: u32,
    cols: u32,
    rows: u32,
}

impl Grid {
    #[inline(always)]
    fn cell_x(&self, x: i32) -> u32 {
        // Saturating keeps far-away coordinates from wrapping; clamping puts
        // them in the nearest edge cell.
        let rel = x.saturating_sub(self.min_x).max(0) as u32;
        (rel >> self.shift).min(self.cols - 1)
    }

    #[inline(always)]
    fn cell_y(&self, y: i32) -> u32 {
        let rel = y.saturating_sub(self.min_y).max(0) as u32;
        (rel >> self.shift).min(self.rows - 1)
    }

    /// The inclusive range of cells a box covers, or `None` for an empty box.
    #[inline(always)]
    fn range(&self, b: &Aabb) -> Option<(u32, u32, u32, u32)> {
        if b.is_empty() {
            return None;
        }
        // The max edge is exclusive, so the last covered coordinate is max - 1.
        Some((
            self.cell_x(b.min_x),
            self.cell_y(b.min_y),
            self.cell_x(b.max_x - 1),
            self.cell_y(b.max_y - 1),
        ))
    }
}

#[derive(Clone, Debug)]
pub struct BroadPhase {
    grid: Grid,
    /// Items in insertion order.
    boxes: Vec<Aabb>,
    ids: Vec<u32>,
    /// `cell_items[cell_start[c]..cell_start[c + 1]]` are the items in cell `c`.
    cell_start: Vec<u32>,
    cell_items: Vec<u32>,
    /// Scratch write cursor per cell, kept to avoid reallocating.
    cursor: Vec<u32>,
}

impl BroadPhase {
    /// A grid covering `bounds`, with cells `1 << shift` subunits wide.
    pub fn new(bounds: Aabb, shift: u32) -> Self {
        let mut shift = shift.min(30);
        let span = |lo: i32, hi: i32| (i64::from(hi) - i64::from(lo)).max(1) as u64;
        let (w, h) = (
            span(bounds.min_x, bounds.max_x),
            span(bounds.min_y, bounds.max_y),
        );
        // Too many cells for this world: grow the cells until it fits.
        while ((w >> shift) + 1) * ((h >> shift) + 1) > MAX_CELLS && shift < 30 {
            shift += 1;
        }
        let grid = Grid {
            min_x: bounds.min_x,
            min_y: bounds.min_y,
            shift,
            cols: ((w >> shift) + 1) as u32,
            rows: ((h >> shift) + 1) as u32,
        };
        // A fresh grid is a valid, empty one: every cell exists and is empty,
        // so querying before the first `rebuild` finds nothing instead of
        // indexing past the end of an empty table.
        let cells = grid.cols as usize * grid.rows as usize;
        Self {
            grid,
            boxes: Vec::new(),
            ids: Vec::new(),
            cell_start: vec![0; cells + 1],
            cell_items: Vec::new(),
            cursor: Vec::new(),
        }
    }

    /// A grid for a room of `width` x `height` tiles.
    pub fn for_room(width_tiles: u32, height_tiles: u32) -> Self {
        let t = crate::SUBUNITS_PER_TILE;
        let half = t / 2;
        let bounds = Aabb::new(
            -half,
            -half,
            (width_tiles as i32).saturating_mul(t).saturating_sub(half),
            (height_tiles as i32).saturating_mul(t).saturating_sub(half),
        );
        Self::new(bounds, DEFAULT_CELL_SHIFT)
    }

    pub fn cell_size(&self) -> i32 {
        1 << self.grid.shift
    }

    pub fn len(&self) -> usize {
        self.boxes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// Replace the contents. `id` is whatever the caller wants back from
    /// queries (an entity index, say). Reuses all buffers.
    pub fn rebuild(&mut self, items: impl IntoIterator<Item = (u32, Aabb)>) {
        self.boxes.clear();
        self.ids.clear();
        for (id, b) in items {
            self.ids.push(id);
            self.boxes.push(b);
        }

        let g = self.grid;
        let cells = g.cols as usize * g.rows as usize;
        self.cell_start.clear();
        self.cell_start.resize(cells + 1, 0);

        // Pass 1: how many items per cell.
        for b in &self.boxes {
            if let Some((x0, y0, x1, y1)) = g.range(b) {
                for cy in y0..=y1 {
                    let row = cy as usize * g.cols as usize;
                    for cx in x0..=x1 {
                        self.cell_start[row + cx as usize + 1] += 1;
                    }
                }
            }
        }
        // Counts become start offsets.
        for c in 0..cells {
            self.cell_start[c + 1] += self.cell_start[c];
        }

        // Pass 2: drop each item into its cells.
        let total = self.cell_start[cells] as usize;
        self.cell_items.clear();
        self.cell_items.resize(total, 0);
        self.cursor.clear();
        self.cursor.extend_from_slice(&self.cell_start[..cells]);
        for (item, b) in self.boxes.iter().enumerate() {
            if let Some((x0, y0, x1, y1)) = g.range(b) {
                for cy in y0..=y1 {
                    let row = cy as usize * g.cols as usize;
                    for cx in x0..=x1 {
                        let c = row + cx as usize;
                        self.cell_items[self.cursor[c] as usize] = item as u32;
                        self.cursor[c] += 1;
                    }
                }
            }
        }
    }

    /// Call `f(id, box)` once for every stored box that overlaps `q`.
    #[inline]
    pub fn query(&self, q: &Aabb, mut f: impl FnMut(u32, &Aabb)) {
        self.query_items(q, |item| f(self.ids[item], &self.boxes[item]));
    }

    /// Like [`query`](Self::query) but hands over the item's position in
    /// insertion order, which is what pair detection needs.
    fn query_items(&self, q: &Aabb, mut f: impl FnMut(usize)) {
        let g = self.grid;
        let Some((x0, y0, x1, y1)) = g.range(q) else {
            return;
        };
        for cy in y0..=y1 {
            let row = cy as usize * g.cols as usize;
            for cx in x0..=x1 {
                let c = row + cx as usize;
                let (s, e) = (self.cell_start[c] as usize, self.cell_start[c + 1] as usize);
                for &item in &self.cell_items[s..e] {
                    let item = item as usize;
                    let b = &self.boxes[item];
                    if !q.overlaps(b) {
                        continue;
                    }
                    // A box in several cells would be seen several times.
                    // Report it only from the cell holding the near corner of
                    // its overlap with the query.
                    let ox = q.min_x.max(b.min_x);
                    let oy = q.min_y.max(b.min_y);
                    if g.cell_x(ox) == cx && g.cell_y(oy) == cy {
                        f(item);
                    }
                }
            }
        }
    }

    /// Is anything stored overlapping `q`? Stops at the first hit.
    #[inline]
    pub fn any(&self, q: &Aabb) -> bool {
        let g = self.grid;
        let Some((x0, y0, x1, y1)) = g.range(q) else {
            return false;
        };
        for cy in y0..=y1 {
            let row = cy as usize * g.cols as usize;
            for cx in x0..=x1 {
                let c = row + cx as usize;
                let (s, e) = (self.cell_start[c] as usize, self.cell_start[c + 1] as usize);
                if self.cell_items[s..e]
                    .iter()
                    .any(|&i| q.overlaps(&self.boxes[i as usize]))
                {
                    return true;
                }
            }
        }
        false
    }

    /// Ids of everything overlapping `q`, appended to `out` (not cleared).
    pub fn query_ids(&self, q: &Aabb, out: &mut Vec<u32>) {
        self.query(q, |id, _| out.push(id));
    }

    /// Every pair of stored boxes that overlap, as `(id_a, id_b)` with `a`
    /// inserted before `b`, appended to `out`. Order is deterministic.
    pub fn pairs(&self, out: &mut Vec<(u32, u32)>) {
        for (i, q) in self.boxes.iter().enumerate() {
            self.query_items(q, |j| {
                if j > i {
                    out.push((self.ids[i], self.ids[j]));
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i32 = crate::SUBUNITS_PER_TILE;

    /// A small deterministic generator, so tests need no external crate.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) as u32
        }
        fn range(&mut self, lo: i32, hi: i32) -> i32 {
            lo + (self.next() % (hi - lo) as u32) as i32
        }
    }

    fn random_boxes(rng: &mut Lcg, n: usize, world_tiles: i32, max_size: i32) -> Vec<Aabb> {
        (0..n)
            .map(|_| {
                let (x, y) = (rng.range(0, world_tiles * T), rng.range(0, world_tiles * T));
                let (w, h) = (rng.range(1, max_size), rng.range(1, max_size));
                Aabb::new(x, y, x + w, y + h)
            })
            .collect()
    }

    fn naive_pairs(boxes: &[Aabb]) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                if boxes[i].overlaps(&boxes[j]) {
                    out.push((i as u32, j as u32));
                }
            }
        }
        out
    }

    fn build(boxes: &[Aabb], world_tiles: i32, shift: u32) -> BroadPhase {
        let mut bp = BroadPhase::new(Aabb::new(0, 0, world_tiles * T, world_tiles * T), shift);
        bp.rebuild(boxes.iter().enumerate().map(|(i, b)| (i as u32, *b)));
        bp
    }

    #[test]
    fn pairs_match_brute_force_for_many_random_worlds_and_cell_sizes() {
        let mut rng = Lcg(1);
        for world in 0..40 {
            let n = 5 + (world * 7) % 150;
            // Mix of tiny, tile-sized and huge boxes (huge ones span many cells).
            let max_size = [T / 4, T, 3 * T, 12 * T][world % 4];
            let boxes = random_boxes(&mut rng, n, 30, max_size);
            let expected = naive_pairs(&boxes);
            for shift in [10, 12, 13, 15] {
                let bp = build(&boxes, 30, shift);
                let mut got = Vec::new();
                bp.pairs(&mut got);
                got.sort_unstable();
                assert_eq!(got, expected, "world {world}, shift {shift}");
            }
        }
    }

    #[test]
    fn point_queries_match_brute_force_and_report_each_box_once() {
        let mut rng = Lcg(2);
        let boxes = random_boxes(&mut rng, 300, 30, 4 * T);
        let bp = build(&boxes, 30, 12);
        for _ in 0..500 {
            let q = random_boxes(&mut rng, 1, 30, 3 * T)[0];
            let mut got = Vec::new();
            bp.query_ids(&q, &mut got);
            let mut sorted = got.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                got.len(),
                sorted.len(),
                "a box was reported twice for {q:?}"
            );
            let expected: Vec<u32> = (0..boxes.len() as u32)
                .filter(|&i| boxes[i as usize].overlaps(&q))
                .collect();
            assert_eq!(sorted, expected, "{q:?}");
            assert_eq!(bp.any(&q), !expected.is_empty());
        }
    }

    #[test]
    fn boxes_outside_the_grid_are_clamped_not_lost() {
        let mut bp = BroadPhase::new(Aabb::new(0, 0, 10 * T, 10 * T), 12);
        let far = Aabb::new(-500 * T, -500 * T, -499 * T, -499 * T);
        let far2 = Aabb::new(900 * T, 900 * T, 901 * T, 901 * T);
        bp.rebuild([(7, far), (8, far2)]);
        let mut out = Vec::new();
        bp.query_ids(&far, &mut out);
        assert_eq!(out, vec![7]);
        out.clear();
        bp.query_ids(&far2, &mut out);
        assert_eq!(out, vec![8]);
        assert!(
            !bp.any(&Aabb::new(0, 0, T, T)),
            "must not hit boxes that are elsewhere"
        );
    }

    /// Regression: a grid that was never rebuilt used to panic on any query.
    #[test]
    fn a_grid_that_was_never_filled_answers_every_query_with_nothing() {
        let bp = BroadPhase::new(Aabb::new(0, 0, 10 * T, 10 * T), 12);
        let everything = Aabb::new(-100 * T, -100 * T, 100 * T, 100 * T);
        assert!(bp.is_empty());
        assert!(!bp.any(&everything));
        let mut out = Vec::new();
        bp.query_ids(&everything, &mut out);
        let mut pairs = Vec::new();
        bp.pairs(&mut pairs);
        assert!(out.is_empty() && pairs.is_empty());

        let room = BroadPhase::for_room(20, 12);
        assert!(!room.any(&Aabb::new(0, 0, T, T)));
    }

    #[test]
    fn empty_boxes_are_never_found_and_empty_queries_find_nothing() {
        let mut bp = BroadPhase::new(Aabb::new(0, 0, 10 * T, 10 * T), 12);
        bp.rebuild([(1, Aabb::new(5, 5, 5, 99)), (2, Aabb::new(0, 0, 100, 100))]);
        let mut out = Vec::new();
        bp.query_ids(&Aabb::new(0, 0, 10 * T, 10 * T), &mut out);
        assert_eq!(out, vec![2]);
        out.clear();
        bp.query_ids(&Aabb::EMPTY, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn results_come_out_in_the_same_order_every_time() {
        let mut rng = Lcg(3);
        let boxes = random_boxes(&mut rng, 200, 20, 2 * T);
        let run = || {
            let bp = build(&boxes, 20, 13);
            let mut p = Vec::new();
            bp.pairs(&mut p);
            p
        };
        assert_eq!(run(), run());
        assert!(
            !run().is_empty(),
            "the test world should have some overlaps"
        );
    }

    #[test]
    fn rebuilding_reuses_the_structure_and_forgets_the_old_contents() {
        let mut bp = BroadPhase::new(Aabb::new(0, 0, 10 * T, 10 * T), 12);
        bp.rebuild([(1, Aabb::new(0, 0, T, T)), (2, Aabb::new(0, 0, T, T))]);
        let mut p = Vec::new();
        bp.pairs(&mut p);
        assert_eq!(p, vec![(1, 2)]);

        bp.rebuild([(3, Aabb::new(0, 0, T, T))]);
        p.clear();
        bp.pairs(&mut p);
        assert!(p.is_empty());
        assert_eq!(bp.len(), 1);
        let mut out = Vec::new();
        bp.query_ids(&Aabb::new(0, 0, T, T), &mut out);
        assert_eq!(out, vec![3]);
    }

    #[test]
    fn a_huge_world_gets_bigger_cells_instead_of_a_huge_allocation() {
        let bp = BroadPhase::new(
            Aabb::new(i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2),
            6,
        );
        let cells = u64::from(bp.grid.cols) * u64::from(bp.grid.rows);
        assert!(cells <= MAX_CELLS, "{cells} cells");
        assert!(bp.cell_size() > 64);
    }

    #[test]
    fn the_room_grid_covers_the_room_exactly() {
        let bp = BroadPhase::for_room(20, 12);
        // A box in the far corner tile is found.
        let mut bp = bp;
        let corner = Aabb::from_center(19 * T, 11 * T, T / 2, T / 2);
        bp.rebuild([(9, corner)]);
        let mut out = Vec::new();
        bp.query_ids(&corner, &mut out);
        assert_eq!(out, vec![9]);
    }
}
