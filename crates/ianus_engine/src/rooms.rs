//! Rooms in the engine window: loading the file, reloading when it changes,
//! and drawing what is in it.

use std::path::Path;

use bevy::prelude::*;
use ianus::prelude::*;
// Bevy's prelude has its own `Severity`; the explicit import picks ours.
use ianus::prelude::Severity;

use crate::{EngineConfig, EngineStatus};

/// The room file being watched for edits.
#[derive(Resource)]
pub(crate) struct RoomFile {
    watch: FileWatch,
    /// The file changed and has not been loaded yet.
    dirty: bool,
    /// Already told the user a change is waiting for Stop.
    told: bool,
}

/// The room's floor and border, rebuilt whenever a room is loaded.
#[derive(Component)]
pub(crate) struct RoomFloor;

/// How an object is drawn: colour, size in tiles, and draw order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Look {
    pub color: [f32; 3],
    pub size_tiles: f32,
    pub z: f32,
}

/// Placeholder visuals by category. Real art comes later; for now each
/// category is recognisable at a glance.
pub(crate) fn look(kind: &Kind) -> Look {
    match kind {
        Kind::Wall { .. } => Look {
            color: [0.45, 0.48, 0.55],
            size_tiles: 0.96,
            z: 0.5,
        },
        Kind::Entity {
            controlled: true, ..
        } => Look {
            color: [1.0, 0.6, 0.2],
            size_tiles: 0.75,
            z: 1.0,
        },
        Kind::Entity { .. } => Look {
            color: [0.4, 0.8, 0.5],
            size_tiles: 0.7,
            z: 1.0,
        },
        Kind::Interactable { .. } => Look {
            color: [0.95, 0.85, 0.3],
            size_tiles: 0.55,
            z: 0.8,
        },
        Kind::Environment {
            walkable: false, ..
        } => Look {
            color: [0.2, 0.25, 0.35],
            size_tiles: 1.0,
            z: 0.2,
        },
        Kind::Environment { .. } => Look {
            color: [0.35, 0.28, 0.2],
            size_tiles: 1.0,
            z: 0.2,
        },
        Kind::Light { color, .. } => Look {
            color: [
                f32::from(color[0]) / 255.0,
                f32::from(color[1]) / 255.0,
                f32::from(color[2]) / 255.0,
            ],
            size_tiles: 0.4,
            z: 0.9,
        },
        Kind::SubstanceSource { substance, .. } => Look {
            color: match substance {
                Substance::Water => [0.3, 0.55, 1.0],
                Substance::Heat => [1.0, 0.4, 0.25],
                Substance::Gas => [0.55, 0.85, 0.4],
                Substance::Smoke => [0.6, 0.6, 0.65],
            },
            size_tiles: 0.6,
            z: 0.8,
        },
    }
}

/// The lines to show the user after trying to load a room.
///
/// `summary` is what to say on success ("loaded room ..."). Pure, so it is
/// unit-tested without a window.
pub(crate) fn describe_load(
    result: &Result<Vec<Problem>, LoadError>,
    summary: &str,
) -> Vec<String> {
    const MAX_LISTED: usize = 6;
    let list = |problems: &[Problem]| -> Vec<String> {
        let mut lines: Vec<String> = problems
            .iter()
            .take(MAX_LISTED)
            .map(|p| p.to_string())
            .collect();
        if problems.len() > MAX_LISTED {
            lines.push(format!("...and {} more", problems.len() - MAX_LISTED));
        }
        lines
    };
    match result {
        Ok(warnings) => {
            let mut lines = vec![summary.to_string()];
            lines.extend(list(warnings));
            lines
        }
        Err(LoadError::Invalid(problems)) => {
            let errors = problems
                .iter()
                .filter(|p| p.severity == Severity::Error)
                .count();
            let mut lines = vec![format!(
                "room NOT loaded: {errors} error(s). The previous room is still loaded."
            )];
            lines.extend(list(problems));
            lines
        }
        Err(other) => vec![format!("room NOT loaded: {other}")],
    }
}

fn load_and_report(world: &mut World, path: &Path) {
    let result = load_room_file(world, path);
    let summary = match world.get_resource::<CurrentRoom>() {
        Some(room) if result.is_ok() => format!(
            "loaded room '{}' ({}x{}, {} objects) from {}",
            room.0.name,
            room.0.width,
            room.0.height,
            room.0.placed.len(),
            path.display()
        ),
        _ => String::new(),
    };
    let lines = describe_load(&result, &summary);
    let mut status = world.resource_mut::<EngineStatus>();
    for line in lines {
        status.say(line);
    }
}

/// Startup: load the configured room, if any, and start watching its file.
pub(crate) fn load_initial_room(world: &mut World) {
    let Some(path) = world.resource::<EngineConfig>().room_path.clone() else {
        return;
    };
    world.insert_resource(RoomFile {
        watch: FileWatch::new(&path),
        dirty: false,
        told: false,
    });
    load_and_report(world, &path);
    if world.resource::<EngineConfig>().play_on_start {
        play(world);
    }
}

/// Load the room file again now (the toolbar's "Reload room").
pub(crate) fn reload_now(world: &mut World) -> String {
    let Some(path) = world
        .get_resource::<RoomFile>()
        .map(|r| r.watch.path().to_path_buf())
    else {
        return "no room file is set, so there is nothing to reload".into();
    };
    if *world.resource::<PlayMode>() == PlayMode::Playing {
        stop(world);
    }
    load_and_report(world, &path);
    "reloaded".into()
}

/// Reload the room when its file is saved. If the game is running, wait until
/// the user presses Stop rather than yanking the world out from under them.
pub(crate) fn watch_room_file(world: &mut World, mut frame: Local<u32>) {
    *frame += 1;
    if !frame.is_multiple_of(20) {
        return;
    }
    let editing = *world.resource::<PlayMode>() == PlayMode::Editing;
    let Some(mut file) = world.get_resource_mut::<RoomFile>() else {
        return;
    };

    let mut say: Option<String> = None;
    match file.watch.poll() {
        FileChange::Changed => {
            file.dirty = true;
            file.told = false;
        }
        FileChange::Gone => {
            say = Some(format!(
                "room file {} is missing",
                file.watch.path().display()
            ))
        }
        FileChange::Unchanged => {}
    }

    if file.dirty && editing {
        file.dirty = false;
        let path = file.watch.path().to_path_buf();
        load_and_report(world, &path);
        return;
    }
    if file.dirty && !file.told {
        file.told = true;
        say = Some("room file changed; press Stop to load the new version".into());
    }
    if let Some(message) = say {
        world.resource_mut::<EngineStatus>().say(message);
    }
}

/// Give every freshly placed object a sprite that matches its category.
pub(crate) fn decorate_room_objects(
    mut commands: Commands,
    room: Option<Res<CurrentRoom>>,
    scale: Res<WorldScale>,
    mut objects: Query<(Entity, &PlacedObject, &mut Transform), Without<Sprite>>,
) {
    let Some(room) = room else { return };
    for (entity, placed, mut transform) in &mut objects {
        let Some(def) = room.0.find_object(&placed.name) else {
            continue;
        };
        let look = look(&def.kind);
        let [r, g, b] = look.color;
        commands.entity(entity).insert(Sprite::from_color(
            Color::srgb(r, g, b),
            Vec2::splat(look.size_tiles * scale.0),
        ));
        transform.translation.z = look.z;
    }
}

/// Draw the room's floor and border, sized to the room.
pub(crate) fn rebuild_floor(
    mut commands: Commands,
    room: Option<Res<CurrentRoom>>,
    scale: Res<WorldScale>,
    floors: Query<Entity, With<RoomFloor>>,
) {
    let Some(room) = room else { return };
    if !room.is_changed() {
        return;
    }
    for entity in &floors {
        commands.entity(entity).despawn();
    }
    let s = scale.0;
    let (w, h) = (room.0.width as f32, room.0.height as f32);
    // Tile (x, y) is drawn centred on (x, y) * s, so the room's centre is half
    // a tile in from the far corner.
    let centre = Vec2::new((w - 1.0) * 0.5 * s, (h - 1.0) * 0.5 * s);
    commands.spawn((
        RoomFloor,
        Sprite::from_color(
            Color::srgb(0.24, 0.27, 0.33),
            Vec2::new(w * s + 8.0, h * s + 8.0),
        ),
        Transform::from_xyz(centre.x, centre.y, -2.0),
    ));
    commands.spawn((
        RoomFloor,
        Sprite::from_color(Color::srgb(0.1, 0.11, 0.14), Vec2::new(w * s, h * s)),
        Transform::from_xyz(centre.x, centre.y, -1.0),
    ));
}

/// The middle of the room in world units, for the camera when nothing is
/// controlled.
pub(crate) fn room_centre(room: &Room, tile: f32) -> Vec2 {
    Vec2::new(
        (room.width as f32 - 1.0) * 0.5 * tile,
        (room.height as f32 - 1.0) * 0.5 * tile,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(sev: Severity, msg: &str) -> Problem {
        Problem {
            severity: sev,
            message: msg.into(),
        }
    }

    #[test]
    fn every_category_has_a_distinct_recognisable_look() {
        let mut seen = Vec::new();
        for c in Category::ALL {
            let l = look(&Kind::default_for(c));
            assert!(l.size_tiles > 0.0 && l.size_tiles <= 1.0);
            assert!(
                !seen.contains(&l.color),
                "{c:?} looks like another category"
            );
            seen.push(l.color);
        }
    }

    #[test]
    fn player_looks_different_from_other_entities_and_draws_on_top_of_walls() {
        let player = look(&Kind::Entity {
            controlled: true,
            speed_tiles_per_s: 3,
        });
        let other = look(&Kind::Entity {
            controlled: false,
            speed_tiles_per_s: 3,
        });
        let wall = look(&Kind::Wall { durability: None });
        assert_ne!(player.color, other.color);
        assert!(player.z > wall.z);
    }

    #[test]
    fn a_light_is_drawn_in_its_own_colour() {
        let l = look(&Kind::Light {
            radius_tiles: 3,
            color: [255, 0, 51],
            intensity_percent: 100,
        });
        assert_eq!(l.color, [1.0, 0.0, 51.0 / 255.0]);
    }

    #[test]
    fn success_shows_the_summary_then_warnings() {
        let r = Ok(vec![problem(Severity::Warning, "careful")]);
        let lines = describe_load(&r, "loaded it");
        assert_eq!(
            lines,
            vec!["loaded it".to_string(), "warning: careful".to_string()]
        );
    }

    #[test]
    fn failure_says_the_old_room_is_kept_and_lists_errors() {
        let r = Err(LoadError::Invalid(vec![
            problem(Severity::Error, "bad one"),
            problem(Severity::Warning, "meh"),
        ]));
        let lines = describe_load(&r, "");
        assert!(
            lines[0].contains("1 error(s)") && lines[0].contains("still loaded"),
            "{}",
            lines[0]
        );
        assert_eq!(lines[1], "error: bad one");
    }

    #[test]
    fn a_long_problem_list_is_cut_off_with_a_count() {
        let many: Vec<_> = (0..10)
            .map(|i| problem(Severity::Error, &format!("p{i}")))
            .collect();
        let lines = describe_load(&Err(LoadError::Invalid(many)), "");
        assert_eq!(lines.len(), 1 + 6 + 1);
        assert_eq!(lines.last().unwrap(), "...and 4 more");
    }

    #[test]
    fn a_missing_file_gets_one_clear_line() {
        let e = LoadError::Io {
            path: "x.room".into(),
            reason: "not found".into(),
        };
        let lines = describe_load(&Err(e), "");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("cannot read x.room"));
    }

    #[test]
    fn room_centre_is_between_the_corner_tiles() {
        let room = Room::new("r", 11, 5);
        assert_eq!(room_centre(&room, 32.0), Vec2::new(160.0, 64.0));
    }
}
