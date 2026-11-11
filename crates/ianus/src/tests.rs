//! These run the real schedules (not the systems in isolation), with Bevy's
//! clock under manual control so every run is repeatable.

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::prelude::Query;
use bevy_input::{ButtonInput, keyboard::KeyCode};
use bevy_time::{Fixed, Time, TimePlugin, TimeUpdateStrategy};
use bevy_transform::components::Transform;
use ianus_sim::step_body;

use crate::prelude::*;

/// App where each `update()` advances the clock by `step_fraction` of one tick.
pub(crate) fn test_app(step_fraction: f64) -> App {
    let mut app = App::new();
    app.add_plugins((TimePlugin, IanusPlugin));
    // Real Bevy fills this in from window events; here we drive it by hand.
    app.init_resource::<ButtonInput<KeyCode>>();
    let tick = app.world().resource::<Time<Fixed>>().timestep();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        tick.as_secs_f64() * step_fraction,
    )));
    app
}

pub(crate) fn keys(app: &mut App) -> bevy_ecs::world::Mut<'_, ButtonInput<KeyCode>> {
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>()
}

pub(crate) fn ticks(app: &App) -> u64 {
    app.world().resource::<Tick>().0
}

fn body_of_player(app: &mut App) -> Body {
    app.world_mut()
        .query_filtered::<&SimBody, bevy_ecs::query::With<Controlled>>()
        .iter(app.world())
        .next()
        .expect("no player")
        .0
}

fn spawn_player(app: &mut App) {
    app.world_mut().spawn((SimBody::default(), Controlled));
}

#[test]
fn holding_d_matches_the_pure_simulation_tick_for_tick() {
    let mut app = test_app(1.0);
    spawn_player(&mut app);
    keys(&mut app).press(KeyCode::KeyD);

    for _ in 0..90 {
        app.update();
    }

    let n = ticks(&app);
    assert!(n >= 80, "expected roughly one tick per update, got {n}");
    let mut reference = Body::default();
    for _ in 0..n {
        step_body(&mut reference, MoveInput::RIGHT);
    }
    assert_eq!(body_of_player(&mut app), reference);
}

#[test]
fn arrow_keys_work_like_wasd() {
    let mut app = test_app(1.0);
    spawn_player(&mut app);
    keys(&mut app).press(KeyCode::ArrowLeft);
    for _ in 0..20 {
        app.update();
    }
    let body = body_of_player(&mut app);
    assert!(body.pos.x < 0 && body.pos.y == 0);
}

#[test]
fn a_tap_shorter_than_a_tick_still_moves_exactly_one_tick() {
    let mut app = test_app(1.0);
    spawn_player(&mut app);

    // Down and up again before the frame ends: `pressed` is already false.
    keys(&mut app).press(KeyCode::KeyD);
    keys(&mut app).release(KeyCode::KeyD);
    app.update();
    keys(&mut app).clear();
    for _ in 0..30 {
        app.update();
    }

    // Reference: one tick of input, then nothing.
    let mut reference = Body::default();
    step_body(&mut reference, MoveInput::RIGHT);
    for _ in 1..ticks(&app) {
        step_body(&mut reference, MoveInput::NONE);
    }
    let body = body_of_player(&mut app);
    assert_eq!(body, reference);
    assert!(body.pos.x > 0, "the tap was lost");
}

#[test]
fn latch_is_consumed_once() {
    let mut app = test_app(1.0);
    spawn_player(&mut app);
    keys(&mut app).press(KeyCode::KeyD);
    keys(&mut app).release(KeyCode::KeyD);
    app.update();
    keys(&mut app).clear();
    for _ in 0..10 {
        app.update();
    }
    // It moved (so the latch was applied)...
    let body = body_of_player(&mut app);
    assert!(body.pos.x > 0, "the latch was never applied");
    // ...and came to a full stop (60, then friction 40 and 20), which it
    // could not if the latch were applied again on every tick.
    assert_eq!(body.vel, Vec2i::ZERO);
}

#[test]
fn rendering_blends_between_ticks() {
    // Frames at half the tick length, so every other frame lands mid-tick.
    let mut app = test_app(0.5);
    spawn_player(&mut app);
    keys(&mut app).press(KeyCode::KeyD);

    let to_world = 32.0 / f64::from(SUBUNITS_PER_TILE);
    let mut blended_frames = 0;
    for _ in 0..40 {
        app.update();

        let alpha = f64::from(app.world().resource::<Time<Fixed>>().overstep_fraction());
        assert!((0.0..1.0).contains(&alpha));

        let mut q = app.world_mut().query::<(&SimBody, &PrevPos, &Transform)>();
        let (body, prev, transform) = q.iter(app.world()).next().unwrap();
        let expected =
            (f64::from(prev.0.x) + f64::from(body.0.pos.x - prev.0.x) * alpha) * to_world;
        assert!(
            (f64::from(transform.translation.x) - expected).abs() < 1e-3,
            "drawn at {} but expected {expected}",
            transform.translation.x
        );
        let at_tick = f64::from(body.0.pos.x) * to_world;
        if (f64::from(transform.translation.x) - at_tick).abs() > 1e-3 {
            blended_frames += 1;
        }
    }
    assert!(blended_frames > 5, "never drew between ticks");
}

#[test]
fn a_body_spawned_far_away_does_not_slide_in_from_the_origin() {
    let mut app = test_app(1.0);
    let far = Vec2i::new(100 * SUBUNITS_PER_TILE, -40 * SUBUNITS_PER_TILE);
    app.world_mut().spawn(SimBody(Body::at(far)));
    app.update();

    let mut q = app.world_mut().query::<&Transform>();
    let t = q.iter(app.world()).next().unwrap();
    assert!(
        (t.translation.x - 3200.0).abs() < 0.01,
        "x = {}",
        t.translation.x
    );
    assert!(
        (t.translation.y + 1280.0).abs() < 0.01,
        "y = {}",
        t.translation.y
    );
}

/// Hold exactly the keys in `bits` for the next frame.
pub(crate) fn hold(app: &mut App, bits: u8) {
    let mut k = keys(app);
    k.release_all();
    k.clear();
    for (dir, key) in [
        (MoveInput::LEFT, KeyCode::KeyA),
        (MoveInput::RIGHT, KeyCode::KeyD),
        (MoveInput::UP, KeyCode::KeyW),
        (MoveInput::DOWN, KeyCode::KeyS),
    ] {
        if MoveInput::from_bits(bits).contains(dir) {
            k.press(key);
        }
    }
}

/// A fixed, busy-looking key script, identical on every run.
pub(crate) fn script(frame: u32) -> u8 {
    let x = (frame / 5).wrapping_mul(2_654_435_761) >> 7;
    (x & 0b1111) as u8
}

/// Play `frames` frames of the script and return the recorded replay.
fn record_session(frames: u32, seed: u64) -> (Replay, Body) {
    let mut app = test_app(1.0);
    spawn_player(&mut app);
    begin_recording(app.world_mut(), seed);
    for f in 0..frames {
        hold(&mut app, script(f));
        app.update();
    }
    let body = body_of_player(&mut app);
    let replay = finish_recording(app.world_mut()).expect("was recording");
    (replay, body)
}

fn play_back(replay: Replay, start: Vec2i) -> (Option<bool>, App) {
    let mut app = test_app(1.0);
    app.world_mut()
        .spawn((SimBody(Body::at(start)), Controlled));
    begin_playback(app.world_mut(), replay);
    // No keys at all: everything must come from the replay.
    for _ in 0..2000 {
        app.update();
        if let ReplayMode::Finished { matched } = *app.world().resource::<ReplayMode>() {
            return (matched, app);
        }
    }
    panic!("playback never finished");
}

#[test]
fn recorded_session_replays_to_the_identical_state() {
    let (replay, recorded_body) = record_session(400, 77);
    assert!(replay.len() > 300);
    assert_ne!(
        recorded_body,
        Body::default(),
        "the script never moved the player"
    );
    assert!(replay.final_hash.is_some());

    // Through the on-disk format, to prove nothing is lost in saving.
    let replay = Replay::from_bytes(&replay.to_bytes()).unwrap();
    let (matched, app) = play_back(replay.clone(), Vec2i::ZERO);
    assert_eq!(matched, Some(true), "replay diverged from the recording");
    assert!(app.world().resource::<Tick>().0 >= replay.len() as u64);
}

#[test]
fn replay_from_a_different_starting_world_is_reported_as_diverged() {
    let (replay, _) = record_session(200, 1);
    let (matched, _) = play_back(replay, Vec2i::new(1, 0));
    assert_eq!(matched, Some(false));
}

#[test]
fn a_tampered_hash_is_reported_as_diverged() {
    let (mut replay, _) = record_session(200, 1);
    replay.final_hash = replay.final_hash.map(|h| h ^ 1);
    let (matched, _) = play_back(replay, Vec2i::ZERO);
    assert_eq!(matched, Some(false));
}

/// The real workflow, in one app: record, then restart through the hook and
/// play the same session back.
#[test]
fn record_then_play_back_in_the_same_running_app() {
    let mut app = test_app(1.0);
    app.set_restart_hook(|mut q: Query<&mut SimBody>| {
        for mut b in &mut q {
            b.0 = Body::default();
        }
    });
    spawn_player(&mut app);

    start_recording(app.world_mut(), 5).unwrap();
    for f in 0..300 {
        hold(&mut app, script(f));
        app.update();
    }
    let played = body_of_player(&mut app);
    assert_ne!(played, Body::default());
    let replay = finish_recording(app.world_mut()).unwrap();

    // The player is somewhere else now; the hook must bring it back.
    start_playback(app.world_mut(), replay).unwrap();
    assert_eq!(body_of_player(&mut app), Body::default());
    assert_eq!(ticks(&app), 0);

    hold(&mut app, 0);
    for _ in 0..2000 {
        app.update();
        if let ReplayMode::Finished { matched } = *app.world().resource::<ReplayMode>() {
            assert_eq!(matched, Some(true));
            return;
        }
    }
    panic!("playback never finished");
}

#[test]
fn restart_without_a_hook_still_clears_leftover_input() {
    let mut app = test_app(1.0);
    spawn_player(&mut app);
    app.world_mut()
        .resource_mut::<MoveIntent>()
        .latch(MoveInput::UP);
    restart_world(app.world_mut()).unwrap();
    assert_eq!(*app.world().resource::<MoveIntent>(), MoveIntent::default());
}

#[test]
fn recording_does_not_change_what_happens() {
    let (_, recorded) = record_session(300, 9);

    let mut app = test_app(1.0);
    spawn_player(&mut app);
    for f in 0..300 {
        hold(&mut app, script(f));
        app.update();
    }
    assert_eq!(body_of_player(&mut app), recorded);
}

#[test]
fn begin_recording_and_playback_reset_tick_and_rng() {
    let mut app = test_app(1.0);
    for _ in 0..5 {
        app.update();
    }
    assert!(ticks(&app) > 0);
    begin_recording(app.world_mut(), 1234);
    assert_eq!(ticks(&app), 0);
    assert_eq!(app.world().resource::<SimRng>().0, Rng::new(1234));
    assert!(finish_recording(app.world_mut()).is_some());
    assert!(
        finish_recording(app.world_mut()).is_none(),
        "nothing left to finish"
    );
}

#[test]
fn pause_stops_ticks_and_step_runs_exactly_one() {
    let mut app = test_app(1.0);
    for _ in 0..10 {
        app.update();
    }
    app.world_mut().resource_mut::<SimControl>().paused = true;
    // The frame that pauses may still run the tick already in flight.
    app.update();
    let frozen = ticks(&app);
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(ticks(&app), frozen, "ticks kept running while paused");

    app.world_mut().resource_mut::<SimControl>().step_requests = 1;
    app.update();
    assert_eq!(ticks(&app), frozen + 1, "a step must be exactly one tick");
    app.update();
    assert_eq!(ticks(&app), frozen + 1, "the step request must be used up");

    app.world_mut().resource_mut::<SimControl>().paused = false;
    for _ in 0..10 {
        app.update();
    }
    assert!(ticks(&app) >= frozen + 8, "did not resume");
}

#[test]
fn speed_multiplier_scales_ticks_per_frame() {
    let mut normal = test_app(1.0);
    let mut fast = test_app(1.0);
    fast.world_mut().resource_mut::<SimControl>().speed = 2.0;
    for _ in 0..60 {
        normal.update();
        fast.update();
    }
    let (n, f) = (ticks(&normal), ticks(&fast));
    assert!(n >= 55, "normal ran {n} ticks");
    assert!(
        (f as f64) > n as f64 * 1.8 && (f as f64) < n as f64 * 2.2,
        "{f} vs {n}"
    );
}

#[test]
fn works_headless_without_keyboard_resource() {
    let mut app = App::new();
    app.add_plugins((TimePlugin, IanusPlugin));
    let tick = app.world().resource::<Time<Fixed>>().timestep();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(tick));
    spawn_player(&mut app);
    app.world_mut()
        .resource_mut::<MoveIntent>()
        .set_held(MoveInput::UP);
    for _ in 0..30 {
        app.update();
    }
    assert!(body_of_player(&mut app).pos.y > 0);
}
