//! Writes the sample room to `rooms/cellar.room` (or the path given).
//!
//! ```text
//! cargo run -p ianus_objects --example write_cellar
//! ```
//!
//! Generated from code so the file format is exactly what the engine writes.

use ianus_objects::{Action, Category, Kind, ObjectDef, Room, Substance};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rooms/cellar.room".into());
    let room = cellar();

    let problems = room.validate();
    assert!(
        problems.is_empty(),
        "the sample room must be clean: {problems:?}"
    );

    if let Some(dir) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(dir).expect("create rooms folder");
    }
    std::fs::write(&path, room.to_text()).expect("write room file");
    println!("wrote {path}: {} objects placed", room.placed.len());
}

fn cellar() -> Room {
    let (w, h) = (20u32, 12u32);
    let mut r = Room::new("Cellar", w, h);

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
    r.define(ObjectDef::with_kind(
        "Torch",
        Kind::Light {
            radius_tiles: 5,
            color: [255, 190, 110],
            intensity_percent: 100,
        },
    ));
    r.define(ObjectDef::with_kind(
        "Switch",
        Kind::Interactable {
            action: Action::Toggle("Torch".into()),
            radius_tiles: 1,
            once: false,
        },
    ));
    r.define(ObjectDef::with_kind(
        "Mud",
        Kind::Environment {
            walkable: true,
            speed_percent: 50,
        },
    ));
    r.define(ObjectDef::with_kind(
        "Spring",
        Kind::SubstanceSource {
            substance: Substance::Water,
            rate_per_tick: 2,
        },
    ));
    r.define(ObjectDef::with_kind(
        "Vent",
        Kind::SubstanceSource {
            substance: Substance::Heat,
            rate_per_tick: 1,
        },
    ));

    // Outer walls.
    for x in 0..w as i32 {
        r.place("Wall", x, 0);
        r.place("Wall", x, h as i32 - 1);
    }
    for y in 1..h as i32 - 1 {
        r.place("Wall", 0, y);
        r.place("Wall", w as i32 - 1, y);
    }
    // An inner wall with a gap.
    for y in 1..7 {
        r.place("Wall", 9, y);
    }
    // A mud patch.
    for x in 12..15 {
        for y in 5..8 {
            r.place("Mud", x, y);
        }
    }
    r.place("Player", 3, 3);
    r.place("Crate", 6, 4);
    r.place("Crate", 11, 8);
    r.place("Torch", 4, 1 + 1);
    r.place("Torch", 15, 9);
    r.place("Switch", 8, 2);
    r.place("Spring", 16, 3);
    r.place("Vent", 6, 9);
    r
}
