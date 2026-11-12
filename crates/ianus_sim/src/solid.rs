//! Stopping at solid things: a separate layer on top of hitbox detection.
//!
//! Detection ([`crate::Aabb`], [`crate::BroadPhase`]) only answers "do these
//! overlap?". **This** module decides what to do about it for objects that are
//! *solid*: a body is moved one axis at a time and, if it would end up inside a
//! solid, is placed flush against it. Nothing here is needed to detect overlaps,
//! and objects that are not solid never stop anything.
//!
//! Solids come in two storage forms, chosen automatically for speed:
//!
//! * **Tiles**: a full-tile solid (the usual wall) is one bit in a grid.
//!   Checking it is an array lookup.
//! * **Boxes**: any other solid (a half-width pillar, an offset hitbox) lives
//!   in a [`BroadPhase`].
//!
//! Integer maths only. Tile `(tx, ty)` is centred on
//! `(tx, ty) * SUBUNITS_PER_TILE` and covers half a tile each way.
//!
//! Movement is resolved one axis at a time, which is what makes bodies *slide*
//! along a wall instead of sticking when you push diagonally into it.

use crate::{Aabb, Body, BroadPhase, MoveInput, SUBUNITS_PER_TILE, accelerate};

/// Half the width of the default body, in subunits (0.375 tiles: a 0.75-tile
/// square).
pub const BODY_HALF_SIZE: i32 = 1536;

const HALF_TILE: i32 = SUBUNITS_PER_TILE / 2;

/// The shape of a moving body: half-size, and where the box sits relative to
/// the body's position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Footprint {
    pub half_x: i32,
    pub half_y: i32,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl Footprint {
    /// The standard 0.75-tile square, centred.
    pub const DEFAULT: Footprint = Footprint {
        half_x: BODY_HALF_SIZE,
        half_y: BODY_HALF_SIZE,
        offset_x: 0,
        offset_y: 0,
    };

    /// The hitbox of a body whose position is `(x, y)`.
    #[inline(always)]
    pub const fn aabb_at(&self, x: i32, y: i32) -> Aabb {
        Aabb::from_center(
            x.saturating_add(self.offset_x),
            y.saturating_add(self.offset_y),
            self.half_x,
            self.half_y,
        )
    }
}

impl Default for Footprint {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Which tiles of a room are solid. Everything outside the room counts as
/// solid, so bodies cannot walk off the edge of the world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolidGrid {
    width: u32,
    height: u32,
    solid: Vec<bool>,
}

impl SolidGrid {
    /// An open grid of `width` x `height` tiles.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            solid: vec![false; width as usize * height as usize],
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

    /// Mark a tile solid or open. Tiles outside the grid are ignored.
    pub fn set(&mut self, x: i32, y: i32, solid: bool) {
        if let Some(i) = self.index(x, y) {
            self.solid[i] = solid;
        }
    }

    /// Is this tile solid? Anything outside the grid is.
    #[inline(always)]
    pub fn is_solid(&self, x: i32, y: i32) -> bool {
        match self.index(x, y) {
            Some(i) => self.solid[i],
            None => true,
        }
    }

    /// Would a default-sized body centred at `(x, y)` overlap a solid tile?
    pub fn overlaps(&self, x: i32, y: i32) -> bool {
        self.overlaps_aabb(&Footprint::DEFAULT.aabb_at(x, y))
    }

    /// Does the box overlap any solid tile?
    #[inline]
    pub fn overlaps_aabb(&self, a: &Aabb) -> bool {
        if a.is_empty() {
            return false;
        }
        let (x0, x1) = (tile_of(a.min_x), tile_of(a.max_x - 1));
        let (y0, y1) = (tile_of(a.min_y), tile_of(a.max_y - 1));
        (y0..=y1).any(|ty| (x0..=x1).any(|tx| self.is_solid(tx, ty)))
    }

    /// The nearest blocking edge along x among the solid tiles under `a`:
    /// for `right`, the smallest left edge; otherwise the largest right edge.
    fn block_edge_x(&self, a: &Aabb, right: bool) -> Option<i32> {
        let (x0, x1) = (tile_of(a.min_x), tile_of(a.max_x - 1));
        let (y0, y1) = (tile_of(a.min_y), tile_of(a.max_y - 1));
        let column_blocked = |tx: i32| (y0..=y1).any(|ty| self.is_solid(tx, ty));
        if right {
            (x0..=x1)
                .find(|&tx| column_blocked(tx))
                .map(|tx| tx * SUBUNITS_PER_TILE - HALF_TILE)
        } else {
            (x0..=x1)
                .rev()
                .find(|&tx| column_blocked(tx))
                .map(|tx| tx * SUBUNITS_PER_TILE + HALF_TILE)
        }
    }

    fn block_edge_y(&self, a: &Aabb, up: bool) -> Option<i32> {
        let (x0, x1) = (tile_of(a.min_x), tile_of(a.max_x - 1));
        let (y0, y1) = (tile_of(a.min_y), tile_of(a.max_y - 1));
        let row_blocked = |ty: i32| (x0..=x1).any(|tx| self.is_solid(tx, ty));
        if up {
            (y0..=y1)
                .find(|&ty| row_blocked(ty))
                .map(|ty| ty * SUBUNITS_PER_TILE - HALF_TILE)
        } else {
            (y0..=y1)
                .rev()
                .find(|&ty| row_blocked(ty))
                .map(|ty| ty * SUBUNITS_PER_TILE + HALF_TILE)
        }
    }
}

/// The tile column or row containing a coordinate. Tile `t` covers
/// `[t * TILE - HALF_TILE, t * TILE + HALF_TILE)`.
#[inline(always)]
fn tile_of(coord: i32) -> i32 {
    (i64::from(coord) + i64::from(HALF_TILE)).div_euclid(i64::from(SUBUNITS_PER_TILE)) as i32
}

/// Everything solid in a room: tile solids plus any other solid boxes.
#[derive(Clone, Debug)]
pub struct Solids {
    tiles: SolidGrid,
    boxes: BroadPhase,
}

impl Solids {
    /// Nothing solid yet in a room of `width` x `height` tiles (the room's
    /// edge is still solid).
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            tiles: SolidGrid::new(width, height),
            boxes: BroadPhase::for_room(width, height),
        }
    }

    /// Wrap an existing tile grid, with no extra boxes.
    pub fn from_tiles(tiles: SolidGrid) -> Self {
        let boxes = BroadPhase::for_room(tiles.width, tiles.height);
        Self { tiles, boxes }
    }

    /// The tile layer, for reading.
    pub fn tiles(&self) -> &SolidGrid {
        &self.tiles
    }

    /// Mark one whole tile solid or open.
    pub fn set_tile(&mut self, x: i32, y: i32, solid: bool) {
        self.tiles.set(x, y, solid);
    }

    /// Replace the non-tile solid boxes (rebuilds the grid; reuses buffers).
    pub fn set_boxes(&mut self, boxes: impl IntoIterator<Item = Aabb>) {
        self.boxes
            .rebuild(boxes.into_iter().enumerate().map(|(i, b)| (i as u32, b)));
    }

    pub fn box_count(&self) -> usize {
        self.boxes.len()
    }

    /// Does the hitbox overlap anything solid, including the room's edge?
    #[inline]
    pub fn overlaps(&self, a: &Aabb) -> bool {
        self.tiles.overlaps_aabb(a) || (!self.boxes.is_empty() && self.boxes.any(a))
    }

    fn edge_x(&self, a: &Aabb, right: bool) -> Option<i32> {
        let tile = self.tiles.block_edge_x(a, right);
        let mut best = tile;
        if !self.boxes.is_empty() {
            self.boxes.query(a, |_, b| {
                let e = if right { b.min_x } else { b.max_x };
                best = Some(match best {
                    None => e,
                    Some(c) if right => c.min(e),
                    Some(c) => c.max(e),
                });
            });
        }
        best
    }

    fn edge_y(&self, a: &Aabb, up: bool) -> Option<i32> {
        let tile = self.tiles.block_edge_y(a, up);
        let mut best = tile;
        if !self.boxes.is_empty() {
            self.boxes.query(a, |_, b| {
                let e = if up { b.min_y } else { b.max_y };
                best = Some(match best {
                    None => e,
                    Some(c) if up => c.min(e),
                    Some(c) => c.max(e),
                });
            });
        }
        best
    }
}

/// One tick of movement that stops at solids.
///
/// Same acceleration as [`crate::step_body`]. A body that starts *inside* a
/// solid (a mistake in the room) moves freely until it is out, rather than
/// being thrown back across the map.
///
/// A tick's movement is cut into pieces no longer than the body is wide, and
/// each piece is checked. With pieces that short the body's box before and
/// after a piece always touch or overlap, so nothing can be jumped over, even
/// a thin wall in front of a tiny body. A body of normal size (a third of a
/// tile or more) moves less than its own width per tick, so it is one piece
/// and behaves exactly as before. A body with no area has nothing to collide
/// with and moves freely.
pub fn step_body_solid(body: &mut Body, input: MoveInput, solids: &Solids, fp: &Footprint) {
    accelerate(body, input);

    let stuck = solids.overlaps(&fp.aabb_at(body.pos.x, body.pos.y));
    let piece = fp.half_x.min(fp.half_y).saturating_mul(2);

    // X first, then Y, each against the position the other left behind.
    if stuck || piece <= 0 {
        body.pos.x = body.pos.x.saturating_add(body.vel.x);
    } else {
        let mut left = body.vel.x;
        while left != 0 {
            let step = left.clamp(-piece, piece);
            let nx = body.pos.x.saturating_add(step);
            let probe = fp.aabb_at(nx, body.pos.y);
            if !solids.overlaps(&probe) {
                body.pos.x = nx;
                left -= step;
            } else {
                if let Some(edge) = solids.edge_x(&probe, step > 0) {
                    // Sit flush against the nearest thing in the way.
                    body.pos.x = if step > 0 {
                        edge - fp.half_x - fp.offset_x
                    } else {
                        edge + fp.half_x - fp.offset_x
                    };
                    body.vel.x = 0;
                }
                break;
            }
        }
    }

    if stuck || piece <= 0 {
        body.pos.y = body.pos.y.saturating_add(body.vel.y);
    } else {
        let mut left = body.vel.y;
        while left != 0 {
            let step = left.clamp(-piece, piece);
            let ny = body.pos.y.saturating_add(step);
            let probe = fp.aabb_at(body.pos.x, ny);
            if !solids.overlaps(&probe) {
                body.pos.y = ny;
                left -= step;
            } else {
                if let Some(edge) = solids.edge_y(&probe, step > 0) {
                    body.pos.y = if step > 0 {
                        edge - fp.half_y - fp.offset_y
                    } else {
                        edge + fp.half_y - fp.offset_y
                    };
                    body.vel.y = 0;
                }
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Vec2i, step_body};

    const T: i32 = SUBUNITS_PER_TILE;
    const FP: Footprint = Footprint::DEFAULT;

    fn at_tile(x: i32, y: i32) -> Body {
        Body::at(Vec2i::new(x * T, y * T))
    }

    /// A 12x12 room with a border and a vertical wall with a gap.
    fn room_grid() -> SolidGrid {
        let mut g = SolidGrid::new(12, 12);
        for i in 0..12 {
            g.set(i, 0, true);
            g.set(i, 11, true);
            g.set(0, i, true);
            g.set(11, i, true);
        }
        for y in 1..8 {
            g.set(6, y, true);
        }
        g
    }

    fn room() -> Solids {
        Solids::from_tiles(room_grid())
    }

    fn run(body: &mut Body, s: &Solids, input: MoveInput, ticks: u32) {
        for _ in 0..ticks {
            step_body_solid(body, input, s, &FP);
        }
    }

    fn inside(s: &Solids, b: &Body) -> bool {
        s.overlaps(&FP.aabb_at(b.pos.x, b.pos.y))
    }

    #[test]
    fn tile_lookup_is_centred_on_the_tile() {
        assert_eq!(tile_of(0), 0);
        assert_eq!(tile_of(HALF_TILE - 1), 0);
        assert_eq!(tile_of(HALF_TILE), 1);
        assert_eq!(tile_of(-HALF_TILE), 0);
        assert_eq!(tile_of(-HALF_TILE - 1), -1);
        assert_eq!(tile_of(5 * T), 5);
    }

    #[test]
    fn outside_the_grid_is_solid_and_inside_follows_set() {
        let mut g = SolidGrid::new(3, 3);
        assert!(g.is_solid(-1, 0) && g.is_solid(3, 0) && g.is_solid(0, 3));
        assert!(!g.is_solid(1, 1));
        g.set(1, 1, true);
        assert!(g.is_solid(1, 1));
        g.set(99, 99, true); // ignored, no panic
        g.set(1, 1, false);
        assert!(!g.is_solid(1, 1));
    }

    #[test]
    fn open_ground_moves_exactly_like_the_plain_simulation() {
        let s = Solids::from_tiles(SolidGrid::new(200, 200));
        let mut solid = at_tile(100, 100);
        let mut plain = at_tile(100, 100);
        for t in 0..300u32 {
            let input = MoveInput::from_bits((t / 11) as u8 ^ (t / 7) as u8);
            step_body_solid(&mut solid, input, &s, &FP);
            step_body(&mut plain, input);
        }
        assert_eq!(solid, plain);
    }

    #[test]
    fn a_wall_stops_the_body_flush_against_it() {
        let s = room();
        let mut b = at_tile(3, 3);
        run(&mut b, &s, MoveInput::RIGHT, 200);
        // Wall at tile 6: its left edge is 5.5 tiles; the body's right edge
        // must sit exactly there.
        assert_eq!(b.pos.x + BODY_HALF_SIZE, 6 * T - HALF_TILE);
        assert_eq!(b.vel.x, 0);
        assert_eq!(b.pos.y, 3 * T, "y must be untouched");
    }

    #[test]
    fn every_direction_stops_at_the_border() {
        let s = room();
        // (input, which body edge leads, where that edge must end up)
        type Edge = fn(&Body) -> i32;
        let cases: [(MoveInput, Edge, i32); 3] = [
            (MoveInput::LEFT, |b| b.pos.x - BODY_HALF_SIZE, HALF_TILE),
            (MoveInput::DOWN, |b| b.pos.y - BODY_HALF_SIZE, HALF_TILE),
            (
                MoveInput::UP,
                |b| b.pos.y + BODY_HALF_SIZE,
                11 * T - HALF_TILE,
            ),
        ];
        for (input, edge, expected) in cases {
            let mut b = at_tile(8, 9);
            run(&mut b, &s, input, 400);
            assert_eq!(edge(&b), expected, "{input:?}");
        }
    }

    #[test]
    fn pushing_diagonally_into_a_wall_slides_along_it() {
        let s = room();
        let mut b = at_tile(5, 3);
        run(&mut b, &s, MoveInput::RIGHT.union(MoveInput::UP), 30);
        assert_eq!(
            b.pos.x + BODY_HALF_SIZE,
            6 * T - HALF_TILE,
            "stuck against the wall on x"
        );
        assert!(
            b.pos.y > 3 * T + T,
            "but must keep moving up along it, y = {}",
            b.pos.y
        );
    }

    #[test]
    fn the_gap_in_a_wall_can_be_walked_through() {
        let s = room();
        let mut b = at_tile(4, 9);
        run(&mut b, &s, MoveInput::RIGHT, 100);
        assert!(
            b.pos.x > 7 * T,
            "should pass through the gap, x = {}",
            b.pos.x
        );
    }

    #[test]
    fn a_body_never_overlaps_a_wall_over_a_long_busy_run() {
        let s = room();
        let mut b = at_tile(3, 5);
        for t in 0..20_000u32 {
            let x = (t / 5).wrapping_mul(2_654_435_761) >> 9;
            step_body_solid(&mut b, MoveInput::from_bits(x as u8), &s, &FP);
            assert!(!inside(&s, &b), "inside a wall at tick {t}: {b:?}");
        }
    }

    #[test]
    fn a_body_that_starts_inside_a_wall_can_walk_out_without_teleporting() {
        let s = room();
        let mut b = at_tile(6, 3);
        assert!(inside(&s, &b));
        let mut free = b;
        run(&mut b, &s, MoveInput::LEFT, 40);
        for _ in 0..40 {
            step_body(&mut free, MoveInput::LEFT);
        }
        assert_eq!(b, free);
        assert!(!inside(&s, &b), "still inside after walking out");
    }

    #[test]
    fn a_tiny_body_cannot_jump_over_a_thin_solid() {
        // Both are narrower than one tick of movement at top speed, so a
        // check of only the end position would miss the wall at some
        // alignments. Try every alignment.
        let fp = Footprint {
            half_x: 81,
            half_y: 81,
            offset_x: 0,
            offset_y: 0,
        };
        for off in 0..crate::MAX_SPEED {
            let mut s = Solids::new(40, 5);
            let wall = 20 * T + off;
            s.set_boxes([Aabb::new(wall, 0, wall + 162, 3 * T)]);
            let mut b = at_tile(2, 2);
            for _ in 0..400 {
                step_body_solid(&mut b, MoveInput::RIGHT, &s, &fp);
            }
            assert_eq!(b.pos.x + fp.half_x, wall, "offset {off}: not flush");

            let mut b = at_tile(30, 2);
            for _ in 0..400 {
                step_body_solid(&mut b, MoveInput::LEFT, &s, &fp);
            }
            assert_eq!(b.pos.x - fp.half_x, wall + 162, "offset {off}: not flush");
        }
    }

    #[test]
    fn no_wall_is_ever_skipped_even_at_top_speed() {
        let mut g = SolidGrid::new(9, 9);
        g.set(4, 4, true);
        let s = Solids::from_tiles(g);
        for (sx, sy, input) in [
            (1, 4, MoveInput::RIGHT),
            (7, 4, MoveInput::LEFT),
            (4, 1, MoveInput::UP),
            (4, 7, MoveInput::DOWN),
        ] {
            let mut b = at_tile(sx, sy);
            run(&mut b, &s, input, 200);
            assert!(!inside(&s, &b));
            let crossed = match input {
                MoveInput::RIGHT => b.pos.x > 4 * T,
                MoveInput::LEFT => b.pos.x < 4 * T,
                MoveInput::UP => b.pos.y > 4 * T,
                _ => b.pos.y < 4 * T,
            };
            assert!(!crossed, "tunnelled through the wall: {b:?}");
        }
    }

    /// Refactor guard: this is the value pinned before walls were rebuilt on
    /// top of hitboxes, so equal results prove behaviour did not change.
    #[test]
    fn walled_runs_are_repeatable_and_match_the_golden_hash() {
        let s = room();
        let go = || {
            let mut b = at_tile(3, 5);
            for t in 0..20_000u32 {
                let x = (t / 5).wrapping_mul(2_654_435_761) >> 9;
                step_body_solid(&mut b, MoveInput::from_bits(x as u8), &s, &FP);
            }
            b
        };
        assert_eq!(go(), go());
        assert_eq!(go().state_hash(), 14_648_965_905_719_324_071);
    }

    // ---- boxes that are not whole tiles ----

    /// A half-tile-wide pillar in the middle of tile (5, 5).
    fn pillar() -> Aabb {
        Aabb::from_center(5 * T, 5 * T, T / 4, T / 2)
    }

    fn open_room_with_pillar() -> Solids {
        let mut s = Solids::from_tiles(SolidGrid::new(12, 12));
        s.set_boxes([pillar()]);
        s
    }

    #[test]
    fn a_partial_tile_solid_stops_a_body_flush_against_its_real_edge() {
        let s = open_room_with_pillar();
        let mut b = at_tile(2, 5);
        run(&mut b, &s, MoveInput::RIGHT, 200);
        // The pillar's left edge is 5 tiles minus a quarter tile.
        assert_eq!(b.pos.x + BODY_HALF_SIZE, pillar().min_x);
        // ...which is not where a full tile would have stopped it.
        assert!(b.pos.x + BODY_HALF_SIZE > 5 * T - HALF_TILE);
    }

    #[test]
    fn a_body_can_squeeze_past_a_pillar_that_leaves_room() {
        let s = open_room_with_pillar();
        let mut b = at_tile(2, 8); // well below the pillar's row
        run(&mut b, &s, MoveInput::RIGHT, 200);
        assert!(b.pos.x > 8 * T);
    }

    #[test]
    fn a_body_never_overlaps_a_box_solid_either() {
        let mut s = open_room_with_pillar();
        s.set_boxes([
            pillar(),
            Aabb::new(7 * T, 2 * T, 8 * T + 300, 6 * T),
            Aabb::from_center(3 * T, 8 * T, 700, 1200),
        ]);
        let mut b = at_tile(2, 2);
        for t in 0..20_000u32 {
            let x = (t / 5).wrapping_mul(2_654_435_761) >> 9;
            step_body_solid(&mut b, MoveInput::from_bits(x as u8), &s, &FP);
            assert!(!inside(&s, &b), "inside a solid box at tick {t}: {b:?}");
        }
    }

    #[test]
    fn a_footprint_with_a_different_size_and_offset_is_respected() {
        let s = room();
        // A wide, flat body whose box sits half a tile above its position.
        let fp = Footprint {
            half_x: 2000,
            half_y: 500,
            offset_x: 0,
            offset_y: T / 2,
        };
        let mut b = at_tile(3, 3);
        for _ in 0..200 {
            step_body_solid(&mut b, MoveInput::RIGHT, &s, &fp);
        }
        assert_eq!(
            b.pos.x + fp.half_x,
            6 * T - HALF_TILE,
            "wide body stops sooner"
        );
        for _ in 0..200 {
            step_body_solid(&mut b, MoveInput::UP, &s, &fp);
        }
        // The wall beside the body's path does not block going up, so the top
        // edge of the *box* (position + offset + half) ends flush with the
        // underside of the border row at y = 11.
        assert_eq!(b.pos.y + fp.offset_y + fp.half_y, 11 * T - HALF_TILE);
    }

    #[test]
    fn solids_report_the_box_count() {
        let mut s = Solids::new(10, 10);
        assert_eq!(s.box_count(), 0);
        s.set_boxes([pillar()]);
        assert_eq!(s.box_count(), 1);
        s.set_tile(3, 3, true);
        assert!(s.tiles().is_solid(3, 3));
    }
}
