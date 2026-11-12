//! Hitboxes in the running world: who touches whom, and what stops what.
//!
//! Two separate jobs, as in the object model:
//!
//! * **Detection**: [`Touching`] lists what the controlled entity's hitbox
//!   overlaps, found through a spatial hash ([`ianus_sim::BroadPhase`]).
//! * **Stopping**: [`SolidMap`] holds the solid things; the controlled entity is
//!   stopped by them (see `body.rs`). Not every collider is solid.
//!
//! An object only has a [`Collider`] if its own `collision` switch is on **and**
//! the level's collision switch is on. No collider, no hitbox: nothing touches
//! it, nothing stops at it, and it does not stop at anything either.

use bevy_app::{App, FixedUpdate};
use bevy_ecs::prelude::*;
use ianus_objects::{Hitbox, ObjectDef, Room};
use ianus_sim::{Aabb, BroadPhase, Footprint, SUBUNITS_PER_TILE, Solids, Vec2i};

use crate::body::{Controlled, SimBody};
use crate::tick::IanusSet;

/// An object's live hitbox. Present only while collision is effectively on.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Collider {
    pub footprint: Footprint,
    /// Other things stop at this one.
    pub solid: bool,
}

impl Collider {
    /// The hitbox of this collider when its body is at `pos`.
    #[inline]
    pub fn aabb_at(&self, pos: Vec2i) -> Aabb {
        self.footprint.aabb_at(pos.x, pos.y)
    }

    /// The collider for an object definition, or `None` if its collision is
    /// off (or the level's is).
    pub fn for_def(def: &ObjectDef, level_collision: bool) -> Option<Collider> {
        (def.collision && level_collision).then(|| Collider {
            footprint: footprint_of(&def.hitbox),
            solid: def.solid,
        })
    }
}

/// Convert an authored hitbox (hundredths of a tile) to subunits. Integer
/// maths only: 75 becomes exactly half-width 1536.
pub fn footprint_of(h: &Hitbox) -> Footprint {
    let t = i64::from(SUBUNITS_PER_TILE);
    let half = |hundredths: u32| (i64::from(hundredths) * t / 200).min(i64::from(i32::MAX)) as i32;
    let offset =
        |hundredths: i32| (i64::from(hundredths) * t / 100).clamp(-(1 << 30), 1 << 30) as i32;
    Footprint {
        half_x: half(h.width),
        half_y: half(h.height),
        offset_x: offset(h.offset_x),
        offset_y: offset(h.offset_y),
    }
}

/// How a solid collider is stored for fast checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolidShape {
    /// Exactly one tile, on the tile grid: one bit.
    Tile(i32, i32),
    /// Anything else: a box.
    Box(Aabb),
}

/// Decide how to store a solid collider whose body is at `pos`.
pub fn classify(c: &Collider, pos: Vec2i) -> SolidShape {
    let half = SUBUNITS_PER_TILE / 2;
    let aligned = pos.x % SUBUNITS_PER_TILE == 0 && pos.y % SUBUNITS_PER_TILE == 0;
    let f = c.footprint;
    if aligned && f.half_x == half && f.half_y == half && f.offset_x == 0 && f.offset_y == 0 {
        SolidShape::Tile(pos.x / SUBUNITS_PER_TILE, pos.y / SUBUNITS_PER_TILE)
    } else {
        SolidShape::Box(c.aabb_at(pos))
    }
}

/// What stops things in the loaded room.
#[derive(Resource, Debug, Clone)]
pub struct SolidMap(pub Solids);

impl SolidMap {
    /// Build from solid colliders and where their bodies are.
    pub fn build(
        width: u32,
        height: u32,
        solids: impl IntoIterator<Item = (Collider, Vec2i)>,
    ) -> Self {
        let mut map = Solids::new(width, height);
        let mut boxes = Vec::new();
        for (collider, pos) in solids {
            if !collider.solid {
                continue;
            }
            match classify(&collider, pos) {
                SolidShape::Tile(x, y) => map.set_tile(x, y, true),
                SolidShape::Box(b) => boxes.push(b),
            }
        }
        map.set_boxes(boxes);
        Self(map)
    }

    /// Build from a room's definitions and placements (no entities needed).
    pub fn from_room(room: &Room) -> Self {
        let t = SUBUNITS_PER_TILE;
        Self::build(
            room.width,
            room.height,
            room.placed.iter().filter_map(|p| {
                let def = room.find_object(&p.object)?;
                // The controlled entity is never part of the solid map (the
                // per-tick rebuild leaves it out too); otherwise its own start
                // box would linger as a ghost wall.
                if matches!(
                    def.kind,
                    ianus_objects::Kind::Entity {
                        controlled: true,
                        ..
                    }
                ) {
                    return None;
                }
                let c = Collider::for_def(def, room.world.collision)?;
                Some((c, Vec2i::new(p.x.saturating_mul(t), p.y.saturating_mul(t))))
            }),
        )
    }
}

/// Every collider except the controlled one, in a spatial hash, so "what is
/// near the player" is a lookup instead of a scan.
#[derive(Resource)]
pub struct ColliderIndex {
    bp: BroadPhase,
    entities: Vec<Entity>,
}

impl ColliderIndex {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            bp: BroadPhase::for_room(width, height),
            entities: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    fn rebuild(&mut self, items: impl IntoIterator<Item = (Entity, Aabb)>) {
        self.entities.clear();
        let entities = &mut self.entities;
        self.bp.rebuild(items.into_iter().map(|(e, b)| {
            entities.push(e);
            ((entities.len() - 1) as u32, b)
        }));
    }
}

/// What the controlled entity's hitbox overlaps right now, oldest-spawned
/// first. Updated every tick, after movement. Solids it is merely standing
/// flush against are not included: edges that only meet do not overlap.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct Touching(pub Vec<Entity>);

/// The level's settings, as loaded.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorldSettings(pub ianus_objects::WorldRules);

/// The World-phase collision rebuild. Things that depend on its result (the
/// nav map) order themselves after this set; they cannot order against the
/// function itself because it is registered twice.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CollisionRebuilt;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Touching>()
        .init_resource::<WorldSettings>()
        // Rebuilt in the World phase, before anything moves, so a wall that was
        // dragged in the editor is already in place on the first tick of Play.
        .add_systems(
            FixedUpdate,
            rebuild_collision
                .in_set(IanusSet::World)
                .in_set(CollisionRebuilt),
        )
        // Again at the start of Detect, so anything that moved or vanished
        // during Sim is seen by this tick's Touching, not the next one's.
        // Each registration tracks its own change ticks, so neither misses
        // anything; the second is a cheap emptiness check on quiet ticks.
        .add_systems(
            FixedUpdate,
            rebuild_collision
                .in_set(IanusSet::Detect)
                .before(detect_touching),
        )
        .add_systems(FixedUpdate, detect_touching.in_set(IanusSet::Detect));
}

type MovedColliders<'w, 's> =
    Query<'w, 's, &'static Collider, (Without<Controlled>, Changed<SimBody>)>;

/// Rebuild the solid map and the collider index, but only when something that
/// could change them moved (or a room just loaded). Almost every tick nothing
/// did, so this is a cheap emptiness check.
pub(crate) fn rebuild_collision(
    moved: MovedColliders,
    mut removed: RemovedComponents<Collider>,
    colliders: Query<(Entity, &Collider, &SimBody), Without<Controlled>>,
    room: Option<Res<crate::room::CurrentRoom>>,
    mut index: Option<ResMut<ColliderIndex>>,
    mut solids: Option<ResMut<SolidMap>>,
) {
    // A collider that was removed, or whose entity was despawned, changes
    // nothing that `Changed` can see, so ask for those separately. (Always
    // drain the reader so old removals do not trigger a rebuild later.)
    let vanished = removed.read().count() > 0;
    // Only a *solid* collider moving changes what stops things. A non-solid
    // one (a walking creature) only changes the collider index, so the solid
    // map, and everything derived from it such as the nav map, stays put.
    let solid_moved = moved.iter().any(|c| c.solid);
    if moved.is_empty() && !vanished {
        return;
    }
    // `solids.is_some()`, not `as_deref_mut()`: taking a mutable reference
    // marks the resource changed, and everything downstream (the nav map)
    // would then redo its work every time any collider moved.
    let (Some(room), Some(index), true) = (room, index.as_deref_mut(), solids.is_some()) else {
        return;
    };
    index.rebuild(colliders.iter().map(|(e, c, b)| (e, c.aabb_at(b.0.pos))));
    // A vanished collider may have been solid; there is no way to ask.
    if (solid_moved || vanished)
        && let Some(solids) = solids.as_deref_mut()
    {
        *solids = SolidMap::build(
            room.0.width,
            room.0.height,
            colliders.iter().map(|(_, c, b)| (*c, b.0.pos)),
        );
    }
}

fn detect_touching(
    index: Option<Res<ColliderIndex>>,
    player: Query<(&Collider, &SimBody), With<Controlled>>,
    mut touching: ResMut<Touching>,
) {
    touching.0.clear();
    let (Some(index), Some((collider, body))) = (index, player.iter().next()) else {
        return;
    };
    let mine = collider.aabb_at(body.0.pos);
    index
        .bp
        .query(&mine, |i, _| touching.0.push(index.entities[i as usize]));
    // The hash visits cells in a fixed order, but sorting makes the result
    // independent of the grid layout, which is the contract users get.
    touching.0.sort_unstable_by_key(|e| e.index());
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn hitbox_percent_converts_exactly() {
        let f = footprint_of(&Hitbox::square(75));
        assert_eq!(
            (f.half_x, f.half_y),
            (1536, 1536),
            "75% is the classic body size"
        );
        let t = footprint_of(&Hitbox::TILE);
        assert_eq!((t.half_x, t.half_y), (2048, 2048));
        let o = footprint_of(&Hitbox {
            width: 50,
            height: 20,
            offset_x: -25,
            offset_y: 10,
        });
        // 20% of a tile's half-height is 409.6 subunits; integer division
        // rounds down, deterministically.
        assert_eq!(
            (o.half_x, o.half_y, o.offset_x, o.offset_y),
            (1024, 409, -1024, 409)
        );
    }

    #[test]
    fn absurd_hitboxes_do_not_overflow() {
        let f = footprint_of(&Hitbox {
            width: u32::MAX,
            height: u32::MAX,
            offset_x: i32::MAX,
            offset_y: i32::MIN,
        });
        assert_eq!(f.half_x, i32::MAX);
        assert!(f.offset_x > 0 && f.offset_y < 0);
    }

    #[test]
    fn only_exact_whole_tile_solids_use_the_tile_grid() {
        let tile = Collider {
            footprint: footprint_of(&Hitbox::TILE),
            solid: true,
        };
        let t = SUBUNITS_PER_TILE;
        assert_eq!(
            classify(&tile, Vec2i::new(3 * t, 5 * t)),
            SolidShape::Tile(3, 5)
        );
        // Off the grid, a box.
        assert!(matches!(
            classify(&tile, Vec2i::new(3 * t + 1, 5 * t)),
            SolidShape::Box(_)
        ));
        // Smaller than a tile, a box.
        let small = Collider {
            footprint: footprint_of(&Hitbox::square(50)),
            solid: true,
        };
        assert!(matches!(
            classify(&small, Vec2i::new(t, t)),
            SolidShape::Box(_)
        ));
        // Offset, a box.
        let off = Collider {
            footprint: footprint_of(&Hitbox {
                offset_x: 10,
                ..Hitbox::TILE
            }),
            solid: true,
        };
        assert!(matches!(
            classify(&off, Vec2i::new(t, t)),
            SolidShape::Box(_)
        ));
    }

    #[test]
    fn collision_off_means_no_collider_at_all() {
        let mut wall = ObjectDef::new("W", ianus_objects::Category::Wall);
        assert!(Collider::for_def(&wall, true).is_some());
        assert!(Collider::for_def(&wall, false).is_none(), "level switch");
        wall.collision = false;
        assert!(Collider::for_def(&wall, true).is_none(), "object switch");
    }

    #[test]
    fn only_solid_colliders_enter_the_solid_map() {
        let t = SUBUNITS_PER_TILE;
        let solid = Collider {
            footprint: footprint_of(&Hitbox::TILE),
            solid: true,
        };
        let ghost = Collider {
            footprint: footprint_of(&Hitbox::TILE),
            solid: false,
        };
        let map = SolidMap::build(
            8,
            8,
            [
                (solid, Vec2i::new(2 * t, 2 * t)),
                (ghost, Vec2i::new(4 * t, 4 * t)),
            ],
        );
        assert!(map.0.tiles().is_solid(2, 2));
        assert!(!map.0.tiles().is_solid(4, 4));
    }
}
