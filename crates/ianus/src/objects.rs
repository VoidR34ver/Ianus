//! Which objects update this tick.
//!
//! Each tick runs in phases (see [`IanusSet`](crate::IanusSet)):
//!
//! 1. **World**: level-wide settings are in force and the collision structures
//!    are brought up to date.
//! 2. **Objects**: [`DueObjects`] is filled with exactly the objects whose
//!    `ob_tick` makes them due this tick. Objects that are not due are never
//!    visited, not even to be skipped (see [`ianus_sim::Scheduler`]).
//! 3. **Sim**: movement, and anything the game adds. Movement and collision
//!    always run every tick, whatever an object's `ob_tick` is.
//! 4. **Detect**: what touches what.
//!
//! This module owns phase 2. A game's own object logic goes in `IanusSet::Sim`
//! and iterates [`DueObjects`] instead of every object.

use bevy_app::{App, FixedUpdate};
use bevy_ecs::prelude::*;
use ianus_sim::Scheduler;

use crate::tick::{IanusSet, Tick};

/// Every object that has an update rate, filed so that finding the due ones is
/// a lookup, not a scan.
#[derive(Resource, Default, Debug, Clone)]
pub struct ObjectSchedule {
    sched: Scheduler,
    entities: Vec<Entity>,
}

impl ObjectSchedule {
    /// Schedule `entity` to update every `ob_tick` ticks (`0` is never).
    pub fn add(&mut self, entity: Entity, ob_tick: u32) {
        let id = self.entities.len() as u32;
        self.entities.push(entity);
        self.sched.insert(id, ob_tick);
    }

    /// How many objects update at some rate.
    pub fn scheduled(&self) -> usize {
        self.sched.scheduled()
    }

    /// How many objects never update (`ob_tick` 0).
    pub fn never(&self) -> usize {
        self.sched.never()
    }

    /// How many objects are due on `tick`, without visiting them.
    pub fn due_count(&self, tick: u64) -> usize {
        self.sched.due_count(tick)
    }
}

/// The objects due this tick, with how many ticks have passed since each one
/// last updated (so its logic can catch up in one go). Refilled every tick;
/// the vector is reused, so this does not allocate once warmed up.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct DueObjects(pub Vec<(Entity, u32)>);

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<DueObjects>()
        .add_systems(FixedUpdate, collect_due.in_set(IanusSet::Objects));
}

fn collect_due(
    alive: Query<()>,
    tick: Res<Tick>,
    schedule: Option<Res<ObjectSchedule>>,
    mut due: ResMut<DueObjects>,
) {
    due.0.clear();
    let Some(schedule) = schedule else { return };
    schedule.sched.due(tick.0, |id, elapsed| {
        let entity = schedule.entities[id as usize];
        // Despawned during play: still filed, but not handed out.
        if alive.contains(entity) {
            due.0.push((entity, elapsed));
        }
    });
}
