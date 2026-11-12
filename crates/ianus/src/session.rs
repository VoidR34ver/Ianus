//! Starting recordings and playbacks from a known world.
//!
//! A replay reproduces a session only if it starts from the same world the
//! recording started from. The engine cannot know how to rebuild an arbitrary
//! game, so the game registers a *restart hook*: one system that puts every
//! simulated thing back to its starting state. Recording and playback call it
//! first.

use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::system::{RegisteredSystemError, SystemId};
use ianus_sim::{Body, Replay};

use crate::body::{PrevPos, SimBody};
use crate::input::MoveIntent;
use crate::nav::reset_agents;
use crate::room::PlacedObject;
use crate::tick::{TickInput, begin_playback, begin_recording};

/// The game's "put the world back to its start" system, if it set one.
#[derive(Resource, Default)]
pub struct RestartHook(Option<SystemId>);

pub trait IanusAppExt {
    /// Register the system that resets the game to its starting state.
    ///
    /// It must restore everything the simulation depends on: positions,
    /// velocities, spawned entities, counters. Anything it misses makes
    /// replays diverge (which playback will report).
    fn set_restart_hook<M>(&mut self, system: impl IntoSystem<(), (), M> + 'static) -> &mut Self;
}

impl IanusAppExt for App {
    fn set_restart_hook<M>(&mut self, system: impl IntoSystem<(), (), M> + 'static) -> &mut Self {
        let id = self.world_mut().register_system(system);
        self.world_mut().insert_resource(RestartHook(Some(id)));
        self
    }
}

/// Run the game's restart hook, then clear everything Ianus keeps between
/// ticks so nothing from before the restart leaks in.
pub fn restart_world(world: &mut World) -> Result<(), RegisteredSystemError> {
    // Objects placed from a room know their own starting point, so rooms need
    // no restart hook. The game's hook (if any) runs after and can override.
    {
        let mut placed = world.query::<(&mut SimBody, &PlacedObject)>();
        for (mut body, object) in placed.iter_mut(world) {
            if body.0 != Body::at(object.home) {
                body.0 = Body::at(object.home);
            }
        }
    }
    if let Some(id) = world.get_resource::<RestartHook>().and_then(|h| h.0) {
        world.run_system(id)?;
    }
    world.insert_resource(MoveIntent::default());
    world.insert_resource(TickInput::default());
    reset_agents(world);
    // Without this a restarted body would visibly glide from where it was.
    let mut bodies = world.query::<(&SimBody, &mut PrevPos)>();
    for (body, mut prev) in bodies.iter_mut(world) {
        prev.0 = body.0.pos;
    }
    Ok(())
}

/// Restart the world, then start recording from tick 0.
pub fn start_recording(world: &mut World, seed: u64) -> Result<(), RegisteredSystemError> {
    restart_world(world)?;
    begin_recording(world, seed);
    Ok(())
}

/// Restart the world, then play `replay` back from tick 0.
pub fn start_playback(world: &mut World, replay: Replay) -> Result<(), RegisteredSystemError> {
    restart_world(world)?;
    begin_playback(world, replay);
    Ok(())
}
