//! Edit mode and play mode, with Stop putting everything back.
//!
//! Pressing Play remembers where every simulated thing is. Stop puts them all
//! back, resets the tick and the random numbers, and drops anything the game
//! spawned while playing. Because the simulation is deterministic and its
//! whole state is small, this is exact, not approximate.
//!
//! Limits, stated plainly: only [`SimBody`] positions and velocities, the
//! tick counter and the RNG are restored. Entities that existed at Play and
//! were despawned during play are not brought back, and other components a
//! game adds are not snapshotted.

use std::collections::HashSet;

use bevy_app::App;
use bevy_ecs::prelude::*;
use ianus_sim::Body;

use crate::body::{PrevPos, SimBody};
use crate::input::MoveIntent;
use crate::tick::{ReplayMode, SimControl, SimRng, Tick, TickInput};

/// Whether the world is being edited or the game is running.
///
/// Defaults to `Playing`, so an app that never loads a room behaves exactly as
/// before. Loading a room switches to `Editing`.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    #[default]
    Playing,
    Editing,
}

/// What Stop restores.
#[derive(Resource)]
struct PlaySnapshot {
    bodies: Vec<(Entity, Body)>,
    tick: Tick,
    rng: SimRng,
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<PlayMode>();
}

/// Leave edit mode and start running. Returns `false` if already playing.
pub fn play(world: &mut World) -> bool {
    if *world.resource::<PlayMode>() != PlayMode::Editing {
        return false;
    }
    let bodies = {
        let mut q = world.query::<(Entity, &SimBody)>();
        q.iter(world).map(|(e, b)| (e, b.0)).collect()
    };
    let tick = *world.resource::<Tick>();
    let rng = *world.resource::<SimRng>();
    world.insert_resource(PlaySnapshot { bodies, tick, rng });

    // Start playing with no leftover key state from the editor.
    world.insert_resource(MoveIntent::default());
    world.insert_resource(TickInput::default());
    *world.resource_mut::<PlayMode>() = PlayMode::Playing;
    world.resource_mut::<SimControl>().paused = false;
    true
}

/// Stop playing and restore the world to how it was at Play. Returns `false`
/// if there is nothing to restore (not playing, or no snapshot was taken).
pub fn stop(world: &mut World) -> bool {
    if *world.resource::<PlayMode>() != PlayMode::Playing {
        return false;
    }
    let Some(snapshot) = world.remove_resource::<PlaySnapshot>() else {
        return false;
    };

    let kept: HashSet<Entity> = snapshot.bodies.iter().map(|&(e, _)| e).collect();
    let extras: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<SimBody>>();
        q.iter(world).filter(|e| !kept.contains(e)).collect()
    };
    for entity in extras {
        let _ = world.despawn(entity);
    }
    for (entity, body) in snapshot.bodies {
        if let Some(mut b) = world.get_mut::<SimBody>(entity) {
            b.0 = body;
        }
        if let Some(mut p) = world.get_mut::<PrevPos>(entity) {
            p.0 = body.pos;
        }
    }

    world.insert_resource(snapshot.tick);
    world.insert_resource(snapshot.rng);
    world.insert_resource(ReplayMode::Off);
    world.insert_resource(MoveIntent::default());
    world.insert_resource(TickInput::default());
    *world.resource_mut::<PlayMode>() = PlayMode::Editing;
    let mut control = world.resource_mut::<SimControl>();
    control.paused = true;
    control.step_requests = 0;
    true
}

/// Run exactly one tick. From edit mode this first enters play mode (paused),
/// so Stop can still undo it.
pub fn step_once(world: &mut World) {
    if *world.resource::<PlayMode>() == PlayMode::Editing {
        play(world);
    }
    let mut control = world.resource_mut::<SimControl>();
    control.paused = true;
    control.step_requests = control.step_requests.saturating_add(1);
}
