//! Pathfinding in the running world: agents that walk to a target by
//! themselves, around walls, over the ground their tags allow.
//!
//! The pieces:
//!
//! * [`NavMap`] is the grid pathfinding runs on. A tile is *open* if a standard
//!   body (0.75 tile) can stand at its centre without touching a solid, and
//!   carries the *tags* painted on it in the room or put on it by objects
//!   standing there ([`NavTags`]). It is rebuilt, in the `World` phase, only
//!   when something solid moved or vanished or a tagged object moved; a
//!   rebuild that changes nothing does not invalidate any route.
//! * [`NavAgent`] says what an entity chases and what it may walk on. Rooms
//!   create agents from an object's `agent` setting; code can add or change
//!   them at any time (for example `NavTarget::Tile` to send a colonist to a
//!   stockpile).
//! * [`NavPath`] is an agent's current route and status.
//!
//! Each tick, in `Sim` right after the player moves:
//!
//! 1. **Plan** (`plan_paths`): for agents that are *due* (their `ob_tick`
//!    sets how often they think; objects not due cost nothing), replan if the
//!    target moved to another tile, the map changed, or the agent got stuck.
//!    The route is A* on the grid, then shortened to a few straight legs.
//! 2. **Steer and move** (`step_agents`): pick a `MoveInput` toward the next
//!    waypoint and move with exactly the physics the player has (same
//!    acceleration, same wall stopping), at the agent's own top speed.
//!
//! Everything is integer maths and fixed ordering, so a replay that includes
//! agents reproduces them exactly.

use bevy_app::{App, FixedUpdate};
use bevy_ecs::prelude::*;
use ianus_objects::{AgentTarget, Kind, Room};
use ianus_sim::{
    Body, Footprint, GROUND_COST, MoveInput, NavCosts, NavFilter, NavGrid, PathFinder, PathOutcome,
    SUBUNITS_PER_TILE, SearchRange, Solids, Vec2i, raw_path, segment_clear, smooth_path,
    step_body_max, step_body_solid_max, tile_of,
};

use crate::body::{Controlled, SimBody, step_controlled};
use crate::collide::{Collider, CollisionRebuilt, SolidMap};
use crate::objects::DueObjects;
use crate::room::{CurrentRoom, PlacedObject};
use crate::tick::IanusSet;

/// How many tiles one search may expand before giving up (a generous cap: a
/// 100x100 room has 10,000).
pub const MAX_EXPANSIONS: u32 = 40_000;
/// How far (in tiles) to look for a walkable tile when the start or the goal
/// is not walkable, such as a target standing in water the agent avoids.
const NEAREST_RADIUS: i32 = 4;
/// How many tiles ahead route smoothing looks.
const SMOOTH_LOOKAHEAD: usize = 24;
/// A waypoint counts as reached within this many subunits (an eighth of a
/// tile): close enough to take corners where the route says, not a tile early.
const WAYPOINT_RADIUS: i64 = 512;
/// Closer than this to a fixed tile goal counts as arrived.
const TILE_ARRIVE_RADIUS: i64 = 600;
/// Ticks without making progress before an agent calls itself stuck.
const STUCK_TICKS: u32 = 45;
/// Progress means moving at least this far (an eighth of a tile).
const PROGRESS: i32 = 512;
/// Stuck this many times in a row and the agent gives up until something changes.
const MAX_STRIKES: u32 = 3;

/// Nav tags an object puts on the tile it stands on. Added to placed objects
/// that have `nav_tags`; the bits follow the room's tag list.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NavTags(pub u32);

/// What an agent goes after.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum NavTarget {
    /// Nothing: the agent stands still.
    #[default]
    None,
    /// The entity the keyboard controls.
    Controlled,
    /// The first placed object with this name.
    Named(String),
    /// A particular entity.
    Entity(Entity),
    /// A tile, to be reached rather than approached.
    Tile(i32, i32),
}

/// Makes an entity walk to a target by itself. Needs a [`SimBody`]; with a
/// [`Collider`] it stops at walls like the player does, without one it passes
/// through them (and still routes around them).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[require(NavPath)]
pub struct NavAgent {
    pub target: NavTarget,
    /// What it may walk on.
    pub filter: NavFilter,
    /// What stepping on each kind of tile costs it, beyond the tile's own cost.
    /// The agent takes the cheapest total route.
    pub costs: NavCosts,
    /// How far a search may look from where the agent stands: the whole map, a
    /// window as wide as the distance to the goal, or that window growing until
    /// a route is found. See [`SearchRange`].
    pub range: SearchRange,
    /// Top speed on one axis, subunits per tick. `step_body` speeds are 341
    /// (about 5 tiles a second); see [`speed_from_tiles_per_second`].
    pub max_speed: i32,
    /// Stops this far from an entity target, in subunits, if it can slide
    /// straight there.
    pub stop_distance: i32,
}

/// Everything about an agent that changes which route it plans. If any of it
/// changes, the route is planned again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NavProfile {
    filter: NavFilter,
    costs: NavCosts,
    range: SearchRange,
}

impl NavAgent {
    pub(crate) fn profile(&self) -> NavProfile {
        NavProfile {
            filter: self.filter,
            costs: self.costs,
            range: self.range,
        }
    }
}

impl Default for NavAgent {
    fn default() -> Self {
        Self {
            target: NavTarget::None,
            filter: NavFilter::ANY,
            costs: NavCosts::NONE,
            range: SearchRange::Growing,
            max_speed: ianus_sim::MAX_SPEED,
            stop_distance: SUBUNITS_PER_TILE,
        }
    }
}

/// Top speed in subunits per tick for a speed in tiles per second. Integer
/// maths: 5 tiles/s is exactly the player's 341.
pub const fn speed_from_tiles_per_second(tiles_per_second: u32) -> i32 {
    let v = tiles_per_second as i64 * SUBUNITS_PER_TILE as i64 / 60;
    if v > i32::MAX as i64 {
        i32::MAX
    } else {
        v as i32
    }
}

/// Where an agent is in following its route.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavStatus {
    /// Has not planned yet, or has no target.
    #[default]
    Idle,
    /// Walking its route.
    Following,
    /// Reached the target (or came within its stop distance).
    Arrived,
    /// No route exists for what this agent may walk on, or it kept getting
    /// stuck. It tries again when the target or the map changes.
    NoPath,
}

/// An agent's route and how it is going. Derived data: it is cleared by Stop
/// and Restart so every run starts the same.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct NavPath {
    pub status: NavStatus,
    /// Where the target is right now (refreshed every tick), if it has one.
    target_pos: Option<Vec2i>,
    waypoints: Vec<Vec2i>,
    next: usize,
    goal_tile: (i32, i32),
    map_version: u64,
    /// What the agent was allowed to walk on when this route was planned.
    profile: NavProfile,
    replan: bool,
    raw: bool,
    stuck_ticks: u32,
    stuck_from: Vec2i,
    strikes: u32,
    /// Steering accumulator (see `steer`). Part of the agent's state.
    steer_acc: i64,
}

impl NavPath {
    /// The waypoints still ahead, for drawing the route.
    pub fn waypoints_ahead(&self) -> &[Vec2i] {
        &self.waypoints[self.next.min(self.waypoints.len())..]
    }

    fn fail(&mut self, goal_tile: (i32, i32), version: u64, profile: NavProfile) {
        self.status = NavStatus::NoPath;
        self.profile = profile;
        self.waypoints.clear();
        self.next = 0;
        self.goal_tile = goal_tile;
        self.map_version = version;
        self.replan = false;
    }
}

/// The grid pathfinding runs on, and a counter that goes up whenever its
/// contents change (routes planned on an older version are replanned).
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct NavMap {
    pub grid: NavGrid,
    pub version: u64,
}

impl NavMap {
    /// Build from a room's solids and the tags in it. `tagged` is the nav tag
    /// bits of each tagged object and where it is.
    pub fn build(
        room: &Room,
        solids: &Solids,
        tagged: impl IntoIterator<Item = (u32, Vec2i)>,
    ) -> NavGrid {
        let mut grid = NavGrid::new(room.width, room.height);
        let t = SUBUNITS_PER_TILE;
        for y in 0..room.height as i32 {
            for x in 0..room.width as i32 {
                let at = Footprint::DEFAULT.aabb_at(x.saturating_mul(t), y.saturating_mul(t));
                if solids.overlaps(&at) {
                    grid.set_open(x, y, false);
                }
            }
        }
        for p in &room.nav_paint {
            if let Some(bit) = room.tag_bit(&p.tag) {
                grid.add_tags(p.x, p.y, bit);
            }
        }
        for (mask, pos) in tagged {
            grid.add_tags(tile_of(pos.x), tile_of(pos.y), mask);
        }
        grid
    }

    /// Build from a room's definitions and placements, no entities needed.
    pub fn from_room(room: &Room, solids: &Solids) -> Self {
        let t = SUBUNITS_PER_TILE;
        let tagged = room.placed.iter().filter_map(|p| {
            let def = room.find_object(&p.object)?;
            let mask = room.tag_mask(&def.nav_tags);
            (mask != 0).then(|| {
                (
                    mask,
                    Vec2i::new(p.x.saturating_mul(t), p.y.saturating_mul(t)),
                )
            })
        });
        Self {
            grid: Self::build(room, solids, tagged),
            version: 1,
        }
    }
}

/// The agent a room's object definition asks for, if it asks for one.
pub(crate) fn agent_from_def(room: &Room, def: &ianus_objects::ObjectDef) -> Option<NavAgent> {
    let agent = def.agent.as_ref()?;
    let speed = match def.kind {
        Kind::Entity {
            speed_tiles_per_s, ..
        } => speed_tiles_per_s,
        _ => 0,
    };
    Some(NavAgent {
        target: match &agent.target {
            AgentTarget::Controlled => NavTarget::Controlled,
            AgentTarget::Named(n) => NavTarget::Named(n.clone()),
        },
        filter: NavFilter {
            allow: room.tag_mask(&agent.allow),
            deny: room.tag_mask(&agent.avoid),
        },
        costs: agent.costs.iter().fold(NavCosts::NONE, |costs, tc| {
            match room.nav_tags.iter().position(|t| *t == tc.tag) {
                Some(i) => costs.with(i, tc.cost),
                None => costs,
            }
        }),
        range: match agent.search {
            ianus_objects::SearchMode::Whole => SearchRange::Whole,
            ianus_objects::SearchMode::Strict => SearchRange::Strict,
            ianus_objects::SearchMode::Growing => SearchRange::Growing,
        },
        max_speed: speed_from_tiles_per_second(speed),
        stop_distance: (i64::from(agent.stop_distance) * i64::from(SUBUNITS_PER_TILE) / 100)
            .min(i64::from(i32::MAX)) as i32,
    })
}

/// Cost fields, one per kind of agent (walk rules plus hazard costs), kept for
/// as long as the nav map does not change. Flattening the map into a plain
/// array of step costs is the expensive part of getting ready to search, so it
/// is done once per kind of agent, not once per search.
#[derive(Resource, Default)]
pub(crate) struct NavFields {
    version: u64,
    entries: Vec<(NavFilter, NavCosts, Vec<u16>)>,
}

impl NavFields {
    /// Most kinds of agent kept at once; more than this and the cache starts over.
    const MAX_KINDS: usize = 64;

    fn get(&mut self, nav: &NavMap, filter: &NavFilter, costs: &NavCosts) -> &[u16] {
        if self.version != nav.version || self.entries.len() >= Self::MAX_KINDS {
            self.entries.clear();
            self.version = nav.version;
        }
        let at = match self
            .entries
            .iter()
            .position(|(f, c, _)| f == filter && c == costs)
        {
            Some(i) => i,
            None => {
                let mut field = Vec::new();
                nav.grid.cost_field(filter, costs, &mut field);
                self.entries.push((*filter, *costs, field));
                self.entries.len() - 1
            }
        };
        &self.entries[at].2
    }
}

/// Search buffers shared by every agent.
#[derive(Resource, Default)]
struct NavScratch {
    finder: PathFinder,
    tiles: Vec<(i32, i32)>,
    legs: Vec<Vec2i>,
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<NavScratch>()
        .init_resource::<NavFields>()
        .add_systems(
            FixedUpdate,
            (
                rebuild_nav.in_set(IanusSet::World).after(CollisionRebuilt),
                (track_targets, plan_paths, step_agents)
                    .chain()
                    .in_set(IanusSet::Sim)
                    .after(step_controlled),
            ),
        );
}

/// Forget every agent's route (not its target). Called by Stop, Restart and
/// Play, so a run never depends on what an earlier run planned.
pub(crate) fn reset_agents(world: &mut World) {
    let mut q = world.query::<&mut NavPath>();
    for mut path in q.iter_mut(world) {
        if *path != NavPath::default() {
            *path = NavPath::default();
        }
    }
}

type TargetQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static SimBody,
        Option<&'static PlacedObject>,
        Has<Controlled>,
    ),
>;

fn resolve_target(
    target: &NavTarget,
    me: Entity,
    controlled: Option<Vec2i>,
    q: &TargetQuery,
) -> Option<Vec2i> {
    match target {
        NavTarget::None => None,
        NavTarget::Tile(x, y) => Some(Vec2i::new(
            x.saturating_mul(SUBUNITS_PER_TILE),
            y.saturating_mul(SUBUNITS_PER_TILE),
        )),
        NavTarget::Entity(e) => q.get(*e).ok().map(|(_, b, _, _)| b.0.pos),
        NavTarget::Controlled => controlled,
        // Lowest entity index among the matches, so which one it picks does
        // not depend on the order the query happens to visit them in.
        NavTarget::Named(name) => q
            .iter()
            .filter(|(e, _, p, _)| *e != me && p.is_some_and(|p| &p.name == name))
            .min_by_key(|(e, _, _, _)| e.index())
            .map(|(_, b, _, _)| b.0.pos),
    }
}

/// Every tick, note where each agent's target is. Done for all agents, apart
/// from planning, because steering uses it every tick.
fn track_targets(mut agents: Query<(Entity, &NavAgent, &mut NavPath)>, targets: TargetQuery) {
    let controlled = targets
        .iter()
        .find(|(_, _, _, c)| *c)
        .map(|(_, b, _, _)| b.0.pos);
    for (entity, agent, mut path) in &mut agents {
        let pos = resolve_target(&agent.target, entity, controlled, &targets);
        if path.target_pos != pos {
            path.target_pos = pos;
        }
    }
}

type MovedTagged<'w, 's> = Query<'w, 's, (), (With<NavTags>, Changed<SimBody>)>;

/// Keep the nav map current, cheaply. Work happens only when the solid map was
/// replaced, a tagged object moved, or one disappeared. If the rebuilt grid
/// is identical the version stays, so no route is thrown away for nothing.
fn rebuild_nav(
    solids: Option<Res<SolidMap>>,
    room: Option<Res<CurrentRoom>>,
    tagged: Query<(&NavTags, &SimBody)>,
    moved: MovedTagged,
    mut removed: RemovedComponents<NavTags>,
    nav: Option<ResMut<NavMap>>,
) {
    let vanished = removed.read().count() > 0;
    let (Some(solids), Some(room), Some(mut nav)) = (solids, room, nav) else {
        return;
    };
    if !(solids.is_changed() || !moved.is_empty() || vanished) {
        return;
    }
    let grid = NavMap::build(
        &room.0,
        &solids.0,
        tagged.iter().map(|(t, b)| (t.0, b.0.pos)),
    );
    if grid != nav.grid {
        nav.grid = grid;
        nav.version += 1;
    }
}

type PlanQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static SimBody,
        &'static NavAgent,
        &'static mut NavPath,
        Option<&'static Collider>,
    ),
>;

/// Decide, for the agents that are due this tick, whether the route needs
/// replanning, and replan.
fn plan_paths(
    due: Res<DueObjects>,
    nav: Option<Res<NavMap>>,
    solids: Option<Res<SolidMap>>,
    mut scratch: ResMut<NavScratch>,
    mut fields: ResMut<NavFields>,
    mut agents: PlanQuery,
) {
    let (Some(nav), Some(solids)) = (nav, solids) else {
        return;
    };
    let grid = &nav.grid;
    let scratch = &mut *scratch;
    for &(entity, _) in &due.0 {
        let Ok((body, agent, mut path, collider)) = agents.get_mut(entity) else {
            continue;
        };
        let Some(goal_pos) = path.target_pos else {
            if path.status != NavStatus::Idle {
                *path = NavPath::default();
            }
            continue;
        };
        let goal_tile = (tile_of(goal_pos.x), tile_of(goal_pos.y));
        let stale = path.status == NavStatus::Idle
            || path.goal_tile != goal_tile
            || path.map_version != nav.version
            || path.profile != agent.profile()
            || path.replan;
        if !stale {
            continue;
        }

        let start_tile = (tile_of(body.0.pos.x), tile_of(body.0.pos.y));
        let start = grid.nearest_walkable(start_tile, &agent.filter, 2);
        let goal = grid.nearest_walkable(goal_tile, &agent.filter, NEAREST_RADIUS);
        let (Some(start), Some(goal)) = (start, goal) else {
            path.fail(goal_tile, nav.version, agent.profile());
            continue;
        };
        let field = fields.get(&nav, &agent.filter, &agent.costs);
        let outcome = scratch.finder.find_ranged(
            grid.width(),
            grid.height(),
            field,
            agent.range,
            start,
            goal,
            MAX_EXPANSIONS,
            &mut scratch.tiles,
        );
        if !matches!(outcome, PathOutcome::Found { .. }) {
            path.fail(goal_tile, nav.version, agent.profile());
            continue;
        }

        let fp = collider.map_or(Footprint::DEFAULT, |c| c.footprint);
        // A straight leg may only cross plain ground. The search may have
        // gone out of its way to avoid a costly tile, and cutting across it to
        // save a corner would undo that.
        let (gw, gh) = (grid.width() as i32, grid.height() as i32);
        let tile_ok = |x: i32, y: i32| {
            x >= 0
                && y >= 0
                && x < gw
                && y < gh
                && field[(y * gw + x) as usize] == u16::from(GROUND_COST)
        };
        if path.raw {
            raw_path(&scratch.tiles, &mut scratch.legs);
        } else {
            smooth_path(
                &solids.0,
                &fp,
                &tile_ok,
                &scratch.tiles,
                SMOOTH_LOOKAHEAD,
                &mut scratch.legs,
            );
        }
        let mut legs = std::mem::take(&mut path.waypoints);
        legs.clear();
        // Standing on a tile it could not stand on (something was painted
        // under it): first get onto the tile the route starts from.
        if start != start_tile {
            legs.push(Vec2i::new(
                start.0.saturating_mul(SUBUNITS_PER_TILE),
                start.1.saturating_mul(SUBUNITS_PER_TILE),
            ));
        }
        legs.extend_from_slice(&scratch.legs);
        // End on the target itself, not the middle of its tile, when the
        // target's own tile is where the route ends.
        if goal == goal_tile && !matches!(agent.target, NavTarget::Tile(..)) {
            match legs.last_mut() {
                Some(last) => *last = goal_pos,
                None => legs.push(goal_pos),
            }
        }
        path.waypoints = legs;
        path.next = 0;
        path.status = NavStatus::Following;
        if path.goal_tile != goal_tile {
            path.strikes = 0;
            path.raw = false;
        }
        path.goal_tile = goal_tile;
        path.map_version = nav.version;
        path.profile = agent.profile();
        path.replan = false;
        path.stuck_ticks = 0;
        path.stuck_from = body.0.pos;
    }
}

/// Which direction keys head from `from` toward `to`.
///
/// The larger axis is pressed every tick. The smaller one is pressed on a duty
/// cycle equal to the ratio of the two distances (an integer line-drawing
/// accumulator), so the average heading is the true heading, however shallow,
/// instead of snapping to eight directions. Without that, a small sideways
/// error at the mouth of a one-tile corridor is never corrected and the body
/// clips the corner.
fn steer(from: Vec2i, to: Vec2i, acc: &mut i64) -> MoveInput {
    const DEAD: i64 = 96;
    let dx = i64::from(to.x) - i64::from(from.x);
    let dy = i64::from(to.y) - i64::from(from.y);
    let (ax, ay) = (dx.abs(), dy.abs());
    let major = ax.max(ay);
    if major <= DEAD {
        *acc = 0;
        return MoveInput::NONE;
    }
    let minor = ax.min(ay);
    *acc += minor;
    let press_minor = *acc >= major;
    if press_minor {
        *acc -= major;
    }
    let dir = |d: i64, positive: MoveInput, negative: MoveInput| {
        if d > 0 { positive } else { negative }
    };
    let (x_pressed, y_pressed) = if ax >= ay {
        (true, press_minor)
    } else {
        (press_minor, true)
    };
    let mut input = MoveInput::NONE;
    if x_pressed && ax > 0 {
        input = input.union(dir(dx, MoveInput::RIGHT, MoveInput::LEFT));
    }
    if y_pressed && ay > 0 {
        input = input.union(dir(dy, MoveInput::UP, MoveInput::DOWN));
    }
    input
}

fn dist2(a: Vec2i, b: Vec2i) -> i64 {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dy = i64::from(a.y) - i64::from(b.y);
    dx * dx + dy * dy
}

type StepQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut SimBody,
        &'static NavAgent,
        &'static mut NavPath,
        Option<&'static Collider>,
    ),
>;

/// Every tick, for every agent: steer along the route and move.
fn step_agents(nav: Option<Res<NavMap>>, solids: Option<Res<SolidMap>>, mut agents: StepQuery) {
    let Some(nav) = nav else { return };
    for (mut body, agent, mut path, collider) in &mut agents {
        let target_pos = path.target_pos;
        let fp = collider.map_or(Footprint::DEFAULT, |c| c.footprint);
        let pos = body.0.pos;
        let mut input = MoveInput::NONE;

        if let Some(goal) = target_pos {
            let fixed_tile = matches!(agent.target, NavTarget::Tile(..));
            let stop = if fixed_tile {
                TILE_ARRIVE_RADIUS
            } else {
                i64::from(agent.stop_distance)
            };
            let tile_ok = |x: i32, y: i32| nav.grid.walkable(x, y, &agent.filter);
            let close_and_clear = dist2(pos, goal) <= stop * stop
                && solids
                    .as_ref()
                    .is_none_or(|s| segment_clear(&s.0, &fp, pos, goal, &tile_ok));
            if path.status == NavStatus::Following || path.status == NavStatus::Arrived {
                if close_and_clear {
                    path.status = NavStatus::Arrived;
                    path.strikes = 0;
                } else {
                    // Walk on: advance past waypoints already reached.
                    while path.next < path.waypoints.len()
                        && dist2(pos, path.waypoints[path.next])
                            <= WAYPOINT_RADIUS * WAYPOINT_RADIUS
                    {
                        path.next += 1;
                    }
                    if path.next >= path.waypoints.len() {
                        // Out of waypoints but not there: the target moved
                        // inside its tile, or we stopped short. Head straight
                        // for it; planning will refine when due.
                        input = steer(pos, goal, &mut path.steer_acc);
                        if path.status == NavStatus::Arrived {
                            path.status = NavStatus::Following;
                        }
                        if dist2(pos, goal) <= TILE_ARRIVE_RADIUS * TILE_ARRIVE_RADIUS {
                            path.status = NavStatus::Arrived;
                            input = MoveInput::NONE;
                        }
                    } else {
                        path.status = NavStatus::Following;
                        let to = path.waypoints[path.next];
                        input = steer(pos, to, &mut path.steer_acc);
                    }
                }
            }
        }

        // Stuck detection: asked to move, and not getting anywhere.
        if path.status == NavStatus::Following && input != MoveInput::NONE {
            let from = path.stuck_from;
            if (pos.x - from.x).abs().max((pos.y - from.y).abs()) >= PROGRESS {
                path.stuck_from = pos;
                path.stuck_ticks = 0;
            } else {
                path.stuck_ticks += 1;
                if path.stuck_ticks >= STUCK_TICKS {
                    path.stuck_ticks = 0;
                    path.stuck_from = pos;
                    path.strikes += 1;
                    // First try the plain tile-by-tile route, which hugs
                    // corners less; after repeated failures, stop trying.
                    path.raw = true;
                    path.replan = true;
                    if path.strikes >= MAX_STRIKES {
                        let (goal_tile, version) = (path.goal_tile, path.map_version);
                        path.fail(goal_tile, version, agent.profile());
                    }
                }
            }
        } else if path.status != NavStatus::Following {
            path.stuck_ticks = 0;
            path.stuck_from = pos;
        }

        // Same physics as the player. Work on a copy and write only on
        // change, so a resting agent is not flagged as changed every tick.
        let mut next: Body = body.0;
        match (&solids, collider) {
            (Some(map), Some(c)) => {
                step_body_solid_max(&mut next, input, &map.0, &c.footprint, agent.max_speed);
            }
            _ => step_body_max(&mut next, input, agent.max_speed),
        }
        if next != body.0 {
            body.0 = next;
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    const T: i32 = SUBUNITS_PER_TILE;

    #[test]
    fn speed_conversion_matches_the_players_speed() {
        assert_eq!(speed_from_tiles_per_second(5), ianus_sim::MAX_SPEED);
        assert_eq!(speed_from_tiles_per_second(0), 0);
        assert!(speed_from_tiles_per_second(2) < speed_from_tiles_per_second(3));
        assert_eq!(speed_from_tiles_per_second(u32::MAX), i32::MAX);
    }

    #[test]
    fn steering_heads_the_true_direction_on_average() {
        let o = Vec2i::new(0, 0);
        let mut acc = 0;
        assert_eq!(steer(o, Vec2i::new(5 * T, 0), &mut acc), MoveInput::RIGHT);
        assert_eq!(steer(o, Vec2i::new(0, -3 * T), &mut acc), MoveInput::DOWN);
        // Dead on a diagonal: both keys every tick.
        let mut acc = 0;
        for _ in 0..10 {
            assert_eq!(
                steer(o, Vec2i::new(3 * T, 3 * T), &mut acc),
                MoveInput::RIGHT.union(MoveInput::UP)
            );
        }
        // On the spot: no keys, and the accumulator is cleared.
        acc = 77;
        assert_eq!(steer(o, Vec2i::new(50, -50), &mut acc), MoveInput::NONE);
        assert_eq!(acc, 0);
        // A shallow heading (1 up for every 4 right) presses UP one tick in
        // four, never UP and DOWN, and always RIGHT.
        let mut acc = 0;
        let mut ups = 0;
        for _ in 0..400 {
            let i = steer(o, Vec2i::new(4 * T, T), &mut acc);
            assert!(i.contains(MoveInput::RIGHT) && !i.contains(MoveInput::DOWN));
            ups += i.contains(MoveInput::UP) as i32;
        }
        assert_eq!(ups, 100);
    }
}
