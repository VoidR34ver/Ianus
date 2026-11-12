//! Ianus runtime: the Bevy side of the engine.
//!
//! [`IanusPlugin`] wires three things together:
//!
//! 1. A fixed simulation tick ([`ianus_sim::TICK_HZ`]), independent of the
//!    frame rate.
//! 2. Keyboard input turned into a per-tick [`MoveIntent`], with *latching*
//!    so a key tapped between two ticks is never lost.
//! 3. Interpolated rendering: entities with a [`SimBody`] are drawn at a
//!    point between their previous and current tick, so motion is smooth at
//!    any refresh rate while the simulation itself stays on a fixed grid.
//!
//! Requires `bevy_time::TimePlugin` (part of Bevy's default plugins). Without
//! a `ButtonInput<KeyCode>` resource (headless runs, servers, replays) input
//! is simply skipped and intent can be fed in by hand.

mod body;
mod collide;
mod input;
mod nav;
mod objects;
mod play;
mod room;
mod session;
mod tick;

use bevy_app::{App, Plugin};

pub use body::{Controlled, PrevPos, SimBody, WorldScale};
pub use collide::{
    Collider, ColliderIndex, SolidMap, SolidShape, Touching, WorldSettings, classify, footprint_of,
};
pub use input::{MoveBindings, MoveIntent};
pub use nav::{
    MAX_EXPANSIONS, NavAgent, NavMap, NavPath, NavStatus, NavTags, NavTarget, PaintError, PaintOp,
    paint_nav_tile, speed_from_tiles_per_second,
};
pub use objects::{DueObjects, ObjectSchedule};
pub use play::{PlayMode, play, step_once, stop};
pub use room::{
    CurrentRoom, FileChange, FileWatch, LoadError, PlacedObject, load_room, load_room_file,
};
pub use session::{IanusAppExt, RestartHook, restart_world, start_playback, start_recording};
pub use tick::{
    IanusSet, ReplayMode, SimControl, SimRng, Tick, TickInput, begin_playback, begin_recording,
    finish_recording, world_hash,
};

/// Everything most users import.
pub mod prelude {
    pub use crate::{
        Collider, ColliderIndex, Controlled, CurrentRoom, DueObjects, FileChange, FileWatch,
        IanusAppExt, IanusPlugin, IanusSet, LoadError, MoveBindings, MoveIntent, NavAgent, NavMap,
        NavPath, NavStatus, NavTags, NavTarget, ObjectSchedule, PaintError, PaintOp, PlacedObject,
        PlayMode, PrevPos, ReplayMode, SimBody, SimControl, SimRng, SolidMap, Tick, TickInput,
        Touching, WorldScale, WorldSettings, begin_playback, begin_recording, finish_recording,
        footprint_of, load_room, load_room_file, paint_nav_tile, play, restart_world,
        start_playback, start_recording, step_once, stop, world_hash,
    };
    pub use ianus_objects::{
        Action, Category, Hitbox, Kind, ObjectDef, Problem, Room, Severity, Substance, WorldRules,
    };
    pub use ianus_sim::{
        Aabb, Body, Footprint, MoveInput, NavCosts, NavFilter, Replay, Rng, SUBUNITS_PER_TILE,
        SearchRange, TICK_HZ, Vec2i, goal_distance_radius,
    };
}

/// Adds the Ianus tick, movement input and render interpolation.
pub struct IanusPlugin;

impl Plugin for IanusPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            tick::plugin,
            input::plugin,
            body::plugin,
            play::plugin,
            collide::plugin,
            objects::plugin,
            nav::plugin,
        ));
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod nav_tests;

#[cfg(test)]
mod room_tests;
