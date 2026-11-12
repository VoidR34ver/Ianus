//! Writes the pathfinding demo room to `rooms/chase.room` (or the path given).
//!
//! ```text
//! cargo run -p ianus_objects --example write_chase
//! cargo run -p ianus_engine --example room -- rooms/chase.room
//! ```
//!
//! A river runs down the middle with a two-tile bridge. The **Nextbot** avoids
//! water, so it has to use the bridge. The **Wader** does not mind water and
//! wades straight across. Both chase the player, around the walls.

use ianus_objects::{AgentDef, AgentTarget, Category, Kind, ObjectDef, Room};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rooms/chase.room".into());
    let room = chase();
    let problems = room.validate();
    assert!(
        problems.is_empty(),
        "the demo room must be clean: {problems:?}"
    );
    if let Some(dir) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(dir).expect("create rooms folder");
    }
    std::fs::write(&path, room.to_text()).expect("write room file");
    println!("wrote {path}: {} objects placed", room.placed.len());
}

pub fn chase() -> Room {
    let (w, h) = (24i32, 14i32);
    let mut r = Room::new("Chase", w as u32, h as u32);
    r.declare_tag("water");

    r.define(ObjectDef::with_kind(
        "Player",
        Kind::Entity {
            controlled: true,
            speed_tiles_per_s: 5,
        },
    ));

    let mut nextbot = ObjectDef::with_kind(
        "Nextbot",
        Kind::Entity {
            controlled: false,
            speed_tiles_per_s: 4,
        },
    );
    nextbot.agent = Some(AgentDef {
        target: AgentTarget::Controlled,
        allow: vec![],
        avoid: vec!["water".into()],
        costs: vec![],
        stop_distance: 100,
    });
    r.define(nextbot);

    let mut wader = ObjectDef::with_kind(
        "Wader",
        Kind::Entity {
            controlled: false,
            speed_tiles_per_s: 3,
        },
    );
    wader.agent = Some(AgentDef::default());
    r.define(wader);

    r.define(ObjectDef::new("Wall", Category::Wall));

    // Water is ground you can walk on, tagged so agents can decide about it.
    // It has no hitbox of its own.
    let mut water = ObjectDef::with_kind(
        "Water",
        Kind::Environment {
            walkable: true,
            speed_percent: 100,
        },
    );
    water.nav_tags = vec!["water".into()];
    water.collision = false;
    r.define(water);

    for x in 0..w {
        r.place("Wall", x, 0);
        r.place("Wall", x, h - 1);
    }
    for y in 1..h - 1 {
        r.place("Wall", 0, y);
        r.place("Wall", w - 1, y);
    }
    // West wall, open at the bottom rows 10 to 12.
    for y in 1..=9 {
        r.place("Wall", 8, y);
    }
    // East wall, open at the top rows 8 to 12.
    for y in 1..=7 {
        r.place("Wall", 16, y);
    }
    // The river, with a bridge at rows 6 and 7.
    for y in 1..h - 1 {
        if y != 6 && y != 7 {
            r.place("Water", 12, y);
        }
    }

    r.place("Player", 3, 6);
    r.place("Nextbot", 20, 3);
    r.place("Wader", 20, 11);
    r
}
