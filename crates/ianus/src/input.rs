use bevy_app::{App, RunFixedMainLoop, RunFixedMainLoopSystems};
use bevy_ecs::prelude::*;
use bevy_input::{ButtonInput, keyboard::KeyCode};
use ianus_sim::MoveInput;

/// What the movement keys asked for, as seen by the simulation.
///
/// Two parts, because frames and ticks do not line up:
///
/// * `held`: keys currently down. Persists across ticks.
/// * `latched`: keys *pressed* since the last tick, even if already released.
///   Consumed by the first tick that runs. Without this, a quick tap that
///   starts and ends between two ticks would vanish.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MoveIntent {
    held: MoveInput,
    latched: MoveInput,
}

impl MoveIntent {
    /// Keys currently held.
    pub fn held(&self) -> MoveInput {
        self.held
    }

    /// Replace the held keys. For AI, replays, gamepads and tests.
    pub fn set_held(&mut self, held: MoveInput) {
        self.held = held;
    }

    /// Guarantee `input` is seen by the next tick, however briefly it was down.
    pub fn latch(&mut self, input: MoveInput) {
        self.latched = self.latched.union(input);
    }

    /// Input for the tick that is about to run; clears the latch.
    pub(crate) fn take(&mut self) -> MoveInput {
        let input = self.held.union(self.latched);
        self.latched = MoveInput::NONE;
        input
    }
}

/// Which key means which direction. Default: WASD and the arrow keys.
#[derive(Resource, Debug, Clone)]
pub struct MoveBindings {
    pub pairs: Vec<(KeyCode, MoveInput)>,
}

impl Default for MoveBindings {
    fn default() -> Self {
        Self {
            pairs: vec![
                (KeyCode::KeyA, MoveInput::LEFT),
                (KeyCode::KeyD, MoveInput::RIGHT),
                (KeyCode::KeyW, MoveInput::UP),
                (KeyCode::KeyS, MoveInput::DOWN),
                (KeyCode::ArrowLeft, MoveInput::LEFT),
                (KeyCode::ArrowRight, MoveInput::RIGHT),
                (KeyCode::ArrowUp, MoveInput::UP),
                (KeyCode::ArrowDown, MoveInput::DOWN),
            ],
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<MoveIntent>()
        .init_resource::<MoveBindings>()
        // Read input right before the fixed loop of *this* frame. Reading in
        // `Update` instead would make the simulation see input one frame late.
        .add_systems(
            RunFixedMainLoop,
            gather_move_intent.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
        );
}

fn gather_move_intent(
    control: Option<Res<crate::tick::SimControl>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    bindings: Res<MoveBindings>,
    mut intent: ResMut<MoveIntent>,
) {
    let Some(keys) = keys else { return };

    let mut held = MoveInput::NONE;
    let mut pressed = MoveInput::NONE;
    for &(key, dir) in &bindings.pairs {
        if keys.pressed(key) {
            held = held.union(dir);
        }
        if keys.just_pressed(key) {
            pressed = pressed.union(dir);
        }
    }

    // Only write when something changed, so change detection stays useful.
    if intent.held() != held {
        intent.set_held(held);
    }
    // Taps while the game is paused are dropped, otherwise one would fire the
    // moment the game resumed. (A step request counts as running: held keys
    // are always seen, and a tap made for that step is kept.)
    let frozen = control.is_some_and(|c| c.paused && c.step_requests == 0);
    if pressed != MoveInput::NONE && !frozen {
        intent.latch(pressed);
    }
}
