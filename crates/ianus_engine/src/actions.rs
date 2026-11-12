//! Everything the user can ask the engine to do, as data.
//!
//! Keyboard shortcuts and GUI buttons both push an [`Action`]; one exclusive
//! system carries them out. That way there is exactly one implementation of
//! "record" or "step", the shortcut and the button cannot drift apart, and
//! actions that need the whole world (running the game's restart hook) have it.

use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use ianus::prelude::*;

use crate::{EngineConfig, EngineStatus};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Leave edit mode and run the game.
    Play,
    /// Stop the game and put everything back as it was at Play.
    Stop,
    /// Load the room file again.
    ReloadRoom,
    /// Pause or resume. In edit mode this starts playing.
    TogglePause,
    /// Run exactly one tick (pauses first; enters play mode from edit mode).
    Step,
    SetSpeed(f32),
    ToggleRecording,
    PlaySavedReplay,
    /// Put the world back to its starting state and tick 0.
    Restart,
    ToggleGui,
}

/// Actions waiting to be carried out at the next opportunity.
#[derive(Resource, Default, Debug)]
pub struct Actions(Vec<Action>);

impl Actions {
    pub fn push(&mut self, action: Action) {
        self.0.push(action);
    }
}

/// Which entity the inspector shows.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct Selection(pub Option<Entity>);

/// Whether the editor panels are shown.
#[derive(Resource, Debug, Clone, Copy)]
pub struct GuiVisible(pub bool);

impl Default for GuiVisible {
    fn default() -> Self {
        Self(true)
    }
}

pub(crate) fn apply_actions(world: &mut World) {
    let queued = std::mem::take(&mut world.resource_mut::<Actions>().0);
    for action in queued {
        match action {
            Action::Play => {
                if play(world) {
                    world.resource_mut::<EngineStatus>().say("playing");
                }
            }
            Action::Stop => {
                let message = if stop(world) {
                    "stopped: everything is back as it was when you pressed Play"
                } else {
                    "nothing to stop"
                };
                world.resource_mut::<EngineStatus>().say(message);
            }
            Action::ReloadRoom => {
                let message = crate::rooms::reload_now(world);
                if message != "reloaded" {
                    world.resource_mut::<EngineStatus>().say(message);
                }
            }
            Action::TogglePause => {
                // From edit mode, "resume" means "start playing".
                if !play(world) {
                    let mut c = world.resource_mut::<SimControl>();
                    c.paused = !c.paused;
                }
            }
            Action::Step => step_once(world),
            Action::SetSpeed(speed) => world.resource_mut::<SimControl>().speed = speed,
            Action::ToggleGui => {
                let mut v = world.resource_mut::<GuiVisible>();
                v.0 = !v.0;
            }
            Action::ToggleRecording => {
                let message = toggle_recording(world);
                world.resource_mut::<EngineStatus>().say(message);
            }
            Action::PlaySavedReplay => {
                let message = play_saved_replay(world);
                world.resource_mut::<EngineStatus>().say(message);
            }
            Action::Restart => {
                let recording = matches!(*world.resource::<ReplayMode>(), ReplayMode::Recording(_));
                let message = match restart_world(world) {
                    Ok(()) => {
                        world.insert_resource(Tick(0));
                        // Same start as loading a room: tick 0, seed 0.
                        world.insert_resource(SimRng::default());
                        world.insert_resource(ReplayMode::Off);
                        if recording {
                            "world restarted (the recording in progress was discarded)".to_string()
                        } else {
                            "world restarted".to_string()
                        }
                    }
                    Err(e) => format!("restart hook failed: {e}"),
                };
                world.resource_mut::<EngineStatus>().say(message);
            }
        }
    }
}

fn toggle_recording(world: &mut World) -> String {
    // Recording needs the simulation running, so leave edit mode first.
    play(world);
    if matches!(*world.resource::<ReplayMode>(), ReplayMode::Recording(_)) {
        let Some(replay) = finish_recording(world) else {
            return "nothing was recording".into();
        };
        let path = world.resource::<EngineConfig>().replay_path.clone();
        return match std::fs::write(&path, replay.to_bytes()) {
            Ok(()) => format!("saved {} ({} ticks)", path.display(), replay.len()),
            Err(e) => format!("could not save {}: {e}", path.display()),
        };
    }
    // The seed only needs to differ between recordings; it is stored in the
    // replay, so playback is still exact.
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    match start_recording(world, seed) {
        Ok(()) => "recording (press record again to stop and save)".into(),
        Err(e) => format!("restart hook failed: {e}"),
    }
}

fn play_saved_replay(world: &mut World) -> String {
    play(world);
    let path = world.resource::<EngineConfig>().replay_path.clone();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return format!("cannot read {}: {e}", path.display()),
    };
    let replay = match Replay::from_bytes(&bytes) {
        Ok(r) => r,
        Err(e) => return format!("{}: {e}", path.display()),
    };
    let ticks = replay.len();
    match start_playback(world, replay) {
        Ok(()) => format!("playing {ticks} ticks from {}", path.display()),
        Err(e) => format!("restart hook failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_queue_in_order() {
        let mut a = Actions::default();
        a.push(Action::Step);
        a.push(Action::SetSpeed(2.0));
        assert_eq!(a.0, vec![Action::Step, Action::SetSpeed(2.0)]);
    }
}
