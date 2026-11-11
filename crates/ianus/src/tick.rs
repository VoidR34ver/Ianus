use bevy_app::{App, FixedUpdate, RunFixedMainLoop, RunFixedMainLoopSystems};
use bevy_ecs::prelude::*;
use bevy_time::{Fixed, Time, Virtual};
use ianus_sim::{MoveInput, Replay, Rng, TICK_HZ};

use crate::body::SimBody;
use crate::input::MoveIntent;

/// The phases of a simulation tick, in `FixedUpdate`, in this order:
///
/// 1. `Input`: decide what input this tick uses (live keys, or a replay).
/// 2. `World`: level-wide settings are in force; collision structures are
///    brought up to date.
/// 3. `Objects`: work out which objects are due this tick
///    ([`DueObjects`](crate::DueObjects)); the rest are never touched.
/// 4. `Sim`: where simulation systems go. Ianus's own movement runs here, every
///    tick. Order against it with `.before` / `.after`.
/// 5. `Detect`: what touches what ([`Touching`](crate::Touching)).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum IanusSet {
    Input,
    World,
    Objects,
    Sim,
    Detect,
}

/// Number of completed simulation ticks.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tick(pub u64);

/// The input every simulation system must use for the current tick.
///
/// Systems read this, never the keyboard, so live play and replay playback
/// run exactly the same code.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TickInput(pub MoveInput);

/// The simulation's random numbers. Seeded; part of the replay.
#[derive(Resource, Debug, Clone, Copy)]
pub struct SimRng(pub Rng);

impl Default for SimRng {
    fn default() -> Self {
        Self(Rng::new(0))
    }
}

/// Whether ticks come from the keyboard, are being recorded, or come from a
/// replay.
#[derive(Resource, Debug, Default)]
pub enum ReplayMode {
    /// Live play, nothing recorded.
    #[default]
    Off,
    /// Live play; every tick's input is appended.
    Recording(Replay),
    /// Input comes from the replay. When it runs out, the mode becomes
    /// [`ReplayMode::Finished`].
    Playing(Replay),
    /// Playback ended. `matched` is `Some(true/false)` if the replay carried a
    /// final hash to compare against; `None` if it did not.
    Finished { matched: Option<bool> },
}

/// Pause, step and speed. The engine's window layer maps keys onto this; tests
/// and tools can drive it directly.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct SimControl {
    pub paused: bool,
    /// Run exactly this many ticks even while paused, then stop. Cleared as
    /// they are used.
    pub step_requests: u32,
    /// Game speed multiplier. 1.0 is real time.
    pub speed: f32,
}

impl Default for SimControl {
    fn default() -> Self {
        Self {
            paused: false,
            step_requests: 0,
            speed: 1.0,
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.insert_resource(Time::<Fixed>::from_hz(f64::from(TICK_HZ)))
        .init_resource::<Tick>()
        .init_resource::<TickInput>()
        .init_resource::<SimRng>()
        .init_resource::<ReplayMode>()
        .init_resource::<SimControl>()
        .configure_sets(
            FixedUpdate,
            (
                IanusSet::Input,
                IanusSet::World,
                IanusSet::Objects,
                IanusSet::Sim,
                IanusSet::Detect,
            )
                .chain(),
        )
        .add_systems(FixedUpdate, resolve_tick_input.in_set(IanusSet::Input))
        // Counted last so systems in the set see the tick they are running.
        .add_systems(FixedUpdate, advance_tick.after(IanusSet::Detect))
        .add_systems(
            RunFixedMainLoop,
            apply_sim_control.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
        );
}

/// Decide this tick's input: from the replay if one is playing, otherwise from
/// the live intent (and append it if recording).
///
/// When playback runs out, the world is exactly as it was after the last
/// recorded tick, so that is the moment to compare against the recorded hash.
/// The tick that discovers the end still runs, with no input, and live input
/// takes over from the next one.
fn resolve_tick_input(
    tick: Res<Tick>,
    mut intent: ResMut<MoveIntent>,
    mut mode: ResMut<ReplayMode>,
    mut out: ResMut<TickInput>,
    bodies: Query<&SimBody>,
) {
    // Always consume the live intent, so a latched press cannot linger and
    // fire later when playback ends.
    let live = intent.take();

    let mut finished: Option<Option<bool>> = None;
    let input = match &mut *mode {
        ReplayMode::Playing(replay) => match replay.input_at(tick.0) {
            Some(i) => i,
            None => {
                let now = world_hash(bodies.iter());
                finished = Some(replay.final_hash.map(|h| h == now));
                MoveInput::NONE
            }
        },
        ReplayMode::Recording(replay) => {
            replay.push(live);
            live
        }
        _ => live,
    };
    if let Some(matched) = finished {
        *mode = ReplayMode::Finished { matched };
    }
    if out.0 != input {
        out.0 = input;
    }
}

/// Fingerprint of all simulated bodies. Order-independent (sum of per-body
/// hashes), so it does not depend on entity iteration order.
pub fn world_hash<'a>(bodies: impl Iterator<Item = &'a SimBody>) -> u64 {
    bodies.fold(0u64, |acc, b| acc.wrapping_add(b.0.state_hash()))
}

/// Start recording from tick 0 with the given seed.
///
/// Playback only reproduces the session if it starts from the same initial
/// world, so call this when the game (re)starts, after spawning its entities.
pub fn begin_recording(world: &mut World, seed: u64) {
    world.insert_resource(Tick(0));
    world.insert_resource(SimRng(Rng::new(seed)));
    world.insert_resource(ReplayMode::Recording(Replay::new(seed)));
}

/// Stop recording and return the replay, stamped with the world's current
/// hash. `None` if nothing was being recorded.
pub fn finish_recording(world: &mut World) -> Option<Replay> {
    let hash = {
        let mut q = world.query::<&SimBody>();
        world_hash(q.iter(world))
    };
    match std::mem::take(&mut *world.resource_mut::<ReplayMode>()) {
        ReplayMode::Recording(mut replay) => {
            replay.final_hash = Some(hash);
            Some(replay)
        }
        other => {
            *world.resource_mut::<ReplayMode>() = other;
            None
        }
    }
}

/// Start playing a replay from tick 0. The caller must first put the world
/// back into the state it was in when recording began.
pub fn begin_playback(world: &mut World, replay: Replay) {
    world.insert_resource(Tick(0));
    world.insert_resource(SimRng(Rng::new(replay.seed)));
    world.insert_resource(ReplayMode::Playing(replay));
}

fn advance_tick(mut tick: ResMut<Tick>) {
    tick.0 += 1;
}

/// Turn [`SimControl`] into Bevy clock settings, before the fixed loop runs.
fn apply_sim_control(
    mut control: ResMut<SimControl>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut fixed: ResMut<Time<Fixed>>,
) {
    let speed = control.speed.clamp(0.0, 16.0);
    if virtual_time.relative_speed() != speed {
        virtual_time.set_relative_speed(speed);
    }

    if control.paused {
        if !virtual_time.is_paused() {
            virtual_time.pause();
        }
        if control.step_requests > 0 {
            // A paused clock adds no time, so adding exactly one timestep
            // makes exactly one tick run.
            let step = fixed.timestep();
            fixed.accumulate_overstep(step);
            control.step_requests -= 1;
        }
    } else if virtual_time.is_paused() {
        virtual_time.unpause();
    }
}
