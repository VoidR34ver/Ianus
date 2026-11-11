//! Engine check, not a game: a floor to see movement against, a player to
//! drive, and a few inert crates so the Scene list has something in it.
//!
//! ```text
//! cargo run -p ianus_engine --example hello_engine
//! ```
//!
//! WASD / arrows move. Click things in the game view or the Scene list to
//! inspect them. F1 to F6 are the shortcuts (see the crate docs).
//! Env vars for automated checks: `IANUS_EXIT_AFTER_FRAMES`, `IANUS_SCREENSHOT`.

use bevy::prelude::*;
use ianus_engine::prelude::*;

fn main() -> AppExit {
    let mut engine = Engine::new("Ianus engine check");
    if let Some(n) = std::env::var("IANUS_EXIT_AFTER_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        engine = engine.exit_after_frames(n);
    }
    if std::env::var("IANUS_NO_GUI").is_ok() {
        engine = engine.gui(false);
    }
    if let Ok(path) = std::env::var("IANUS_SCREENSHOT") {
        let at = std::env::var("IANUS_SCREENSHOT_FRAME")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        engine = engine.screenshot_at_frame(at, path);
    }

    let mut app = engine.build();
    app.set_restart_hook(reset_bodies)
        .add_systems(Startup, setup);
    if std::env::var("IANUS_DEBUG").is_ok() {
        app.add_systems(Update, debug_report);
    }
    app.run()
}

fn debug_report(
    mut frame: Local<u32>,
    cameras: Query<(Entity, &Camera, &Transform, &Projection, Has<Camera2d>)>,
    sprites: Query<(
        &Sprite,
        &GlobalTransform,
        &ViewVisibility,
        &InheritedVisibility,
        Has<Mesh2d>,
    )>,
) {
    *frame += 1;
    if !frame.is_multiple_of(60) {
        return;
    }
    for (e, cam, tf, proj, is2d) in &cameras {
        println!(
            "[dbg] camera {e:?} 2d={is2d} order={} active={} viewport={:?} pos={} proj={:?}",
            cam.order,
            cam.is_active,
            cam.viewport
                .as_ref()
                .map(|v| (v.physical_position, v.physical_size)),
            tf.translation,
            match proj {
                Projection::Orthographic(o) =>
                    format!("ortho area={:?} near={} far={}", o.area, o.near, o.far),
                _ => "other".into(),
            }
        );
    }
    let total = sprites.iter().count();
    let seen = sprites.iter().filter(|s| s.2.get()).count();
    let inherited = sprites.iter().filter(|s| s.3.get()).count();
    let with_mesh = sprites.iter().filter(|s| s.4).count();
    println!(
        "[dbg] sprites total={total} view_visible={seen} inherited_visible={inherited} with_mesh2d={with_mesh}"
    );
    if let Some((s, g, ..)) = sprites.iter().next() {
        println!(
            "[dbg] first sprite size={:?} color={:?} at {}",
            s.custom_size,
            s.color,
            g.translation()
        );
    }
}

/// Where each body starts, in tiles. The restart hook puts them back here.
#[derive(Component)]
struct Home(Vec2i);

fn tiles(x: i32, y: i32) -> Vec2i {
    Vec2i::new(x * SUBUNITS_PER_TILE, y * SUBUNITS_PER_TILE)
}

fn setup(mut commands: Commands) {
    for x in -24..24 {
        for y in -16..16 {
            let shade = if (x + y) % 2 == 0 { 0.16 } else { 0.2 };
            commands.spawn((
                Sprite::from_color(
                    Color::srgb(shade, shade + 0.03, shade + 0.07),
                    Vec2::splat(31.0),
                ),
                Transform::from_xyz(x as f32 * 32.0, y as f32 * 32.0, 0.0),
            ));
        }
    }

    commands.spawn((
        Name::new("Player"),
        SimBody::default(),
        Home(Vec2i::ZERO),
        Controlled,
        Sprite::from_color(Color::srgb(1.0, 0.6, 0.2), Vec2::splat(24.0)),
        Transform::from_xyz(0.0, 0.0, 1.0),
    ));

    for (i, (x, y)) in [(4, 2), (-3, 3), (2, -4)].into_iter().enumerate() {
        commands.spawn((
            Name::new(format!("Crate {}", i + 1)),
            SimBody(Body::at(tiles(x, y))),
            Home(tiles(x, y)),
            Sprite::from_color(Color::srgb(0.55, 0.42, 0.28), Vec2::splat(22.0)),
            Transform::from_xyz(0.0, 0.0, 1.0),
        ));
    }
}

fn reset_bodies(mut bodies: Query<(&mut SimBody, &Home)>) {
    for (mut body, home) in &mut bodies {
        body.0 = Body::at(home.0);
    }
}
