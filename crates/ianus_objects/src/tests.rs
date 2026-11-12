use crate::validate::{distance_for_tests, suggest_for_tests};
use crate::*;

fn player() -> ObjectDef {
    ObjectDef::with_kind(
        "Player",
        Kind::Entity {
            controlled: true,
            speed_tiles_per_s: 5,
        },
    )
}

/// A small valid room, used as the starting point for most tests.
fn good_room() -> Room {
    let mut r = Room::new("Cellar", 10, 8);
    r.define(player());
    r.define(ObjectDef::new("Wall", Category::Wall));
    r.define(ObjectDef::new("Torch", Category::Light));
    r.place("Player", 2, 2);
    r.place("Wall", 0, 0);
    r.place("Wall", 1, 0);
    r.place("Torch", 5, 5);
    r
}

fn messages(room: &Room) -> Vec<String> {
    room.validate().iter().map(|p| p.message.clone()).collect()
}

#[test]
fn a_good_room_has_no_problems() {
    assert_eq!(good_room().validate(), vec![]);
    assert!(good_room().is_playable());
}

#[test]
fn every_category_has_a_default_and_a_blurb() {
    for c in Category::ALL {
        let def = ObjectDef::new("x", c);
        assert_eq!(def.category(), c, "default kind has the wrong category");
        assert!(!c.label().is_empty() && !c.blurb().is_empty());
    }
}

#[test]
fn text_roundtrip_keeps_everything() {
    let mut r = good_room();
    r.define(ObjectDef::with_kind(
        "Spring",
        Kind::SubstanceSource {
            substance: Substance::Water,
            rate_per_tick: 3,
        },
    ));
    r.define(ObjectDef::with_kind(
        "Switch",
        Kind::Interactable {
            action: Action::Toggle("Torch".into()),
            radius_tiles: 2,
            once: true,
        },
    ));
    let text = r.to_text();
    assert_eq!(Room::from_text(&text).unwrap(), r);
}

#[test]
fn the_room_file_is_readable_by_a_human() {
    let text = good_room().to_text();
    assert!(text.contains("Cellar"));
    assert!(text.contains("controlled: true"));
    assert!(
        text.contains('\n'),
        "should be multi-line so git diffs are useful"
    );
}

#[test]
fn a_broken_room_file_says_where() {
    let mut text = good_room().to_text();
    text = text.replace("width: 10", "width: ten");
    let err = Room::from_text(&text).unwrap_err();
    assert!(err.line > 0 && err.column > 0, "no position in {err:?}");
    let shown = err.to_string();
    assert!(
        shown.contains("line") && shown.contains("column"),
        "{shown}"
    );
}

#[test]
fn garbage_text_is_an_error_not_a_panic() {
    for text in ["", "(", "Room(", "42", "💥", "Room(name: 1)"] {
        assert!(Room::from_text(text).is_err(), "{text:?} was accepted");
    }
}

#[test]
fn a_typo_in_an_object_name_gets_a_suggestion() {
    let mut r = good_room();
    r.place("Wal", 3, 3);
    let m = messages(&r);
    assert_eq!(m.len(), 1);
    assert!(
        m[0].contains("'Wal'") && m[0].contains("Did you mean 'Wall'?"),
        "{}",
        m[0]
    );
}

#[test]
fn an_unrelated_name_lists_what_exists_instead() {
    let mut r = good_room();
    r.place("Dragon", 3, 3);
    let m = messages(&r);
    assert!(
        m[0].contains("Defined objects:") && m[0].contains("Player"),
        "{}",
        m[0]
    );
    assert!(!m[0].contains("Did you mean"));
}

#[test]
fn placing_into_an_empty_room_says_to_create_an_object() {
    let mut r = Room::new("Empty", 4, 4);
    r.place("Wall", 1, 1);
    assert!(messages(&r)[0].contains("defines no objects yet"));
}

#[test]
fn out_of_bounds_placement_names_the_valid_range() {
    let mut r = good_room();
    r.place("Wall", 10, 3);
    r.place("Wall", -1, 0);
    let m = messages(&r);
    assert_eq!(m.len(), 2);
    assert!(
        m[0].contains("outside the 10x8 room") && m[0].contains("(9, 7)"),
        "{}",
        m[0]
    );
}

#[test]
fn a_zero_size_room_is_an_error() {
    let r = Room::new("Nothing", 0, 5);
    assert!(!r.is_playable());
    assert!(messages(&r)[0].contains("0x5"));
}

#[test]
fn duplicate_names_are_an_error() {
    let mut r = good_room();
    r.objects.push(ObjectDef::new("Wall", Category::Wall));
    let m = messages(&r);
    assert!(
        m.iter().any(|s| s.contains("2 objects called 'Wall'")),
        "{m:?}"
    );
    assert!(!r.is_playable());
}

#[test]
fn define_refuses_a_name_that_is_taken() {
    let mut r = good_room();
    assert!(!r.define(ObjectDef::new("Wall", Category::Environment)));
    assert_eq!(r.objects.len(), 3);
}

#[test]
fn a_room_with_no_player_warns_but_still_plays() {
    let mut r = good_room();
    r.placed.retain(|p| p.object != "Player");
    let problems = r.validate();
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].severity, Severity::Warning);
    assert!(problems[0].message.contains("WASD"));
    assert!(r.is_playable());
}

#[test]
fn two_players_is_an_error() {
    let mut r = good_room();
    r.place("Player", 4, 4);
    assert!(!r.is_playable());
    assert!(messages(&r)[0].contains("only one can follow the keyboard"));
}

#[test]
fn stacked_walls_and_a_stuck_entity_are_warnings() {
    let mut r = good_room();
    r.place("Wall", 0, 0);
    r.place("Player", 1, 0);
    r.placed.retain(|p| !(p.object == "Player" && p.x == 2));
    let m = messages(&r);
    assert!(
        m.iter().any(|s| s.contains("stacked on tile (0, 0)")),
        "{m:?}"
    );
    assert!(
        m.iter().any(|s| s.contains("inside a solid object")),
        "{m:?}"
    );
    assert!(r.is_playable(), "these are warnings, not errors");
}

#[test]
fn pointless_options_are_called_out_by_name() {
    let mut r = good_room();
    r.define(ObjectDef::with_kind(
        "DeadLamp",
        Kind::Light {
            radius_tiles: 0,
            color: [255, 255, 255],
            intensity_percent: 100,
        },
    ));
    r.define(ObjectDef::with_kind(
        "DryWell",
        Kind::SubstanceSource {
            substance: Substance::Water,
            rate_per_tick: 0,
        },
    ));
    r.define(ObjectDef::with_kind(
        "Glue",
        Kind::Environment {
            walkable: true,
            speed_percent: 0,
        },
    ));
    let m = messages(&r);
    for name in ["DeadLamp", "DryWell", "Glue"] {
        assert!(
            m.iter().any(|s| s.contains(name)),
            "no message about {name}: {m:?}"
        );
    }
}

#[test]
fn errors_are_listed_before_warnings() {
    let mut r = good_room();
    r.placed.retain(|p| p.object != "Player"); // warning
    r.place("Nope", 1, 1); // error
    let p = r.validate();
    assert_eq!(p[0].severity, Severity::Error);
    assert_eq!(p.last().unwrap().severity, Severity::Warning);
}

/// The sample room shipped in the repo must stay loadable and clean, so the
/// first thing a new user opens never shows a problem. Regenerate it with
/// `cargo run -p ianus_objects --example write_cellar` if the format changes.
#[test]
fn the_shipped_sample_room_parses_and_is_clean() {
    let text = include_str!("../../../rooms/cellar.room");
    let room = Room::from_text(text).expect("rooms/cellar.room must parse");
    assert_eq!(room.name, "Cellar");
    assert_eq!(room.validate(), vec![], "sample room has problems");
    assert!(room.is_playable());
    for c in Category::ALL {
        assert!(
            room.objects.iter().any(|o| o.category() == c),
            "the sample should show off every category, missing {c:?}"
        );
    }
}

#[test]
fn a_switch_pointing_at_nothing_is_flagged_with_a_suggestion() {
    let mut r = good_room();
    r.define(ObjectDef::with_kind(
        "Switch",
        Kind::Interactable {
            action: Action::Toggle("Torh".into()),
            radius_tiles: 1,
            once: false,
        },
    ));
    let m = messages(&r);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("toggles 'Torh'") && m[0].contains("Did you mean 'Torch'?"),
        "{}",
        m[0]
    );

    // And a switch that points at a real object is fine.
    r.objects.retain(|o| o.name != "Switch");
    r.define(ObjectDef::with_kind(
        "Switch",
        Kind::Interactable {
            action: Action::Toggle("Torch".into()),
            radius_tiles: 1,
            once: false,
        },
    ));
    assert_eq!(messages(&r), Vec::<String>::new());
}

#[test]
fn edit_distance_basics() {
    assert_eq!(distance_for_tests("wall", "wall"), 0);
    assert_eq!(distance_for_tests("wal", "wall"), 1);
    assert_eq!(distance_for_tests("Wall", "wall"), 0, "case does not count");
    assert_eq!(distance_for_tests("", "abc"), 3);
    assert_eq!(distance_for_tests("kitten", "sitting"), 3);
}

#[test]
fn suggestions_only_for_close_names() {
    let known = ["Player", "Wall", "Torch"];
    assert_eq!(suggest_for_tests("Plyer", &known), Some("Player"));
    assert_eq!(suggest_for_tests("torch", &known), Some("Torch"));
    assert_eq!(suggest_for_tests("Dragon", &known), None);
    // Short names must be nearly exact, so "Cat" does not suggest "Wall".
    assert_eq!(suggest_for_tests("Cat", &known), None);
}

// ---- hitboxes, collision switches, tick rates, world rules ----

#[test]
fn defaults_follow_the_kind() {
    let wall = ObjectDef::new("W", Category::Wall);
    assert!(wall.collision && wall.solid && wall.blocks());
    assert_eq!(wall.ob_tick, 0, "walls never update");
    assert!(wall.hitbox.is_full_tile());

    let player = player();
    assert!(player.collision && !player.solid);
    assert_eq!(player.ob_tick, 1);
    assert_eq!(player.hitbox, Hitbox::square(75));

    let pit = ObjectDef::with_kind(
        "Pit",
        Kind::Environment {
            walkable: false,
            speed_percent: 100,
        },
    );
    assert!(pit.solid, "ground you cannot walk on is solid");
    assert!(!ObjectDef::new("Mud", Category::Environment).solid);
}

#[test]
fn blocking_needs_both_switches() {
    let mut w = ObjectDef::new("W", Category::Wall);
    assert!(w.blocks());
    w.collision = false;
    assert!(!w.blocks(), "the master switch wins");
    w.collision = true;
    w.solid = false;
    assert!(!w.blocks());
}

#[test]
fn the_settings_come_first_in_the_file_and_roundtrip() {
    let mut r = good_room();
    r.world = WorldRules {
        collision: false,
        ob_tick_scale: 3,
    };
    r.objects[0].ob_tick = 12;
    r.objects[0].hitbox = Hitbox {
        width: 50,
        height: 80,
        offset_x: -10,
        offset_y: 25,
    };
    let text = r.to_text();
    assert_eq!(Room::from_text(&text).unwrap(), r);

    let name = text.find("name: \"Player\"").unwrap();
    let ob_tick = text[name..].find("ob_tick").unwrap();
    let kind = text[name..].find("kind:").unwrap();
    assert!(
        ob_tick < kind,
        "ob_tick must come before the kind's options"
    );
    assert!(
        text.find("world:").unwrap() < text.find("objects:").unwrap(),
        "world rules come before objects"
    );
}

#[test]
fn a_file_without_the_new_settings_still_loads_with_defaults() {
    let text = r#"(
        name: "Old", width: 4, height: 4,
        objects: [(name: "Thing", kind: Interactable(action: Message("hi"), radius_tiles: 1, once: false))],
        placed: [],
    )"#;
    let room = Room::from_text(text).unwrap();
    let thing = &room.objects[0];
    assert_eq!(thing.ob_tick, 1);
    assert!(thing.collision && !thing.solid);
    assert_eq!(thing.hitbox, Hitbox::TILE);
    assert_eq!(room.world, WorldRules::default());
}

#[test]
fn contradictory_switches_are_explained() {
    let mut r = good_room();
    r.objects[1].collision = false; // the Wall: still solid, collision off
    let m = messages(&r);
    assert!(
        m.iter()
            .any(|s| s.contains("'Wall' is solid but its collision is off")),
        "{m:?}"
    );

    let mut r = good_room();
    r.objects[1].solid = false;
    let m = messages(&r);
    assert!(
        m.iter().any(|s| s.contains("Wall 'Wall' is not solid")),
        "{m:?}"
    );

    let mut r = good_room();
    r.objects[0].hitbox.width = 0;
    let m = messages(&r);
    assert!(m.iter().any(|s| s.contains("hitbox of 0x75")), "{m:?}");
    assert!(r.is_playable(), "all of these are warnings");
}

#[test]
fn an_object_with_collision_off_is_not_reported_as_stuck_in_a_wall() {
    let mut r = good_room();
    r.placed.retain(|p| p.object != "Player");
    r.place("Player", 0, 0); // on a wall
    assert!(
        messages(&r)
            .iter()
            .any(|s| s.contains("inside a solid object"))
    );
    r.objects[0].collision = false;
    assert!(
        !messages(&r)
            .iter()
            .any(|s| s.contains("inside a solid object"))
    );
}

#[test]
fn tick_rate_mistakes_are_explained() {
    let mut r = good_room();
    r.objects[0].ob_tick = 0;
    assert!(
        messages(&r)
            .iter()
            .any(|s| s.contains("ob_tick 0") && s.contains("still move"))
    );

    let mut r = good_room();
    r.objects[2].ob_tick = 999_999_999;
    assert!(messages(&r).iter().any(|s| s.contains("more than an hour")));

    let mut r = good_room();
    r.world.ob_tick_scale = 0;
    assert!(
        messages(&r)
            .iter()
            .any(|s| s.contains("ob_tick_scale is 0"))
    );
}

#[test]
fn a_light_with_two_pointless_options_gets_both_warnings() {
    let mut r = good_room();
    r.objects.retain(|o| o.name != "Torch");
    r.define(ObjectDef::with_kind(
        "Torch",
        Kind::Light {
            radius_tiles: 0,
            color: [255, 255, 255],
            intensity_percent: 150,
        },
    ));
    let m = messages(&r);
    assert!(m.iter().any(|s| s.contains("radius 0")), "{m:?}");
    assert!(m.iter().any(|s| s.contains("intensity 150%")), "{m:?}");
}

#[test]
fn a_level_scale_that_pushes_ob_tick_past_the_limit_is_a_warning() {
    let mut r = good_room();
    let mut slow = ObjectDef::new("Slow", Category::Interactable);
    slow.ob_tick = 600;
    r.define(slow);
    r.world.ob_tick_scale = 1000;
    let m = messages(&r);
    assert!(
        m.iter()
            .any(|s| s.contains("'Slow'") && s.contains("ob_tick_scale")),
        "{m:?}"
    );
}

#[test]
fn a_zero_size_hitbox_on_something_that_moves_says_it_ignores_walls() {
    let mut r = good_room();
    r.objects[0].hitbox = Hitbox::square(0);
    let m = messages(&r);
    assert!(m.iter().any(|s| s.contains("walk through walls")), "{m:?}");
}

// ---- Nav tags and agents ----

fn chaser() -> ObjectDef {
    let mut d = ObjectDef::with_kind(
        "Hunter",
        Kind::Entity {
            controlled: false,
            speed_tiles_per_s: 4,
        },
    );
    d.agent = Some(AgentDef {
        target: AgentTarget::Controlled,
        allow: vec![],
        avoid: vec!["water".into()],
        stop_distance: 150,
    });
    d
}

fn room_with_hunter() -> Room {
    let mut r = good_room();
    r.declare_tag("water");
    r.declare_tag("road");
    r.define(chaser());
    r.place("Hunter", 6, 6);
    r
}

#[test]
fn a_room_with_tags_painting_and_an_agent_round_trips_through_text() {
    let mut r = room_with_hunter();
    r.paint_tag("water", 4, 4);
    r.paint_tag("road", 5, 4);
    let mut mud = ObjectDef::with_kind(
        "Puddle",
        Kind::Environment {
            walkable: true,
            speed_percent: 100,
        },
    );
    mud.nav_tags = vec!["water".into()];
    r.define(mud);
    r.place("Puddle", 3, 5);
    assert_eq!(r.validate(), vec![], "{:?}", messages(&r));
    let text = r.to_text();
    assert_eq!(Room::from_text(&text).unwrap(), r);
    assert!(text.contains("Hunter") && text.contains("water"));
}

#[test]
fn rooms_without_nav_fields_load_and_save_unchanged() {
    // The field names are new; a room that never mentions them must not
    // start printing them, or every saved room would change.
    let text = good_room().to_text();
    assert!(!text.contains("nav_tags") && !text.contains("nav_paint") && !text.contains("agent"));
    let back = Room::from_text(&text).unwrap();
    assert!(back.nav_tags.is_empty() && back.nav_paint.is_empty());
    assert!(
        back.objects
            .iter()
            .all(|o| o.agent.is_none() && o.nav_tags.is_empty())
    );
}

#[test]
fn tag_bits_follow_declaration_order_and_are_capped() {
    let mut r = Room::new("T", 3, 3);
    assert_eq!(r.declare_tag("a"), Some(1));
    assert_eq!(r.declare_tag("b"), Some(2));
    assert_eq!(
        r.declare_tag("a"),
        Some(1),
        "declaring twice is the same tag"
    );
    assert_eq!(r.tag_mask(&["b".into(), "a".into(), "nope".into()]), 3);
    for i in 2..MAX_NAV_TAGS {
        assert!(r.declare_tag(format!("t{i}")).is_some());
    }
    assert_eq!(r.declare_tag("one too many"), None);
}

#[test]
fn unknown_tags_are_errors_with_a_suggestion() {
    let mut r = room_with_hunter();
    r.nav_paint.push(NavPaint {
        x: 1,
        y: 1,
        tag: "watr".into(),
    });
    r.objects
        .iter_mut()
        .find(|o| o.name == "Hunter")
        .unwrap()
        .agent
        .as_mut()
        .unwrap()
        .allow = vec!["rod".into()];
    let m = messages(&r);
    assert!(
        m.iter()
            .any(|s| s.contains("tile (1, 1)") && s.contains("Did you mean 'water'")),
        "{m:?}"
    );
    assert!(
        m.iter()
            .any(|s| s.contains("agent 'Hunter'") && s.contains("Did you mean 'road'")),
        "{m:?}"
    );
    assert!(!r.is_playable());
}

#[test]
fn bad_paint_positions_and_duplicate_tags_are_errors() {
    let mut r = room_with_hunter();
    r.paint_tag("water", 40, 2);
    r.nav_tags.push("water".into());
    let m = messages(&r);
    assert!(
        m.iter().any(|s| s.contains("outside the 10x8 room")),
        "{m:?}"
    );
    assert!(
        m.iter().any(|s| s.contains("declared more than once")),
        "{m:?}"
    );
}

#[test]
fn agents_on_the_wrong_kind_of_object_are_errors() {
    let mut r = room_with_hunter();
    r.objects
        .iter_mut()
        .find(|o| o.name == "Player")
        .unwrap()
        .agent = Some(AgentDef::default());
    r.objects
        .iter_mut()
        .find(|o| o.name == "Wall")
        .unwrap()
        .agent = Some(AgentDef::default());
    let m = messages(&r);
    assert!(
        m.iter()
            .any(|s| s.contains("'Player'") && s.contains("would fight")),
        "{m:?}"
    );
    assert!(
        m.iter()
            .any(|s| s.contains("'Wall'") && s.contains("not an Entity")),
        "{m:?}"
    );
}

#[test]
fn risky_agent_settings_are_warnings() {
    let mut r = room_with_hunter();
    {
        let h = r.objects.iter_mut().find(|o| o.name == "Hunter").unwrap();
        h.collision = false;
        h.solid = true;
        h.hitbox = Hitbox::square(100);
        h.ob_tick = 0;
        let a = h.agent.as_mut().unwrap();
        a.allow = vec!["water".into()];
        a.target = AgentTarget::Named("Cratee".into());
    }
    r.define(ObjectDef::with_kind(
        "Crate",
        Kind::Entity {
            controlled: false,
            speed_tiles_per_s: 1,
        },
    ));
    let m = messages(&r);
    for needle in [
        "both allows and avoids the tag 'water'",
        "collision off",
        "is solid",
        "hitbox of 100x100",
        "ob_tick 0",
        "Did you mean 'Crate'",
    ] {
        assert!(
            m.iter().any(|s| s.contains(needle)),
            "missing {needle:?} in {m:?}"
        );
    }
    assert!(r.is_playable(), "all of these are warnings, not errors");
}

#[test]
fn an_agent_with_nothing_to_chase_is_a_warning() {
    let mut r = room_with_hunter();
    r.placed.retain(|p| p.object != "Player");
    let m = messages(&r);
    assert!(m.iter().any(|s| s.contains("none is placed")), "{m:?}");
}
