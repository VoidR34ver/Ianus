//! Pathfinding on the tile grid.
//!
//! Integer maths only, like the rest of this crate, so the same map and the
//! same question give the same path on every machine.
//!
//! * [`NavGrid`] says which tiles a body can physically stand on, which
//!   *tags* each tile carries (road, water, mud: whatever the game names), and
//!   what it costs to cross.
//! * [`NavFilter`] is what one kind of agent is allowed to walk on: a list of
//!   tags it may use (empty means anything) and a list it must avoid. Walls
//!   always block, whatever the filter says.
//! * [`PathFinder`] is A* over the grid: eight directions, no cutting corners,
//!   fixed tie-breaking, buffers reused between searches.
//! * [`smooth_path`] turns the tile-by-tile route into a few straight legs,
//!   using swept hitbox checks against [`Solids`], so a body follows the route
//!   without zig-zagging along tile centres.
//!
//! Costs: a straight step costs the cost of the tile entered (10 for plain
//! ground), a diagonal step costs 14/10 of that, rounded down. Every tile
//! costs at least 10, which keeps the heuristic honest.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::{Footprint, SUBUNITS_PER_TILE, Solids, Vec2i, tile_of};

/// Cost of crossing one tile of plain ground in a straight line.
pub const GROUND_COST: u8 = 10;

/// What an agent may walk on. Tags are bits, so at most 32 kinds of tile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NavFilter {
    /// If not 0, a tile must carry at least one of these tags. Plain ground
    /// carries no tags, so a non-zero `allow` means "only on these".
    pub allow: u32,
    /// A tile carrying any of these tags is never entered.
    pub deny: u32,
}

impl NavFilter {
    /// Anything goes (walls still block).
    pub const ANY: NavFilter = NavFilter { allow: 0, deny: 0 };

    /// May an agent with this filter enter a tile with these tags?
    #[inline]
    pub const fn admits(&self, tags: u32) -> bool {
        tags & self.deny == 0 && (self.allow == 0 || tags & self.allow != 0)
    }
}

/// The map pathfinding runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavGrid {
    width: u32,
    height: u32,
    open: Vec<bool>,
    tags: Vec<u32>,
    cost: Vec<u8>,
}

impl NavGrid {
    /// An open grid of plain ground, no tags.
    pub fn new(width: u32, height: u32) -> Self {
        let n = width as usize * height as usize;
        Self {
            width,
            height,
            open: vec![true; n],
            tags: vec![0; n],
            cost: vec![GROUND_COST; n],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return None;
        }
        Some(y as usize * self.width as usize + x as usize)
    }

    /// Can a body physically stand here? (Walls and the edge say no.)
    pub fn set_open(&mut self, x: i32, y: i32, open: bool) {
        if let Some(i) = self.index(x, y) {
            self.open[i] = open;
        }
    }

    /// Replace a tile's tags.
    pub fn set_tags(&mut self, x: i32, y: i32, tags: u32) {
        if let Some(i) = self.index(x, y) {
            self.tags[i] = tags;
        }
    }

    /// Add tags to a tile, keeping the ones it has.
    pub fn add_tags(&mut self, x: i32, y: i32, tags: u32) {
        if let Some(i) = self.index(x, y) {
            self.tags[i] |= tags;
        }
    }

    /// Set what crossing a tile costs. Anything under [`GROUND_COST`] is
    /// raised to it.
    pub fn set_cost(&mut self, x: i32, y: i32, cost: u8) {
        if let Some(i) = self.index(x, y) {
            self.cost[i] = cost.max(GROUND_COST);
        }
    }

    pub fn tags(&self, x: i32, y: i32) -> u32 {
        self.index(x, y).map_or(0, |i| self.tags[i])
    }

    /// Physically standable, ignoring tags.
    pub fn is_open(&self, x: i32, y: i32) -> bool {
        self.index(x, y).is_some_and(|i| self.open[i])
    }

    /// May an agent with this filter stand on and enter this tile?
    #[inline]
    pub fn walkable(&self, x: i32, y: i32, filter: &NavFilter) -> bool {
        match self.index(x, y) {
            Some(i) => self.open[i] && filter.admits(self.tags[i]),
            None => false,
        }
    }

    /// The closest walkable tile to `(x, y)` within `max_radius` tiles, or the
    /// tile itself if it is walkable. "Closest" means the smallest square ring
    /// around the tile that holds a walkable one; inside that ring, the shortest
    /// straight-line distance, and ties go to the lowest row, then the lowest
    /// column. For targets standing on a tile the agent may not enter, such as
    /// a player in water.
    pub fn nearest_walkable(
        &self,
        (x, y): (i32, i32),
        filter: &NavFilter,
        max_radius: i32,
    ) -> Option<(i32, i32)> {
        if self.walkable(x, y, filter) {
            return Some((x, y));
        }
        for r in 1..=max_radius.max(0) {
            let mut best: Option<(i64, (i32, i32))> = None;
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r || !self.walkable(x + dx, y + dy, filter) {
                        continue;
                    }
                    let d = i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy);
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, (x + dx, y + dy)));
                    }
                }
            }
            if let Some((_, tile)) = best {
                return Some(tile);
            }
        }
        None
    }
}

/// How a search ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathOutcome {
    /// A cheapest route was written to `out`, start and goal included.
    Found { cost: u32 },
    /// The start and goal are fine but no route exists for this filter.
    NoPath,
    /// Gave up after the allowed number of tile expansions. A route may exist.
    Capped,
    /// The start tile is not walkable for this filter.
    BadStart,
    /// The goal tile is not walkable for this filter.
    BadGoal,
}

/// Neighbour offsets in a fixed order: the four straight ones, then the
/// diagonals. The order only matters when two routes cost the same, and then it
/// must be the same everywhere.
const NEIGHBOURS: [(i32, i32); 8] = [
    (0, 1),
    (1, 0),
    (0, -1),
    (-1, 0),
    (1, 1),
    (1, -1),
    (-1, -1),
    (-1, 1),
];

/// A* with buffers kept between searches. Make one and reuse it.
#[derive(Debug, Default)]
pub struct PathFinder {
    g: Vec<u32>,
    parent: Vec<u32>,
    seen: Vec<u32>,
    done: Vec<u32>,
    generation: u32,
    heap: BinaryHeap<Reverse<(u32, u32, u32)>>,
    expansions: u32,
}

impl PathFinder {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many tiles the most recent search expanded. A tile is expanded at
    /// most once per search, so this never exceeds the walkable tile count.
    pub fn last_expansions(&self) -> u32 {
        self.expansions
    }

    fn prepare(&mut self, cells: usize) {
        if self.g.len() != cells {
            self.g = vec![0; cells];
            self.parent = vec![0; cells];
            self.seen = vec![0; cells];
            self.done = vec![0; cells];
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen.fill(0);
            self.done.fill(0);
            self.generation = 1;
        }
        self.heap.clear();
    }

    /// Find the cheapest route from `start` to `goal` (tile coordinates) for
    /// an agent with `filter`, expanding at most `max_expansions` tiles.
    /// On [`PathOutcome::Found`] `out` holds the tiles from start to goal;
    /// otherwise it is left empty.
    pub fn find(
        &mut self,
        grid: &NavGrid,
        filter: &NavFilter,
        start: (i32, i32),
        goal: (i32, i32),
        max_expansions: u32,
        out: &mut Vec<(i32, i32)>,
    ) -> PathOutcome {
        out.clear();
        self.expansions = 0;
        if !grid.walkable(start.0, start.1, filter) {
            return PathOutcome::BadStart;
        }
        if !grid.walkable(goal.0, goal.1, filter) {
            return PathOutcome::BadGoal;
        }
        if start == goal {
            out.push(start);
            return PathOutcome::Found { cost: 0 };
        }

        let w = grid.width as i32;
        let cells = grid.width as usize * grid.height as usize;
        self.prepare(cells);
        let gen_ = self.generation;
        let idx = |x: i32, y: i32| (y * w + x) as u32;
        let heuristic = |x: i32, y: i32| -> u32 {
            let dx = (x - goal.0).unsigned_abs();
            let dy = (y - goal.1).unsigned_abs();
            // Octile distance at the cheapest possible tile cost.
            10 * dx.max(dy) + 4 * dx.min(dy)
        };

        let s = idx(start.0, start.1) as usize;
        let goal_i = idx(goal.0, goal.1);
        self.g[s] = 0;
        self.seen[s] = gen_;
        let h0 = heuristic(start.0, start.1);
        self.heap.push(Reverse((h0, h0, s as u32)));

        while let Some(Reverse((_, _, i))) = self.heap.pop() {
            let iu = i as usize;
            if self.done[iu] == gen_ {
                continue;
            }
            self.done[iu] = gen_;
            if i == goal_i {
                let cost = self.g[iu];
                let mut at = i;
                out.push(goal);
                while at as usize != s {
                    at = self.parent[at as usize];
                    out.push(((at as i32) % w, (at as i32) / w));
                }
                out.reverse();
                return PathOutcome::Found { cost };
            }
            self.expansions += 1;
            if self.expansions > max_expansions {
                return PathOutcome::Capped;
            }
            let (x, y) = ((i as i32) % w, (i as i32) / w);
            for (k, &(dx, dy)) in NEIGHBOURS.iter().enumerate() {
                let (nx, ny) = (x + dx, y + dy);
                if !grid.walkable(nx, ny, filter) {
                    continue;
                }
                let diagonal = k >= 4;
                // No cutting corners: a diagonal step needs both straight
                // neighbours it passes between to be walkable too.
                if diagonal
                    && !(grid.walkable(x + dx, y, filter) && grid.walkable(x, y + dy, filter))
                {
                    continue;
                }
                let n = idx(nx, ny);
                let nu = n as usize;
                if self.done[nu] == gen_ {
                    continue;
                }
                let tile_cost = u32::from(grid.cost[nu]);
                let step = if diagonal {
                    tile_cost * 14 / 10
                } else {
                    tile_cost
                };
                let ng = self.g[iu] + step;
                if self.seen[nu] != gen_ || ng < self.g[nu] {
                    self.seen[nu] = gen_;
                    self.g[nu] = ng;
                    self.parent[nu] = i;
                    let h = heuristic(nx, ny);
                    // Ties: lower estimate-to-go first (go deeper), then the
                    // lower tile index, so the order is total and fixed.
                    self.heap.push(Reverse((ng + h, h, n)));
                }
            }
        }
        PathOutcome::NoPath
    }
}

/// Can a body with this footprint slide in a straight line from `a` to `b`
/// without touching a solid, or overlapping a tile that `tile_ok` refuses?
///
/// The hitbox is checked at points no further apart than half its size, so it
/// cannot hop over anything. A footprint with no area is never blocked.
pub fn segment_clear(
    solids: &Solids,
    fp: &Footprint,
    a: Vec2i,
    b: Vec2i,
    tile_ok: &dyn Fn(i32, i32) -> bool,
) -> bool {
    let step = i64::from(fp.half_x.min(fp.half_y));
    if step <= 0 {
        return true;
    }
    let dx = i64::from(b.x) - i64::from(a.x);
    let dy = i64::from(b.y) - i64::from(a.y);
    let dist = dx.abs().max(dy.abs());
    let n = ((dist + step - 1) / step).max(1);
    for i in 0..=n {
        let x = (i64::from(a.x) + dx * i / n) as i32;
        let y = (i64::from(a.y) + dy * i / n) as i32;
        let bb = fp.aabb_at(x, y);
        if solids.overlaps(&bb) {
            return false;
        }
        let (x0, x1) = (tile_of(bb.min_x), tile_of(bb.max_x - 1));
        let (y0, y1) = (tile_of(bb.min_y), tile_of(bb.max_y - 1));
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                if !tile_ok(tx, ty) {
                    return false;
                }
            }
        }
    }
    true
}

/// Turn a tile route into waypoints (tile centres, in subunits), skipping
/// every tile that a straight, clear slide can bypass. The first tile (where
/// the agent already is) is not included. Looks at most `lookahead` tiles ahead
/// so one call stays cheap on long routes.
pub fn smooth_path(
    solids: &Solids,
    fp: &Footprint,
    tile_ok: &dyn Fn(i32, i32) -> bool,
    tiles: &[(i32, i32)],
    lookahead: usize,
    out: &mut Vec<Vec2i>,
) {
    out.clear();
    let centre = |t: (i32, i32)| {
        Vec2i::new(
            t.0.saturating_mul(SUBUNITS_PER_TILE),
            t.1.saturating_mul(SUBUNITS_PER_TILE),
        )
    };
    let mut anchor = 0;
    while anchor + 1 < tiles.len() {
        let far = (anchor + lookahead.max(1)).min(tiles.len() - 1);
        let mut best = anchor + 1;
        for j in (anchor + 2..=far).rev() {
            if segment_clear(solids, fp, centre(tiles[anchor]), centre(tiles[j]), tile_ok) {
                best = j;
                break;
            }
        }
        out.push(centre(tiles[best]));
        anchor = best;
    }
}

/// The plain tile-centre route, with no smoothing. First tile left out.
pub fn raw_path(tiles: &[(i32, i32)], out: &mut Vec<Vec2i>) {
    out.clear();
    for &(x, y) in tiles.iter().skip(1) {
        out.push(Vec2i::new(
            x.saturating_mul(SUBUNITS_PER_TILE),
            y.saturating_mul(SUBUNITS_PER_TILE),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Aabb;

    const T: i32 = SUBUNITS_PER_TILE;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) as u32
        }
        fn below(&mut self, n: u32) -> u32 {
            self.next() % n
        }
    }

    fn random_grid(rng: &mut Lcg, w: u32, h: u32, blocked_pct: u32) -> NavGrid {
        let mut g = NavGrid::new(w, h);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                if rng.below(100) < blocked_pct {
                    g.set_open(x, y, false);
                }
                g.set_cost(x, y, (10 + rng.below(4) * 7) as u8);
                if rng.below(100) < 20 {
                    g.set_tags(x, y, 1 << rng.below(3));
                }
            }
        }
        g
    }

    /// Cost of one step into `to`, from the rules in the module docs, written
    /// out separately from the search so the two can disagree.
    fn step_cost(g: &NavGrid, to: (i32, i32), diagonal: bool) -> u32 {
        let c = u32::from(g.cost[to.1 as usize * g.width as usize + to.0 as usize]);
        if diagonal { c * 14 / 10 } else { c }
    }

    /// Is this a legal route, and what does it cost?
    fn route_cost(g: &NavGrid, f: &NavFilter, path: &[(i32, i32)]) -> u32 {
        let mut total = 0;
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            assert!(
                dx.abs() <= 1 && dy.abs() <= 1 && (dx, dy) != (0, 0),
                "not a step"
            );
            assert!(
                g.walkable(b.0, b.1, f),
                "stepped onto a tile it may not use"
            );
            let diagonal = dx != 0 && dy != 0;
            if diagonal {
                assert!(
                    g.walkable(a.0 + dx, a.1, f) && g.walkable(a.0, a.1 + dy, f),
                    "cut a corner"
                );
            }
            total += step_cost(g, b, diagonal);
        }
        total
    }

    /// Plain Dijkstra by repeated scanning: slow, simple, independent.
    fn dijkstra(g: &NavGrid, f: &NavFilter, s: (i32, i32), goal: (i32, i32)) -> Option<u32> {
        let (w, h) = (g.width as i32, g.height as i32);
        let mut dist = vec![u32::MAX; (w * h) as usize];
        let mut done = vec![false; (w * h) as usize];
        dist[(s.1 * w + s.0) as usize] = 0;
        loop {
            let mut best: Option<usize> = None;
            for i in 0..dist.len() {
                if !done[i] && dist[i] != u32::MAX && best.is_none_or(|b| dist[i] < dist[b]) {
                    best = Some(i);
                }
            }
            let Some(i) = best else { return None };
            done[i] = true;
            let (x, y) = (i as i32 % w, i as i32 / w);
            if (x, y) == goal {
                return Some(dist[i]);
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if (dx, dy) == (0, 0) || !g.walkable(x + dx, y + dy, f) {
                        continue;
                    }
                    let diagonal = dx != 0 && dy != 0;
                    if diagonal && !(g.walkable(x + dx, y, f) && g.walkable(x, y + dy, f)) {
                        continue;
                    }
                    let n = ((y + dy) * w + x + dx) as usize;
                    let c = dist[i] + step_cost(g, (x + dx, y + dy), diagonal);
                    if c < dist[n] {
                        dist[n] = c;
                    }
                }
            }
        }
    }

    #[test]
    fn matches_dijkstra_on_random_weighted_grids_with_filters() {
        let mut rng = Lcg(42);
        let mut pf = PathFinder::new();
        let mut out = Vec::new();
        let (mut found, mut none) = (0, 0);
        for case in 0..400 {
            let g = random_grid(&mut rng, 13, 11, 28);
            let filter = match case % 4 {
                0 => NavFilter::ANY,
                1 => NavFilter { allow: 0, deny: 1 },
                2 => NavFilter {
                    allow: 0b110,
                    deny: 0,
                },
                _ => NavFilter {
                    allow: 0,
                    deny: 0b101,
                },
            };
            let s = (rng.below(13) as i32, rng.below(11) as i32);
            let t = (rng.below(13) as i32, rng.below(11) as i32);
            let outcome = pf.find(&g, &filter, s, t, u32::MAX, &mut out);
            match outcome {
                PathOutcome::BadStart => assert!(!g.walkable(s.0, s.1, &filter)),
                PathOutcome::BadGoal => assert!(!g.walkable(t.0, t.1, &filter)),
                PathOutcome::Found { cost } => {
                    found += 1;
                    assert_eq!(out.first(), Some(&s));
                    assert_eq!(out.last(), Some(&t));
                    assert_eq!(route_cost(&g, &filter, &out), cost, "case {case}");
                    assert_eq!(
                        dijkstra(&g, &filter, s, t),
                        Some(cost),
                        "case {case}: not cheapest"
                    );
                }
                PathOutcome::NoPath => {
                    none += 1;
                    assert_eq!(
                        dijkstra(&g, &filter, s, t),
                        None,
                        "case {case}: a route exists"
                    );
                    assert!(out.is_empty());
                }
                PathOutcome::Capped => panic!("no cap was set"),
            }
        }
        assert!(
            found > 100 && none > 10,
            "weak test: found {found}, none {none}"
        );
    }

    #[test]
    fn no_tile_is_expanded_twice() {
        let mut rng = Lcg(99);
        let mut pf = PathFinder::new();
        let mut out = Vec::new();
        for _ in 0..100 {
            let g = random_grid(&mut rng, 15, 15, 20);
            let open = (0..15)
                .flat_map(|y| (0..15).map(move |x| (x, y)))
                .filter(|&(x, y)| g.walkable(x, y, &NavFilter::ANY))
                .count() as u32;
            let s = (rng.below(15) as i32, rng.below(15) as i32);
            let t = (rng.below(15) as i32, rng.below(15) as i32);
            pf.find(&g, &NavFilter::ANY, s, t, u32::MAX, &mut out);
            assert!(
                pf.last_expansions() <= open,
                "{} > {open}",
                pf.last_expansions()
            );
        }
    }

    #[test]
    fn the_same_question_gives_the_same_route_every_time() {
        let mut rng = Lcg(7);
        let g = random_grid(&mut rng, 20, 20, 25);
        let mut reused = PathFinder::new();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for _ in 0..50 {
            let s = (rng.below(20) as i32, rng.below(20) as i32);
            let t = (rng.below(20) as i32, rng.below(20) as i32);
            let r1 = reused.find(&g, &NavFilter::ANY, s, t, u32::MAX, &mut a);
            let r2 = PathFinder::new().find(&g, &NavFilter::ANY, s, t, u32::MAX, &mut b);
            assert_eq!(r1, r2);
            assert_eq!(a, b, "a reused finder disagrees with a fresh one");
        }
    }

    #[test]
    fn a_diagonal_may_not_squeeze_between_two_corners() {
        // . #
        // # .   (y grows upward in the room, but only the pattern matters)
        let mut g = NavGrid::new(2, 2);
        g.set_open(1, 0, false);
        g.set_open(0, 1, false);
        let mut out = Vec::new();
        let r = PathFinder::new().find(&g, &NavFilter::ANY, (0, 0), (1, 1), u32::MAX, &mut out);
        assert_eq!(r, PathOutcome::NoPath);
        // With one of the two open, the diagonal is still refused, so it is
        // a two-step route around.
        g.set_open(1, 0, true);
        let r = PathFinder::new().find(&g, &NavFilter::ANY, (0, 0), (1, 1), u32::MAX, &mut out);
        assert!(matches!(r, PathOutcome::Found { .. }));
        assert_eq!(out.len(), 3, "{out:?}");
    }

    #[test]
    fn trivial_and_bad_endpoints() {
        let mut g = NavGrid::new(5, 5);
        g.set_open(2, 2, false);
        let mut out = vec![(9, 9)];
        let mut pf = PathFinder::new();
        assert_eq!(
            pf.find(&g, &NavFilter::ANY, (1, 1), (1, 1), 10, &mut out),
            PathOutcome::Found { cost: 0 }
        );
        assert_eq!(out, vec![(1, 1)]);
        assert_eq!(
            pf.find(&g, &NavFilter::ANY, (2, 2), (0, 0), 10, &mut out),
            PathOutcome::BadStart
        );
        assert!(out.is_empty());
        assert_eq!(
            pf.find(&g, &NavFilter::ANY, (0, 0), (2, 2), 10, &mut out),
            PathOutcome::BadGoal
        );
        assert_eq!(
            pf.find(&g, &NavFilter::ANY, (0, 0), (9, 9), 10, &mut out),
            PathOutcome::BadGoal
        );
    }

    #[test]
    fn a_cap_gives_up_instead_of_searching_forever() {
        let g = NavGrid::new(60, 60);
        let mut out = Vec::new();
        let mut pf = PathFinder::new();
        assert_eq!(
            pf.find(&g, &NavFilter::ANY, (0, 0), (59, 59), 5, &mut out),
            PathOutcome::Capped
        );
        assert!(out.is_empty());
        // And the finder still works afterwards.
        assert!(matches!(
            pf.find(&g, &NavFilter::ANY, (0, 0), (59, 59), u32::MAX, &mut out),
            PathOutcome::Found { .. }
        ));
    }

    #[test]
    fn tags_decide_where_an_agent_may_walk() {
        // A 7x1 corridor: ground, ground, road, road, water, road, road.
        const ROAD: u32 = 1;
        const WATER: u32 = 2;
        let mut g = NavGrid::new(7, 1);
        for x in 2..7 {
            g.set_tags(x, 0, ROAD);
        }
        g.add_tags(4, 0, WATER);
        let mut out = Vec::new();
        let mut pf = PathFinder::new();
        let go = |pf: &mut PathFinder, f: NavFilter, out: &mut Vec<(i32, i32)>| {
            pf.find(&g, &f, (0, 0), (6, 0), u32::MAX, out)
        };
        assert!(matches!(
            go(&mut pf, NavFilter::ANY, &mut out),
            PathOutcome::Found { .. }
        ));
        assert_eq!(
            go(
                &mut pf,
                NavFilter {
                    allow: 0,
                    deny: WATER
                },
                &mut out
            ),
            PathOutcome::NoPath,
            "the only way through is water"
        );
        // Road-only: the start (plain ground) is not allowed at all.
        assert_eq!(
            go(
                &mut pf,
                NavFilter {
                    allow: ROAD,
                    deny: 0
                },
                &mut out
            ),
            PathOutcome::BadStart
        );
        // Start on the road instead; the water tile (also a road) is allowed.
        let r = pf.find(
            &g,
            &NavFilter {
                allow: ROAD,
                deny: 0,
            },
            (2, 0),
            (6, 0),
            u32::MAX,
            &mut out,
        );
        assert!(matches!(r, PathOutcome::Found { .. }));
        let r = pf.find(
            &g,
            &NavFilter {
                allow: ROAD,
                deny: WATER,
            },
            (2, 0),
            (6, 0),
            u32::MAX,
            &mut out,
        );
        assert_eq!(r, PathOutcome::NoPath);
    }

    #[test]
    fn nearest_walkable_is_closest_and_deterministic() {
        let mut g = NavGrid::new(7, 7);
        let f = NavFilter::ANY;
        assert_eq!(g.nearest_walkable((3, 3), &f, 3), Some((3, 3)));
        for y in 2..=4 {
            for x in 2..=4 {
                g.set_open(x, y, false);
            }
        }
        // All eight neighbours are blocked; ring 2 has many equal-distance
        // tiles, and ties go to the lowest row, then the lowest column.
        assert_eq!(g.nearest_walkable((3, 3), &f, 3), Some((3, 1)));
        assert_eq!(g.nearest_walkable((3, 3), &f, 1), None);
        // The smallest ring wins: (6,6) is two tiles out on the diagonal, (4,7)
        // is three straight down.
        let mut g = NavGrid::new(9, 9);
        for y in 0..9 {
            for x in 0..9 {
                g.set_open(x, y, false);
            }
        }
        g.set_open(6, 6, true);
        g.set_open(4, 7, true);
        assert_eq!(g.nearest_walkable((4, 4), &f, 5), Some((6, 6)));
    }

    #[test]
    fn straight_slides_respect_solids_and_forbidden_tiles() {
        let mut s = Solids::new(12, 6);
        let fp = Footprint::DEFAULT;
        let a = Vec2i::new(T, 2 * T);
        let b = Vec2i::new(9 * T, 2 * T);
        let ok = |_x: i32, _y: i32| true;
        assert!(segment_clear(&s, &fp, a, b, &ok));
        // A thin pillar mid-way, off the tile grid.
        s.set_boxes([Aabb::new(5 * T, T + 100, 5 * T + 200, 3 * T)]);
        assert!(!segment_clear(&s, &fp, a, b, &ok));
        // Forbidden tile in the way, no solids.
        let s = Solids::new(12, 6);
        let no_water = |x: i32, y: i32| !(x == 5 && y == 2);
        assert!(!segment_clear(&s, &fp, a, b, &no_water));
        // A body right beside the forbidden tile clips it with its own width.
        let near = Vec2i::new(T, 3 * T - 1500);
        let near_b = Vec2i::new(9 * T, 3 * T - 1500);
        assert!(
            !segment_clear(&s, &fp, near, near_b, &no_water),
            "hitbox overlaps tile row 2"
        );
        // A footprint with no area is never blocked.
        let zero = Footprint {
            half_x: 0,
            half_y: 0,
            offset_x: 0,
            offset_y: 0,
        };
        assert!(segment_clear(&s, &zero, a, b, &no_water));
    }

    #[test]
    fn smoothing_keeps_the_corners_and_drops_the_rest() {
        let fp = Footprint::DEFAULT;
        // Open room: the whole diagonal-ish route collapses to one leg.
        let s = Solids::new(20, 20);
        let g = NavGrid::new(20, 20);
        let f = NavFilter::ANY;
        let ok = |x: i32, y: i32| g.walkable(x, y, &f);
        let mut out = Vec::new();
        let mut tiles = Vec::new();
        PathFinder::new().find(&g, &f, (2, 2), (15, 9), u32::MAX, &mut tiles);
        let mut legs = Vec::new();
        smooth_path(&s, &fp, &ok, &tiles, 64, &mut legs);
        assert_eq!(legs, vec![Vec2i::new(15 * T, 9 * T)]);

        // A wall with a gap forces a bend.
        let mut s = Solids::new(20, 20);
        let mut g = NavGrid::new(20, 20);
        for y in 0..20 {
            if y != 14 {
                s.set_tile(10, y, true);
                g.set_open(10, y, false);
            }
        }
        let ok = |x: i32, y: i32| g.walkable(x, y, &f);
        PathFinder::new().find(&g, &f, (3, 3), (17, 3), u32::MAX, &mut tiles);
        smooth_path(&s, &fp, &ok, &tiles, 64, &mut legs);
        assert!(legs.len() >= 2 && legs.len() < tiles.len() - 1, "{legs:?}");
        assert_eq!(legs.last(), Some(&Vec2i::new(17 * T, 3 * T)));
        // Every leg is a clear slide.
        let mut from = Vec2i::new(3 * T, 3 * T);
        for &p in &legs {
            assert!(
                segment_clear(&s, &fp, from, p, &ok),
                "leg {from:?} -> {p:?}"
            );
            from = p;
        }
        raw_path(&tiles, &mut out);
        assert_eq!(out.len(), tiles.len() - 1);
        let _ = out;
    }
}
