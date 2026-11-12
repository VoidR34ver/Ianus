//! Deterministic simulation core.
//!
//! Rules for this crate, because determinism is the point:
//!
//! * No floating point. Positions and speeds are integers, so results are
//!   bit-identical on every OS and CPU.
//! * No Bevy, no clocks, no global state. A tick is a pure function of the
//!   previous state and that tick's input.
//! * World units are *subunits*: [`SUBUNITS_PER_TILE`] per tile. `+x` is right,
//!   `+y` is up.

pub mod aabb;
pub mod broadphase;
pub mod nav;
pub mod replay;
pub mod rng;
pub mod schedule;
pub mod solid;

pub use aabb::Aabb;
pub use broadphase::BroadPhase;
pub use nav::{
    GROUND_COST, NavFilter, NavGrid, PathFinder, PathOutcome, raw_path, segment_clear, smooth_path,
};
pub use replay::{Replay, ReplayError};
pub use rng::Rng;
pub use schedule::{MAX_OB_TICK, Scheduler};
pub use solid::{
    BODY_HALF_SIZE, Footprint, SolidGrid, Solids, step_body_solid, step_body_solid_max, tile_of,
};

/// Simulation ticks per second.
pub const TICK_HZ: u32 = 60;

/// Fixed-point resolution: this many subunits make one tile.
pub const SUBUNITS_PER_TILE: i32 = 4096;

/// Top speed on one axis, in subunits per tick (5 tiles per second).
pub const MAX_SPEED: i32 = 341;
/// Speed gained per tick while a direction is held (full speed in 6 ticks).
pub const ACCEL: i32 = 60;
/// Speed lost per tick with no direction held (full stop in 9 ticks).
pub const FRICTION: i32 = 40;

// Diagonal movement scales each axis by 181/256 (about 0.7070) so moving
// diagonally is not faster than moving straight. Integer maths keeps it
// deterministic; the error against 1/sqrt(2) is under 0.1%.
const DIAGONAL_NUM: i32 = 181;
const DIAGONAL_DEN: i32 = 256;

/// A 2D integer vector in subunits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Vec2i {
    pub x: i32,
    pub y: i32,
}

impl Vec2i {
    pub const ZERO: Self = Self { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Which movement keys are down, packed into one byte.
///
/// One byte per tick is also exactly what a replay file needs to store.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MoveInput(u8);

impl MoveInput {
    pub const NONE: Self = Self(0);
    pub const LEFT: Self = Self(1);
    pub const RIGHT: Self = Self(2);
    pub const UP: Self = Self(4);
    pub const DOWN: Self = Self(8);

    /// Build from raw bits; bits above the four direction bits are dropped.
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 0b1111)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Direction as `(x, y)`, each in `-1..=1`. Opposite keys cancel out.
    pub const fn axis(self) -> (i32, i32) {
        let x = self.contains(Self::RIGHT) as i32 - self.contains(Self::LEFT) as i32;
        let y = self.contains(Self::UP) as i32 - self.contains(Self::DOWN) as i32;
        (x, y)
    }
}

/// A moving thing: position and velocity in subunits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Body {
    pub pos: Vec2i,
    pub vel: Vec2i,
}

impl Body {
    pub const fn at(pos: Vec2i) -> Self {
        Self {
            pos,
            vel: Vec2i::ZERO,
        }
    }

    /// FNV-1a hash of the full state. Cheap fingerprint for replay checks:
    /// two runs agree if and only if (in practice) these agree every tick.
    pub fn state_hash(&self) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut h = OFFSET;
        for v in [self.pos.x, self.pos.y, self.vel.x, self.vel.y] {
            for byte in v.to_le_bytes() {
                h ^= u64::from(byte);
                h = h.wrapping_mul(PRIME);
            }
        }
        h
    }
}

/// Move `v` toward `target` by at most `step`, never overshooting.
const fn approach(v: i32, target: i32, step: i32) -> i32 {
    if v < target {
        let n = v + step;
        if n > target { target } else { n }
    } else {
        let n = v - step;
        if n < target { target } else { n }
    }
}

/// How fast one axis changes speed this tick.
///
/// Reversing direction gets accel *and* friction, so turning around feels
/// snappy instead of sliding through a long stop first.
const fn axis_rate(vel: i32, dir: i32) -> i32 {
    if dir == 0 {
        FRICTION
    } else if vel != 0 && (vel > 0) != (dir > 0) {
        ACCEL + FRICTION
    } else {
        ACCEL
    }
}

/// Work out this tick's velocity from the input: acceleration, friction and
/// the diagonal correction. Moves nothing. `max_speed` is the top speed on one
/// axis in subunits per tick; [`MAX_SPEED`] is the default, and slower or
/// faster things (a creature, a vehicle) use their own. Shared by every step
/// function so they all feel identical.
pub fn accelerate_max(body: &mut Body, input: MoveInput, max_speed: i32) {
    let (dx, dy) = input.axis();
    let speed = if dx != 0 && dy != 0 {
        max_speed * DIAGONAL_NUM / DIAGONAL_DEN
    } else {
        max_speed
    };

    body.vel.x = approach(body.vel.x, dx * speed, axis_rate(body.vel.x, dx));
    body.vel.y = approach(body.vel.y, dy * speed, axis_rate(body.vel.y, dy));
}

/// Advance one body by one tick. Allocation-free and branch-light: this is
/// the hot path that will run for every agent, every tick.
pub fn step_body(body: &mut Body, input: MoveInput) {
    step_body_max(body, input, MAX_SPEED);
}

/// [`step_body`] with a chosen top speed.
pub fn step_body_max(body: &mut Body, input: MoveInput, max_speed: i32) {
    accelerate_max(body, input, max_speed);

    // Saturating, so a body at the edge of the world stops instead of
    // wrapping around (which would differ between debug and release).
    body.pos.x = body.pos.x.saturating_add(body.vel.x);
    body.pos.y = body.pos.y.saturating_add(body.vel.y);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(body: &mut Body, input: MoveInput, ticks: u32) {
        for _ in 0..ticks {
            step_body(body, input);
        }
    }

    #[test]
    fn idle_body_stays_put() {
        let mut b = Body::at(Vec2i::new(100, -50));
        run(&mut b, MoveInput::NONE, 1000);
        assert_eq!(b, Body::at(Vec2i::new(100, -50)));
    }

    #[test]
    fn reaches_top_speed_in_expected_ticks_and_never_exceeds_it() {
        let mut b = Body::default();
        for tick in 1..=20 {
            step_body(&mut b, MoveInput::RIGHT);
            assert!(b.vel.x <= MAX_SPEED);
            if tick == 6 {
                assert_eq!(b.vel.x, MAX_SPEED);
            }
        }
        assert_eq!(b.vel.y, 0);
    }

    #[test]
    fn five_tiles_per_second_at_top_speed() {
        let mut b = Body::default();
        run(&mut b, MoveInput::RIGHT, TICK_HZ);
        // One second of holding the key: just under 5 tiles because of the
        // 6-tick ramp-up, and certainly more than 4.
        let tiles = b.pos.x as f64 / SUBUNITS_PER_TILE as f64;
        assert!(tiles > 4.0 && tiles < 5.0, "moved {tiles} tiles");
    }

    #[test]
    fn diagonal_is_not_faster_than_straight() {
        let mut b = Body::default();
        run(&mut b, MoveInput::RIGHT.union(MoveInput::UP), 30);
        let straight = i64::from(MAX_SPEED).pow(2);
        let diag = i64::from(b.vel.x).pow(2) + i64::from(b.vel.y).pow(2);
        let err = (diag - straight).abs() as f64 / straight as f64;
        assert!(err < 0.005, "diagonal speed off by {:.3}%", err * 100.0);
        assert_eq!(b.vel.x, b.vel.y);
    }

    #[test]
    fn opposite_keys_cancel() {
        let mut b = Body::default();
        run(&mut b, MoveInput::LEFT.union(MoveInput::RIGHT), 50);
        assert_eq!(b, Body::default());
    }

    #[test]
    fn friction_stops_exactly_with_no_creep() {
        let mut b = Body::default();
        run(&mut b, MoveInput::UP, 30);
        run(&mut b, MoveInput::NONE, 30);
        assert_eq!(b.vel, Vec2i::ZERO);
        let stopped = b.pos;
        run(&mut b, MoveInput::NONE, 100);
        assert_eq!(b.pos, stopped);
    }

    #[test]
    fn reversing_is_snappier_than_plain_acceleration() {
        let mut b = Body::default();
        run(&mut b, MoveInput::RIGHT, 30);
        assert_eq!(b.vel.x, MAX_SPEED);
        // From +341 to -341 is 682 of speed to shed.
        let mut ticks = 0;
        while b.vel.x > -MAX_SPEED {
            step_body(&mut b, MoveInput::LEFT);
            ticks += 1;
            assert!(ticks < 100);
        }
        let plain = 682 / ACCEL + 1; // what it would take with accel alone
        assert!(ticks < plain, "reverse took {ticks}, plain accel {plain}");
    }

    #[test]
    fn world_edge_saturates_instead_of_wrapping() {
        let mut b = Body::at(Vec2i::new(i32::MAX - 10, 0));
        run(&mut b, MoveInput::RIGHT, 100);
        assert_eq!(b.pos.x, i32::MAX);
    }

    #[test]
    fn input_bits_roundtrip_and_mask() {
        assert_eq!(MoveInput::from_bits(0xFF).bits(), 0b1111);
        assert_eq!(MoveInput::UP.union(MoveInput::LEFT).axis(), (-1, 1));
    }

    /// Scripted "random" input, no external RNG so the test itself is
    /// deterministic everywhere.
    fn scripted_input(tick: u32) -> MoveInput {
        let mut x = tick.wrapping_mul(2_654_435_761).wrapping_add(12_345);
        x ^= x >> 13;
        x = x.wrapping_mul(0x85eb_ca6b);
        x ^= x >> 16;
        // Hold each pick for 8 ticks so there are real runs of movement.
        let held = (x >> 4) as u8;
        MoveInput::from_bits(held ^ (tick / 8) as u8)
    }

    fn replay(ticks: u32) -> Body {
        let mut b = Body::default();
        for t in 0..ticks {
            step_body(&mut b, scripted_input(t / 8 * 8));
        }
        b
    }

    #[test]
    fn replay_is_repeatable() {
        assert_eq!(replay(20_000), replay(20_000));
    }

    /// Golden value. If this changes, simulation behaviour changed: either a
    /// bug, or an intentional tuning change that breaks old replays and must
    /// be called out (update the constant and say why in the commit).
    #[test]
    fn replay_matches_golden_hash() {
        assert_eq!(replay(20_000).state_hash(), 15_410_214_906_945_323_614);
    }
}
