//! Ianus engine: a ready-to-run 2D window with an editor-style GUI, on top of
//! the [`ianus`] runtime.
//!
//! ```no_run
//! use ianus_engine::Engine;
//! Engine::new("My game").run();
//! ```
//!
//! What you get without writing any code:
//!
//! * A window split like an editor: toolbar on top, **Scene** list on the
//!   left, **Inspector** on the right, **Simulation** panel at the bottom, and
//!   the game in the middle. Click things in the game view to select them.
//! * A 2D camera that always shows the same number of tiles vertically, and
//!   smoothly follows the [`Controlled`] entity.
//! * The fixed-rate deterministic tick, WASD/arrow movement and smooth
//!   interpolated rendering from [`ianus`].
//! * Pause, single-step, speed, record and replay, as buttons and shortcuts:
//!
//! | Key | Does |
//! |-----|------|
//! | F5  | **Play**; press again to **Stop** and put everything back |
//! | F1  | pause / resume the simulation (from edit mode: start playing) |
//! | F2  | advance exactly one tick (from edit mode: plays paused) |
//! | F3  | cycle speed: 1x, 2x, 4x, 0.5x |
//! | F4  | show / hide the editor panels |
//! | F6  | restart the world and play the saved replay back |
//! | F7  | start / stop recording; stopping saves the replay file |
//! | F8  | reload the room file now |
//!
//! Replay needs the game to say how to reset itself; see
//! [`ianus::IanusAppExt::set_restart_hook`].

mod actions;
mod gui;
mod rooms;

use std::collections::VecDeque;
use std::path::PathBuf;

use bevy::camera::{CameraOutputMode, ScalingMode, visibility::RenderLayers};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::{WindowLevel, WindowResolution};
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext};
use ianus::prelude::*;

pub use actions::{Action, Actions, GuiVisible, Selection};

/// Everything a game usually needs from both Ianus and this crate.
pub mod prelude {
    pub use crate::{
        Action, Actions, Engine, EngineCamera, EngineConfig, EnginePlugin, EngineStatus,
        GuiVisible, Selection,
    };
    pub use ianus::prelude::*;
}

/// Settings for the engine window. Also available as a resource at runtime.
#[derive(Resource, Clone, Debug)]
pub struct EngineConfig {
    pub title: String,
    /// Initial window size in logical pixels.
    pub window_size: (u32, u32),
    /// How many tiles fit vertically in the game view, at any window size.
    pub view_height_tiles: f32,
    pub background: Color,
    /// Camera follow stiffness. Higher is snappier; 0 locks the camera to the
    /// target exactly.
    pub camera_follow_speed: f32,
    /// Where record and replay save and load.
    pub replay_path: PathBuf,
    /// Quit by itself after this many frames. For automated checks.
    pub exit_after_frames: Option<u32>,
    /// Save a screenshot at this frame. For automated checks.
    pub screenshot: Option<(u32, PathBuf)>,
    /// Show the editor panels at startup.
    pub gui: bool,
    /// A room file to load at startup and watch for changes. With a room the
    /// game starts in edit mode (paused); without one it just runs.
    pub room_path: Option<PathBuf>,
    /// Start playing straight after the room loads.
    pub play_on_start: bool,
    /// Open the window without taking focus, behind every other window and
    /// out of the taskbar. Automated runs (anything with `exit_after_frames`
    /// or a screenshot) turn this on so they never interrupt what the user is
    /// doing; set it yourself with [`Engine::quiet`].
    pub quiet: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            title: "Ianus".into(),
            window_size: (1280, 720),
            view_height_tiles: 14.0,
            background: Color::srgb(0.07, 0.08, 0.1),
            camera_follow_speed: 14.0,
            replay_path: PathBuf::from("last.ianr"),
            exit_after_frames: None,
            screenshot: None,
            gui: true,
            room_path: None,
            play_on_start: false,
            quiet: false,
        }
    }
}

impl EngineConfig {
    /// The window this config asks for.
    pub(crate) fn window(&self) -> Window {
        let quiet = self.quiet || self.exit_after_frames.is_some() || self.screenshot.is_some();
        Window {
            title: self.title.clone(),
            resolution: WindowResolution::new(self.window_size.0, self.window_size.1),
            // `focused: false` is what makes the OS show the window without
            // activating it; the level and taskbar flags keep it out of the way.
            focused: !quiet,
            window_level: if quiet {
                WindowLevel::AlwaysOnBottom
            } else {
                WindowLevel::Normal
            },
            skip_taskbar: quiet,
            ..default()
        }
    }
}

/// Builder for an engine app.
pub struct Engine {
    config: EngineConfig,
}

impl Engine {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            config: EngineConfig {
                title: title.into(),
                ..EngineConfig::default()
            },
        }
    }

    pub fn window_size(mut self, width: u32, height: u32) -> Self {
        self.config.window_size = (width, height);
        self
    }

    pub fn view_height_tiles(mut self, tiles: f32) -> Self {
        self.config.view_height_tiles = tiles.max(1.0);
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.config.background = color;
        self
    }

    pub fn camera_follow_speed(mut self, speed: f32) -> Self {
        self.config.camera_follow_speed = speed.max(0.0);
        self
    }

    pub fn replay_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.replay_path = path.into();
        self
    }

    /// Start with the editor panels hidden (F4 shows them).
    /// Load this room file at startup and reload it whenever it is saved.
    pub fn room_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.room_path = Some(path.into());
        self
    }

    /// Start running the game right after loading the room, instead of in
    /// edit mode.
    pub fn play_on_start(mut self, on: bool) -> Self {
        self.config.play_on_start = on;
        self
    }

    pub fn gui(mut self, show: bool) -> Self {
        self.config.gui = show;
        self
    }

    /// Open the window without stealing focus (see [`EngineConfig::quiet`]).
    pub fn quiet(mut self, on: bool) -> Self {
        self.config.quiet = on;
        self
    }

    pub fn exit_after_frames(mut self, frames: u32) -> Self {
        self.config.exit_after_frames = Some(frames);
        self
    }

    pub fn screenshot_at_frame(mut self, frame: u32, path: impl Into<PathBuf>) -> Self {
        self.config.screenshot = Some((frame, path.into()));
        self
    }

    /// Build the app without running it, so the game can add its own systems.
    pub fn build(self) -> App {
        let config = self.config;
        let mut app = App::new();
        let image = if std::env::var("IANUS_LINEAR").is_ok() {
            ImagePlugin::default()
        } else {
            ImagePlugin::default_nearest()
        };
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(config.window()),
                    ..default()
                })
                .set(image),
        )
        .add_plugins(EnginePlugin(config));
        app
    }

    pub fn run(self) -> AppExit {
        self.build().run()
    }
}

/// The engine's own systems: cameras, GUI, actions and shortcuts. Added by
/// [`Engine::build`]; add it yourself only if you assemble the app by hand
/// (it expects Bevy's default plugins to be present).
pub struct EnginePlugin(pub EngineConfig);

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone())
            .insert_resource(ClearColor(self.0.background))
            .insert_resource(GuiVisible(self.0.gui))
            .add_plugins((IanusPlugin, EguiPlugin::default()))
            .init_resource::<EngineStatus>()
            .init_resource::<Actions>()
            .init_resource::<Selection>()
            .init_resource::<gui::FrameHistory>()
            .add_systems(Startup, (spawn_cameras, rooms::load_initial_room))
            .add_systems(
                Update,
                (
                    configure_projection,
                    shortcuts,
                    actions::apply_actions,
                    rooms::watch_room_file,
                    rooms::decorate_room_objects,
                    rooms::rebuild_floor,
                    report_playback,
                    follow_camera,
                    gui::record_frame_time,
                    frame_driver,
                )
                    .chain(),
            )
            .add_systems(EguiPrimaryContextPass, gui::gui_system);
    }
}

/// Marks the camera that draws the game.
#[derive(Component)]
pub struct EngineCamera;

/// Short messages for the user ("saved last.ianr"), newest last.
#[derive(Resource, Default, Debug)]
pub struct EngineStatus {
    log: VecDeque<String>,
}

impl EngineStatus {
    const KEEP: usize = 8;

    pub fn say(&mut self, message: impl Into<String>) {
        if self.log.len() >= Self::KEEP {
            self.log.pop_front();
        }
        self.log.push_back(message.into());
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.log.iter().map(String::as_str)
    }
}

pub(crate) const SPEEDS: [f32; 4] = [0.5, 1.0, 2.0, 4.0];

/// The speed after `current` in the F3 cycle (1x, 2x, 4x, 0.5x). Anything not
/// in the cycle goes back to 1x.
pub(crate) fn next_speed(current: f32) -> f32 {
    const CYCLE: [f32; 4] = [1.0, 2.0, 4.0, 0.5];
    match CYCLE.iter().position(|&s| (s - current).abs() < 1e-3) {
        Some(i) => CYCLE[(i + 1) % CYCLE.len()],
        None => CYCLE[0],
    }
}

fn spawn_cameras(
    mut commands: Commands,
    config: Res<EngineConfig>,
    scale: Res<WorldScale>,
    mut egui_settings: ResMut<EguiGlobalSettings>,
) {
    // We attach the GUI to its own camera so the game camera is free to use
    // a smaller viewport (the space between the panels).
    egui_settings.auto_create_primary_context = false;
    // While a text box has focus, keys must not also steer the player.
    egui_settings.enable_absorb_bevy_input_system = true;

    // Plain Camera2d; the projection is adjusted afterwards by
    // `configure_projection` (inserting a custom one at spawn blanked the
    // game view in Bevy 0.20.1; see the note there).
    commands.spawn((EngineCamera, Camera2d));
    let _ = (&config, &scale);

    if std::env::var("IANUS_NO_EGUI_CAMERA").is_ok() {
        return;
    }
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        // Renders nothing of the game, only the GUI, over the top.
        RenderLayers::none(),
        Camera {
            order: 1,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}

/// Make the game camera show a fixed number of tiles vertically.
///
/// Done by editing the projection `Camera2d` created, not by spawning a
/// custom one: the spawn-time version rendered nothing in Bevy 0.20.1.
fn configure_projection(
    config: Res<EngineConfig>,
    scale: Res<WorldScale>,
    mut cameras: Query<&mut Projection, With<EngineCamera>>,
) {
    for mut projection in &mut cameras {
        if let Projection::Orthographic(ortho) = &mut *projection {
            let height = config.view_height_tiles * scale.0;
            let already = matches!(
                ortho.scaling_mode,
                ScalingMode::FixedVertical { viewport_height } if viewport_height == height
            );
            if !already {
                ortho.scaling_mode = ScalingMode::FixedVertical {
                    viewport_height: height,
                };
            }
        }
    }
}

/// Keyboard shortcuts for the same actions the toolbar buttons trigger.
fn shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    control: Res<SimControl>,
    mode: Res<PlayMode>,
    mut actions: ResMut<Actions>,
) {
    if keys.just_pressed(KeyCode::F5) {
        actions.push(match *mode {
            PlayMode::Editing => Action::Play,
            PlayMode::Playing => Action::Stop,
        });
    }
    if keys.just_pressed(KeyCode::F8) {
        actions.push(Action::ReloadRoom);
    }
    if keys.just_pressed(KeyCode::F1) {
        actions.push(Action::TogglePause);
    }
    if keys.just_pressed(KeyCode::F2) {
        actions.push(Action::Step);
    }
    if keys.just_pressed(KeyCode::F3) {
        actions.push(Action::SetSpeed(next_speed(control.speed)));
    }
    if keys.just_pressed(KeyCode::F4) {
        actions.push(Action::ToggleGui);
    }
    if keys.just_pressed(KeyCode::F7) {
        actions.push(Action::ToggleRecording);
    }
    if keys.just_pressed(KeyCode::F6) {
        actions.push(Action::PlaySavedReplay);
    }
}

/// When playback ends, say whether it reproduced the recording.
fn report_playback(mut mode: ResMut<ReplayMode>, mut status: ResMut<EngineStatus>) {
    if let ReplayMode::Finished { matched } = *mode {
        status.say(match matched {
            Some(true) => "replay finished: IDENTICAL to the recording",
            Some(false) => "replay finished: DIVERGED (the world differs from the recording)",
            None => "replay finished (no final state stored to compare)",
        });
        *mode = ReplayMode::Off;
    }
}

/// Where the camera should look. While editing, the middle of the room, so the
/// whole level is in view. While playing, the controlled entity, falling back
/// to the room's middle if nothing is controlled.
pub(crate) fn camera_goal(
    mode: PlayMode,
    controlled: Option<Vec2>,
    room_centre: Option<Vec2>,
) -> Option<Vec2> {
    match (mode, controlled, room_centre) {
        (PlayMode::Editing, _, Some(centre)) => Some(centre),
        (_, Some(player), _) => Some(player),
        (_, None, centre) => centre,
    }
}

/// Ease the camera toward [`camera_goal`]. Uses real time, so it keeps
/// settling while the simulation is paused, and an exponential so the feel is
/// the same at any frame rate.
// A Bevy system: its parameters are its inputs, so the count is not a design smell.
#[allow(clippy::too_many_arguments)]
fn follow_camera(
    time: Res<Time<Real>>,
    config: Res<EngineConfig>,
    scale: Res<WorldScale>,
    mode: Res<PlayMode>,
    room: Option<Res<CurrentRoom>>,
    target: Query<&Transform, (With<Controlled>, Without<EngineCamera>)>,
    mut camera: Query<&mut Transform, With<EngineCamera>>,
    mut placed: Local<bool>,
) {
    let Ok(mut camera) = camera.single_mut() else {
        return;
    };
    let Some(goal) = camera_goal(
        *mode,
        target.iter().next().map(|t| t.translation.truncate()),
        room.as_ref().map(|r| rooms::room_centre(&r.0, scale.0)),
    ) else {
        return;
    };
    let k = if !*placed || config.camera_follow_speed <= 0.0 {
        *placed = true;
        1.0
    } else {
        1.0 - (-time.delta_secs() * config.camera_follow_speed).exp()
    };
    let dx = goal.x - camera.translation.x;
    let dy = goal.y - camera.translation.y;
    if dx != 0.0 || dy != 0.0 {
        camera.translation.x += dx * k;
        camera.translation.y += dy * k;
    }
}

/// Automated-check hooks: take a screenshot and/or quit after N frames.
fn frame_driver(
    mut frame: Local<u32>,
    config: Res<EngineConfig>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;
    if let Some((at, path)) = &config.screenshot
        && *frame == *at
    {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
    }
    if let Some(limit) = config.exit_after_frames
        && *frame >= limit
    {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_cycles_through_the_list_and_wraps() {
        assert_eq!(next_speed(1.0), 2.0);
        assert_eq!(next_speed(2.0), 4.0);
        assert_eq!(next_speed(4.0), 0.5);
        assert_eq!(next_speed(0.5), 1.0);
    }

    #[test]
    fn unknown_speed_resets_to_normal() {
        assert_eq!(next_speed(3.0), 1.0);
        assert_eq!(next_speed(0.0), 1.0);
    }

    #[test]
    fn builder_clamps_nonsense() {
        let e = Engine::new("t")
            .view_height_tiles(-5.0)
            .camera_follow_speed(-1.0);
        assert_eq!(e.config.view_height_tiles, 1.0);
        assert_eq!(e.config.camera_follow_speed, 0.0);
    }

    #[test]
    fn camera_frames_the_room_while_editing_and_follows_the_player_while_playing() {
        let player = Some(Vec2::new(10.0, 20.0));
        let centre = Some(Vec2::new(300.0, 200.0));
        assert_eq!(camera_goal(PlayMode::Editing, player, centre), centre);
        assert_eq!(camera_goal(PlayMode::Playing, player, centre), player);
        // No player: always the room. No room: the player. Neither: nothing.
        assert_eq!(camera_goal(PlayMode::Playing, None, centre), centre);
        assert_eq!(camera_goal(PlayMode::Editing, player, None), player);
        assert_eq!(camera_goal(PlayMode::Playing, None, None), None);
    }

    #[test]
    fn a_normal_window_takes_focus() {
        let w = EngineConfig::default().window();
        assert!(w.focused);
        assert_eq!(w.window_level, WindowLevel::Normal);
        assert!(!w.skip_taskbar);
    }

    #[test]
    fn quiet_windows_never_take_focus() {
        let w = Engine::new("t").quiet(true).config.window();
        assert!(!w.focused);
        assert_eq!(w.window_level, WindowLevel::AlwaysOnBottom);
        assert!(w.skip_taskbar);
    }

    /// The case that cost a ranked match: any automated run must be quiet
    /// without anyone remembering to ask for it.
    #[test]
    fn automated_runs_are_background_by_default() {
        for e in [
            Engine::new("t").exit_after_frames(10),
            Engine::new("t").screenshot_at_frame(5, "x.png"),
        ] {
            let w = e.config.window();
            assert!(!w.focused, "automated run would steal focus");
            assert!(w.skip_taskbar);
        }
    }

    #[test]
    fn status_log_keeps_only_the_newest_lines() {
        let mut s = EngineStatus::default();
        for i in 0..20 {
            s.say(format!("m{i}"));
        }
        let lines: Vec<_> = s.lines().collect();
        assert_eq!(lines.len(), EngineStatus::KEEP);
        assert_eq!(lines.last(), Some(&"m19"));
        assert_eq!(lines.first(), Some(&"m12"));
    }
}
