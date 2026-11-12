//! Rooms, play/stop and file watching, through the real schedules.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use bevy_app::App;
use bevy_ecs::prelude::{IntoScheduleConfigs, Query, With};
use ianus_objects::{Kind, ObjectDef};
use ianus_sim::SUBUNITS_PER_TILE as TILE;

use crate::prelude::*;
use crate::tests::{hold, keys, script, test_app, ticks};

/// A valid room with no warnings: a player, a crate, two walls and a torch.
fn cellar() -> Room {
    let mut r = Room::new("Cellar", 12, 8);
    r.define(ObjectDef::with_kind(
        "Player",
        Kind::Entity {
            controlled: true,
            speed_tiles_per_s: 5,
        },
    ));
    r.define(ObjectDef::with_kind(
        "Crate",
        Kind::Entity {
            controlled: false,
            speed_tiles_per_s: 2,
        },
    ));
    r.define(ObjectDef::new("Wall", Category::Wall));
    r.define(ObjectDef::new("Torch", Category::Light));
    r.place("Player", 2, 2);
    r.place("Crate", 5, 3);
    r.place("Wall", 0, 0);
    r.place("Wall", 1, 0);
    r.place("Torch", 6, 6);
    r
}

fn loaded_app() -> App {
    let mut app = test_app(1.0);
    app.world_mut()
        .init_resource::<bevy_input::ButtonInput<bevy_input::keyboard::KeyCode>>();
    load_room(app.world_mut(), cellar()).expect("cellar is valid");
    app
}

fn body_named(app: &mut App, name: &str) -> Body {
    let mut q = app.world_mut().query::<(&PlacedObject, &SimBody)>();
    q.iter(app.world())
        .find(|(p, _)| p.name == name)
        .map(|(_, b)| b.0)
        .unwrap_or_else(|| panic!("no object named {name}"))
}

fn set_body_named(app: &mut App, name: &str, pos: Vec2i) {
    let mut q = app.world_mut().query::<(&PlacedObject, &mut SimBody)>();
    for (p, mut b) in q.iter_mut(app.world_mut()) {
        if p.name == name {
            b.0 = Body::at(pos);
        }
    }
}

fn count<C: bevy_ecs::component::Component>(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<(), With<C>>()
        .iter(app.world())
        .count()
}

fn hash(app: &mut App) -> u64 {
    let mut q = app.world_mut().query::<&SimBody>();
    world_hash(q.iter(app.world()))
}

fn mode(app: &App) -> PlayMode {
    *app.world().resource::<PlayMode>()
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ianus_test_{}_{name}", std::process::id()))
}

#[test]
fn loading_a_room_spawns_every_placed_object() {
    let mut app = test_app(1.0);
    let warnings = load_room(app.world_mut(), cellar()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");

    assert_eq!(count::<PlacedObject>(&mut app), 5);
    assert_eq!(count::<Controlled>(&mut app), 1);
    assert_eq!(
        body_named(&mut app, "Player").pos,
        Vec2i::new(2 * TILE, 2 * TILE)
    );
    assert_eq!(
        body_named(&mut app, "Torch").pos,
        Vec2i::new(6 * TILE, 6 * TILE)
    );
    assert_eq!(app.world().resource::<CurrentRoom>().0.name, "Cellar");
}

#[test]
fn a_loaded_room_starts_in_edit_mode_and_paused() {
    let app = loaded_app();
    assert_eq!(mode(&app), PlayMode::Editing);
    assert!(app.world().resource::<SimControl>().paused);
    assert_eq!(ticks(&app), 0);
}

#[test]
fn edit_mode_is_frozen_even_with_keys_held() {
    let mut app = loaded_app();
    hold(&mut app, MoveInput::RIGHT.bits());
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(ticks(&app), 0);
    assert_eq!(
        body_named(&mut app, "Player").pos,
        Vec2i::new(2 * TILE, 2 * TILE)
    );
}

#[test]
fn a_bad_room_is_refused_and_changes_nothing() {
    let mut app = loaded_app();
    let mut bad = cellar();
    bad.name = "Broken".into();
    bad.place("Wal", 3, 3);

    let err = load_room(app.world_mut(), bad).unwrap_err();
    let LoadError::Invalid(problems) = err else {
        panic!("wrong error kind")
    };
    assert!(
        problems[0].message.contains("Did you mean 'Wall'?"),
        "{}",
        problems[0].message
    );

    assert_eq!(count::<PlacedObject>(&mut app), 5, "old room was disturbed");
    assert_eq!(app.world().resource::<CurrentRoom>().0.name, "Cellar");
}

#[test]
fn reloading_replaces_instead_of_piling_up() {
    let mut app = loaded_app();
    for _ in 0..3 {
        load_room(app.world_mut(), cellar()).unwrap();
    }
    assert_eq!(count::<PlacedObject>(&mut app), 5);

    let mut smaller = Room::new("Closet", 3, 3);
    smaller.define(ObjectDef::new("Wall", Category::Wall));
    smaller.place("Wall", 1, 1);
    load_room(app.world_mut(), smaller).unwrap();
    assert_eq!(count::<PlacedObject>(&mut app), 1);
}

#[test]
fn warnings_are_returned_but_do_not_block_loading() {
    let mut app = test_app(1.0);
    let mut r = cellar();
    r.placed.retain(|p| p.object != "Player"); // no controlled entity
    let warnings = load_room(app.world_mut(), r).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("WASD"));
}

#[test]
fn play_runs_the_game_and_stop_puts_everything_back() {
    let mut app = loaded_app();
    let before = hash(&mut app);

    assert!(play(app.world_mut()));
    assert!(!play(app.world_mut()), "already playing");
    assert_eq!(mode(&app), PlayMode::Playing);

    hold(&mut app, MoveInput::RIGHT.bits());
    for _ in 0..40 {
        app.update();
    }
    assert!(
        body_named(&mut app, "Player").pos.x > 2 * TILE,
        "player did not move"
    );
    assert!(ticks(&app) > 0);
    assert_ne!(hash(&mut app), before);

    assert!(stop(app.world_mut()));
    assert!(!stop(app.world_mut()), "already stopped");
    assert_eq!(mode(&app), PlayMode::Editing);
    assert_eq!(ticks(&app), 0);
    assert_eq!(
        hash(&mut app),
        before,
        "Stop must restore the exact starting state"
    );
    assert!(app.world().resource::<SimControl>().paused);
}

#[test]
fn stop_restores_the_edited_layout_not_the_original() {
    let mut app = loaded_app();
    // The user drags the crate in the editor before pressing Play.
    set_body_named(&mut app, "Crate", Vec2i::new(9 * TILE, 5 * TILE));

    play(app.world_mut());
    set_body_named(&mut app, "Crate", Vec2i::new(11 * TILE, 7 * TILE)); // the game moves it
    stop(app.world_mut());

    assert_eq!(
        body_named(&mut app, "Crate").pos,
        Vec2i::new(9 * TILE, 5 * TILE)
    );
}

#[test]
fn things_spawned_during_play_vanish_on_stop() {
    let mut app = loaded_app();
    play(app.world_mut());
    app.world_mut().spawn(SimBody::default());
    app.world_mut().spawn(SimBody::default());
    assert_eq!(count::<SimBody>(&mut app), 7);
    stop(app.world_mut());
    assert_eq!(count::<SimBody>(&mut app), 5);
}

#[test]
fn stepping_from_edit_mode_runs_exactly_one_tick_and_can_be_undone() {
    let mut app = loaded_app();
    let before = hash(&mut app);
    hold(&mut app, MoveInput::RIGHT.bits());

    step_once(app.world_mut());
    assert_eq!(mode(&app), PlayMode::Playing);
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(ticks(&app), 1, "a step is exactly one tick");

    assert!(stop(app.world_mut()));
    assert_eq!(hash(&mut app), before);
}

#[test]
fn stop_throws_away_a_recording_in_progress() {
    let mut app = loaded_app();
    play(app.world_mut());
    start_recording(app.world_mut(), 1).unwrap();
    assert!(matches!(
        app.world().resource::<ReplayMode>(),
        ReplayMode::Recording(_)
    ));
    stop(app.world_mut());
    assert!(matches!(
        app.world().resource::<ReplayMode>(),
        ReplayMode::Off
    ));
}

#[test]
fn restart_puts_placed_objects_back_without_any_hook() {
    let mut app = loaded_app();
    play(app.world_mut());
    set_body_named(&mut app, "Crate", Vec2i::new(10 * TILE, TILE));
    restart_world(app.world_mut()).unwrap();
    assert_eq!(
        body_named(&mut app, "Crate").pos,
        Vec2i::new(5 * TILE, 3 * TILE)
    );
    assert_eq!(
        mode(&app),
        PlayMode::Playing,
        "restart must not change the mode"
    );
}

/// The player can never be inside a wall tile, checked through the real
/// schedules while steering around the cellar with the key script.
fn player_is_inside_a_solid_tile(app: &mut App) -> bool {
    let solid = app.world().resource::<SolidMap>().0.clone();
    let b = body_named(app, "Player");
    solid.overlaps(&Footprint::DEFAULT.aabb_at(b.pos.x, b.pos.y))
}

#[test]
fn walls_are_built_from_the_room() {
    let map = SolidMap::from_room(&cellar());
    assert!(
        map.0.tiles().is_solid(0, 0) && map.0.tiles().is_solid(1, 0),
        "placed walls"
    );
    assert!(!map.0.tiles().is_solid(2, 2), "open floor");
    assert!(
        map.0.tiles().is_solid(-1, 3) && map.0.tiles().is_solid(12, 3),
        "outside the room"
    );
    assert!(!map.0.tiles().is_solid(5, 3), "a crate is not a wall");

    let mut with_hole = cellar();
    with_hole.define(ObjectDef::with_kind(
        "Pit",
        Kind::Environment {
            walkable: false,
            speed_percent: 100,
        },
    ));
    with_hole.place("Pit", 8, 4);
    assert!(
        SolidMap::from_room(&with_hole).0.tiles().is_solid(8, 4),
        "non-walkable ground blocks"
    );
}

#[test]
fn the_player_cannot_leave_the_room_or_enter_walls() {
    let mut app = loaded_app();
    play(app.world_mut());
    for f in 0..2000 {
        hold(&mut app, script(f));
        app.update();
        assert!(
            !player_is_inside_a_solid_tile(&mut app),
            "inside a wall at frame {f}"
        );
    }
}

#[test]
fn walking_into_a_wall_stops_flush_against_it() {
    let mut app = loaded_app();
    play(app.world_mut());
    // This cellar has no left wall, only two wall tiles on the bottom row, so
    // going left ends at the edge of the room itself, and going down then
    // lands on top of the wall tile at (0, 0).
    hold(&mut app, MoveInput::LEFT.bits());
    for _ in 0..200 {
        app.update();
    }
    let b = body_named(&mut app, "Player");
    // The room's edge is half a tile left of tile 0's centre.
    assert_eq!(b.pos.x - ianus_sim::BODY_HALF_SIZE, -TILE / 2);
    assert_eq!(b.vel.x, 0);

    hold(&mut app, MoveInput::DOWN.bits());
    for _ in 0..200 {
        app.update();
    }
    let b = body_named(&mut app, "Player");
    // Flush with the top of the wall tile at (0, 0).
    assert_eq!(b.pos.y - ianus_sim::BODY_HALF_SIZE, TILE / 2);
}

#[test]
fn a_room_with_no_walls_loaded_still_blocks_at_its_edge() {
    let mut app = test_app(1.0);
    let mut r = Room::new("Bare", 6, 6);
    r.define(ObjectDef::with_kind(
        "Player",
        Kind::Entity {
            controlled: true,
            speed_tiles_per_s: 5,
        },
    ));
    r.place("Player", 3, 3);
    load_room(app.world_mut(), r).unwrap();
    play(app.world_mut());
    hold(&mut app, MoveInput::RIGHT.bits());
    for _ in 0..300 {
        app.update();
    }
    let b = body_named(&mut app, "Player");
    // Last open tile is 5; its far edge is 5.5 tiles.
    assert_eq!(b.pos.x + ianus_sim::BODY_HALF_SIZE, 5 * TILE + TILE / 2);
}

#[test]
fn replays_in_a_walled_room_still_match() {
    let mut app = loaded_app();
    play(app.world_mut());
    start_recording(app.world_mut(), 3).unwrap();
    for f in 0..600 {
        hold(&mut app, script(f));
        app.update();
    }
    let replay = finish_recording(app.world_mut()).unwrap();
    start_playback(app.world_mut(), replay).unwrap();
    hold(&mut app, 0);
    for _ in 0..3000 {
        app.update();
        if let ReplayMode::Finished { matched } = *app.world().resource::<ReplayMode>() {
            assert_eq!(matched, Some(true), "collision must be deterministic");
            return;
        }
    }
    panic!("playback never finished");
}

#[test]
fn replays_work_in_a_room_with_no_restart_hook() {
    let mut app = loaded_app();
    play(app.world_mut());

    start_recording(app.world_mut(), 7).unwrap();
    for f in 0..300 {
        hold(&mut app, script(f));
        app.update();
    }
    let replay = finish_recording(app.world_mut()).unwrap();
    assert_ne!(
        body_named(&mut app, "Player").pos,
        Vec2i::new(2 * TILE, 2 * TILE)
    );

    start_playback(app.world_mut(), replay).unwrap();
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
fn loading_from_a_file_and_the_three_ways_it_can_fail() {
    let good = temp_path("good.room");
    fs::write(&good, cellar().to_text()).unwrap();
    let mut app = test_app(1.0);
    assert!(load_room_file(app.world_mut(), &good).is_ok());
    assert_eq!(count::<PlacedObject>(&mut app), 5);

    let garbage = temp_path("garbage.room");
    fs::write(&garbage, "this is not a room").unwrap();
    let e = load_room_file(app.world_mut(), &garbage).unwrap_err();
    assert!(matches!(e, LoadError::Parse(_)));
    assert!(e.to_string().contains("line"), "{e}");

    let missing = temp_path("missing.room");
    let e = load_room_file(app.world_mut(), &missing).unwrap_err();
    assert!(matches!(e, LoadError::Io { .. }));
    assert!(e.to_string().contains("cannot read"), "{e}");

    assert_eq!(
        count::<PlacedObject>(&mut app),
        5,
        "failed loads must not disturb the room"
    );
    let _ = fs::remove_file(good);
    let _ = fs::remove_file(garbage);
}

#[test]
fn file_watch_notices_edits_deletion_and_recreation() {
    let path = temp_path("watched.room");
    fs::write(&path, "one").unwrap();
    let mut watch = FileWatch::new(&path);
    assert_eq!(watch.poll(), FileChange::Unchanged);

    // Set the time explicitly: real writes can land in the same timestamp tick.
    fs::write(&path, "two").unwrap();
    let later = SystemTime::now() + Duration::from_secs(60);
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert_eq!(watch.poll(), FileChange::Changed);
    assert_eq!(
        watch.poll(),
        FileChange::Unchanged,
        "a change is reported once"
    );

    fs::remove_file(&path).unwrap();
    assert_eq!(watch.poll(), FileChange::Gone);
    assert_eq!(watch.poll(), FileChange::Unchanged, "gone is reported once");

    fs::write(&path, "three").unwrap();
    assert_eq!(watch.poll(), FileChange::Changed);
    let _ = fs::remove_file(path);
}

#[test]
fn watching_a_file_that_does_not_exist_yet_is_quiet_until_it_does() {
    let path = temp_path("later.room");
    let _ = fs::remove_file(&path);
    let mut watch = FileWatch::new(&path);
    assert_eq!(watch.poll(), FileChange::Unchanged);
    fs::write(&path, "x").unwrap();
    assert_eq!(watch.poll(), FileChange::Changed);
    let _ = fs::remove_file(path);
}

#[test]
fn keys_helper_is_reachable_for_other_tests() {
    // Guards the shared test helpers: if they were made private again this
    // module would stop compiling, which is the signal.
    let mut app = test_app(1.0);
    app.world_mut()
        .init_resource::<bevy_input::ButtonInput<bevy_input::keyboard::KeyCode>>();
    let _ = keys(&mut app);
}

// ---- Things other than the player moving, and things disappearing ----

/// The cellar plus one wall in the player's row, three tiles to its right.
fn cellar_with_wall_ahead() -> App {
    let mut room = cellar();
    room.place("Wall", 5, 2);
    let mut app = test_app(1.0);
    load_room(app.world_mut(), room).expect("valid");
    play(app.world_mut());
    // The first frame runs no tick; a few frames make sure the collision
    // structures have been built at least once.
    for _ in 0..5 {
        app.update();
    }
    app
}

fn entity_at(app: &mut App, name: &str, tile: (i32, i32)) -> bevy_ecs::entity::Entity {
    let home = Vec2i::new(tile.0 * TILE, tile.1 * TILE);
    let mut q = app
        .world_mut()
        .query::<(bevy_ecs::entity::Entity, &PlacedObject)>();
    q.iter(app.world())
        .find(|(_, p)| p.name == name && p.home == home)
        .map(|(e, _)| e)
        .unwrap_or_else(|| panic!("no {name} at {tile:?}"))
}

#[test]
fn a_wall_despawned_during_play_stops_blocking() {
    let mut app = cellar_with_wall_ahead();
    let wall = entity_at(&mut app, "Wall", (5, 2));
    app.world_mut().despawn(wall);
    hold(&mut app, MoveInput::RIGHT.bits());
    for _ in 0..120 {
        app.update();
    }
    assert!(
        body_named(&mut app, "Player").pos.x > 6 * TILE,
        "the player was stopped by a wall that no longer exists"
    );
}

#[test]
fn a_collider_removed_during_play_stops_blocking() {
    let mut app = cellar_with_wall_ahead();
    let wall = entity_at(&mut app, "Wall", (5, 2));
    app.world_mut().entity_mut(wall).remove::<Collider>();
    hold(&mut app, MoveInput::RIGHT.bits());
    for _ in 0..120 {
        app.update();
    }
    assert!(body_named(&mut app, "Player").pos.x > 6 * TILE);
}

#[test]
fn despawned_objects_leave_touching_and_due_objects() {
    let mut room = cellar();
    room.placed.retain(|p| p.object != "Crate");
    room.place("Crate", 4, 2);
    let mut app = test_app(1.0);
    load_room(app.world_mut(), room).expect("valid");
    play(app.world_mut());
    for _ in 0..5 {
        app.update();
    }
    let crate_entity = entity_at(&mut app, "Crate", (4, 2));
    let due = |app: &App| {
        app.world()
            .resource::<DueObjects>()
            .0
            .iter()
            .any(|&(e, _)| e == crate_entity)
    };
    assert!(due(&app), "an ob_tick 1 crate is due every tick");

    app.world_mut().despawn(crate_entity);
    hold(&mut app, MoveInput::RIGHT.bits());
    for _ in 0..40 {
        app.update();
        assert!(!due(&app), "a despawned object was handed out as due");
        assert!(
            !app.world().resource::<Touching>().0.contains(&crate_entity),
            "a despawned object was reported as touched"
        );
    }
    assert!(
        body_named(&mut app, "Player").pos.x > 4 * TILE,
        "the player never reached where the crate was, so the test proved nothing"
    );
}

/// Game logic: on tick 3, teleport the crate onto the player's start tile.
fn shove_crate(tick: bevy_ecs::prelude::Res<Tick>, mut q: Query<(&PlacedObject, &mut SimBody)>) {
    if tick.0 == 3 {
        for (p, mut b) in &mut q {
            if p.name == "Crate" {
                b.0.pos = Vec2i::new(2 * TILE, 2 * TILE);
            }
        }
    }
}

#[test]
fn something_other_than_the_player_moving_is_touched_and_drawn_right_away() {
    let mut app = loaded_app();
    app.add_systems(
        bevy_app::FixedUpdate,
        shove_crate.in_set(crate::IanusSet::Sim),
    );
    play(app.world_mut());
    while ticks(&app) < 4 {
        app.update();
    }
    let crate_entity = entity_at(&mut app, "Crate", (5, 3));
    assert_eq!(body_named(&mut app, "Crate").pos, Vec2i::new(2 * TILE, 2 * TILE));
    assert!(
        app.world().resource::<Touching>().0.contains(&crate_entity),
        "Touching must see a move made earlier in the same tick"
    );

    // One more tick and it has settled: drawn exactly where its body is.
    app.update();
    app.update();
    let x = app
        .world()
        .get::<bevy_transform::components::Transform>(crate_entity)
        .unwrap()
        .translation
        .x;
    assert_eq!(x, 2.0 * 32.0, "drawn somewhere other than where it is");
}

#[test]
fn a_key_tapped_while_paused_does_not_fire_on_resume_but_a_held_key_steps() {
    let mut app = loaded_app();
    play(app.world_mut());
    app.update();
    app.update();
    app.world_mut().resource_mut::<SimControl>().paused = true;
    app.update();
    let before = body_named(&mut app, "Player");

    hold(&mut app, MoveInput::RIGHT.bits()); // tapped while paused
    app.update();
    hold(&mut app, 0);
    for _ in 0..20 {
        app.update();
    }
    app.world_mut().resource_mut::<SimControl>().paused = false;
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(
        body_named(&mut app, "Player"),
        before,
        "a tap from during the pause moved the player after resume"
    );

    // Holding a key and stepping one tick must still move.
    app.world_mut().resource_mut::<SimControl>().paused = true;
    app.update();
    hold(&mut app, MoveInput::RIGHT.bits());
    app.world_mut().resource_mut::<SimControl>().step_requests = 1;
    app.update();
    assert!(body_named(&mut app, "Player").pos.x > before.pos.x);
}

#[test]
fn a_solid_player_is_not_a_wall_in_its_own_way() {
    let mut room = Room::new("Solo", 12, 8);
    let mut player = ObjectDef::with_kind(
        "Player",
        Kind::Entity {
            controlled: true,
            speed_tiles_per_s: 5,
        },
    );
    player.solid = true;
    room.define(player);
    room.place("Player", 2, 2);
    assert_eq!(SolidMap::from_room(&room).0.box_count(), 0);
}
