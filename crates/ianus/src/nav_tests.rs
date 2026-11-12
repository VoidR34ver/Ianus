//! Agents and pathfinding through the real schedules.

use bevy_app::{App, FixedUpdate};
use bevy_ecs::prelude::*;
use ianus_objects::{AgentDef, AgentTarget, Hitbox, Kind, ObjectDef};
use ianus_sim::{NavFilter, SUBUNITS_PER_TILE as TILE};

use crate::prelude::*;
use crate::tests::{hold, script, test_app};

const W: u32 = 18;
const H: u32 = 9;
const WALL_X: i32 = 9;

/// An 18x9 room with a border and a vertical wall at x = 9, open only at the
/// rows in `gaps`. The player stands at (15, 2), the hunter at (3, 2).
fn arena(gaps: &[i32], hunter_speed: u32) -> Room {
    let mut r = Room::new("Arena", W, H);
    r.define(ObjectDef::with_kind(
        "Player",
        Kind::Entity {
            controlled: true,
            speed_tiles_per_s: 5,
        },
    ));
    let mut hunter = ObjectDef::with_kind(
        "Hunter",
        Kind::Entity {
            controlled: false,
            speed_tiles_per_s: hunter_speed,
        },
    );
    hunter.agent = Some(AgentDef {
        target: AgentTarget::Controlled,
        allow: vec![],
        avoid: vec!["water".into()],
        stop_distance: 100,
    });
    r.define(hunter);
    r.define(ObjectDef::new("Wall", Category::Wall));
    let mut puddle = ObjectDef::with_kind(
        "Puddle",
        Kind::Environment {
            walkable: true,
            speed_percent: 100,
        },
    );
    puddle.nav_tags = vec!["water".into()];
    // Water has no hitbox, so removing it takes away no collider: the nav map
    // has to notice the tags leaving on its own.
    puddle.collision = false;
    r.define(puddle);
    r.declare_tag("water");
    r.declare_tag("road");
    for x in 0..W as i32 {
        r.place("Wall", x, 0);
        r.place("Wall", x, H as i32 - 1);
    }
    for y in 1..H as i32 - 1 {
        r.place("Wall", 0, y);
        r.place("Wall", W as i32 - 1, y);
        if !gaps.contains(&y) {
            r.place("Wall", WALL_X, y);
        }
    }
    r.place("Player", 15, 2);
    r.place("Hunter", 3, 2);
    r
}

fn start(room: Room) -> App {
    let mut app = test_app(1.0);
    load_room(app.world_mut(), room).expect("valid room");
    play(app.world_mut());
    app.update();
    app
}

fn run(app: &mut App, ticks: u32) {
    for _ in 0..ticks {
        app.update();
    }
}

fn entity(app: &mut App, name: &str) -> Entity {
    let mut q = app.world_mut().query::<(Entity, &PlacedObject)>();
    q.iter(app.world())
        .find(|(_, p)| p.name == name)
        .map(|(e, _)| e)
        .unwrap_or_else(|| panic!("no {name}"))
}

fn body(app: &mut App, name: &str) -> Body {
    let e = entity(app, name);
    app.world().get::<SimBody>(e).unwrap().0
}

fn tile(b: &Body) -> (i32, i32) {
    (ianus_sim::tile_of(b.pos.x), ianus_sim::tile_of(b.pos.y))
}

fn status(app: &mut App) -> NavStatus {
    let e = entity(app, "Hunter");
    app.world().get::<NavPath>(e).unwrap().status
}

fn dist_tiles(a: &Body, b: &Body) -> f64 {
    let dx = f64::from(a.pos.x - b.pos.x);
    let dy = f64::from(a.pos.y - b.pos.y);
    (dx * dx + dy * dy).sqrt() / f64::from(TILE)
}

fn inside_a_solid(app: &mut App, b: &Body) -> bool {
    let solid = app.world().resource::<SolidMap>().0.clone();
    solid.overlaps(&Footprint::DEFAULT.aabb_at(b.pos.x, b.pos.y))
}

/// Run until `done` or `max` ticks; returns the ticks used.
fn run_until(app: &mut App, max: u32, mut done: impl FnMut(&mut App) -> bool) -> u32 {
    for t in 0..max {
        app.update();
        if done(app) {
            return t;
        }
    }
    panic!("did not happen within {max} ticks");
}

#[test]
fn the_test_arena_is_a_clean_room() {
    assert_eq!(arena(&[7], 5).validate(), vec![]);
    assert_eq!(arena(&[2, 7], 3).validate(), vec![]);
}

#[test]
fn an_agent_chases_the_player_around_a_wall_through_the_gap() {
    let mut app = start(arena(&[7], 5));
    let mut highest = 0;
    run_until(&mut app, 900, |app| {
        let h = body(app, "Hunter");
        highest = highest.max(h.pos.y);
        assert!(!inside_a_solid(app, &h), "agent inside a wall");
        status(app) == NavStatus::Arrived
    });
    let (h, p) = (body(&mut app, "Hunter"), body(&mut app, "Player"));
    assert!(
        dist_tiles(&h, &p) < 1.3,
        "stopped {} tiles away",
        dist_tiles(&h, &p)
    );
    assert!(
        highest > 6 * TILE,
        "never went up to the gap at row 7 (highest y {highest})"
    );
    assert!(
        h.pos.x > WALL_X * TILE,
        "ended on the wrong side of the wall"
    );
}

#[test]
fn without_a_target_an_agent_stands_still() {
    let mut app = start(arena(&[7], 5));
    let e = entity(&mut app, "Hunter");
    app.world_mut().get_mut::<NavAgent>(e).unwrap().target = NavTarget::None;
    let before = body(&mut app, "Hunter");
    run(&mut app, 200);
    assert_eq!(body(&mut app, "Hunter"), before);
    assert_eq!(status(&mut app), NavStatus::Idle);
}

#[test]
fn an_agent_avoids_tiles_with_a_tag_it_avoids() {
    // Gaps at rows 2 (straight ahead, painted as water) and 7 (far).
    let mut room = arena(&[2, 7], 5);
    room.paint_tag("water", WALL_X, 2);
    let mut app = start(room.clone());
    let mut visited_water = false;
    let mut highest = 0;
    run_until(&mut app, 1200, |app| {
        let h = body(app, "Hunter");
        highest = highest.max(h.pos.y);
        visited_water |= tile(&h) == (WALL_X, 2);
        status(app) == NavStatus::Arrived
    });
    assert!(!visited_water, "walked through the water it avoids");
    assert!(highest > 6 * TILE, "should have gone round by the far gap");

    // The same room, an agent that does not mind water: straight through.
    let mut app = start(room);
    let e = entity(&mut app, "Hunter");
    app.world_mut().get_mut::<NavAgent>(e).unwrap().filter = NavFilter::ANY;
    let mut through = false;
    run_until(&mut app, 900, |app| {
        through |= tile(&body(app, "Hunter")) == (WALL_X, 2);
        status(app) == NavStatus::Arrived
    });
    assert!(
        through,
        "an agent with no avoid list should take the short way"
    );
}

#[test]
fn no_allowed_route_means_no_path_until_the_rules_change() {
    let mut room = arena(&[4], 5);
    room.paint_tag("water", WALL_X, 4);
    let mut app = start(room);
    let start_pos = body(&mut app, "Hunter");
    run(&mut app, 300);
    assert_eq!(status(&mut app), NavStatus::NoPath);
    let h = body(&mut app, "Hunter");
    assert!(h.pos.x < WALL_X * TILE, "crossed the water anyway");
    assert!(
        dist_tiles(&h, &start_pos) < 1.0,
        "it should wait, not wander"
    );

    // Changing what it may walk on is noticed without anything else moving.
    let e = entity(&mut app, "Hunter");
    app.world_mut().get_mut::<NavAgent>(e).unwrap().filter = NavFilter::ANY;
    run_until(&mut app, 900, |app| status(app) == NavStatus::Arrived);
}

#[test]
fn an_agent_that_may_only_use_roads_stays_on_the_road() {
    let mut room = arena(&[4], 5);
    for x in 1..W as i32 - 1 {
        room.paint_tag("road", x, 4);
    }
    // Player and hunter both stand on the road.
    for p in room.placed.iter_mut() {
        if p.object == "Player" || p.object == "Hunter" {
            p.y = 4;
        }
    }
    let mut app = start(room);
    let e = entity(&mut app, "Hunter");
    {
        let mut agent = app.world_mut().get_mut::<NavAgent>(e).unwrap();
        agent.filter = NavFilter {
            allow: 0b10, // "road" is the second tag declared
            deny: 0,
        };
    }
    run_until(&mut app, 900, |app| {
        let h = body(app, "Hunter");
        assert_eq!(tile(&h).1, 4, "left the road: tile {:?}", tile(&h));
        status(app) == NavStatus::Arrived
    });
    let (h, p) = (body(&mut app, "Hunter"), body(&mut app, "Player"));
    assert!(dist_tiles(&h, &p) < 1.3);
}

#[test]
fn objects_carry_nav_tags_and_removing_one_changes_the_routes() {
    let mut room = arena(&[2, 7], 5);
    room.place("Puddle", WALL_X, 2);
    let mut app = start(room);
    // Let the first tick's rebuilds happen, so the despawn below is the only
    // thing that can make the nav map change.
    run(&mut app, 10);
    assert!(
        app.world().resource::<NavMap>().grid.tags(WALL_X, 2) != 0,
        "the puddle's tag should be on its tile"
    );
    let version = app.world().resource::<NavMap>().version;
    let puddle = entity(&mut app, "Puddle");
    app.world_mut().despawn(puddle);
    run(&mut app, 3);
    let nav = app.world().resource::<NavMap>();
    assert_eq!(nav.grid.tags(WALL_X, 2), 0, "the tag left with the object");
    assert!(
        nav.version > version,
        "routes planned before must be replanned"
    );

    let mut through = false;
    run_until(&mut app, 900, |app| {
        through |= tile(&body(app, "Hunter")) == (WALL_X, 2);
        status(app) == NavStatus::Arrived
    });
    assert!(through, "with the puddle gone the short way is open");
}

#[test]
fn a_wall_removed_during_play_opens_a_route() {
    let mut app = start(arena(&[], 5));
    run(&mut app, 60);
    assert_eq!(status(&mut app), NavStatus::NoPath, "fully walled off");
    let mut q = app.world_mut().query::<(Entity, &PlacedObject)>();
    let wall = q
        .iter(app.world())
        .find(|(_, p)| p.name == "Wall" && p.home == Vec2i::new(WALL_X * TILE, 4 * TILE))
        .map(|(e, _)| e)
        .unwrap();
    app.world_mut().despawn(wall);
    run_until(&mut app, 900, |app| status(app) == NavStatus::Arrived);
}

#[test]
fn code_can_send_an_agent_to_a_tile() {
    let mut app = start(arena(&[7], 5));
    let e = entity(&mut app, "Hunter");
    app.world_mut().get_mut::<NavAgent>(e).unwrap().target = NavTarget::Tile(15, 6);
    run_until(&mut app, 900, |app| status(app) == NavStatus::Arrived);
    let h = body(&mut app, "Hunter");
    let goal = Vec2i::new(15 * TILE, 6 * TILE);
    let d = f64::from(h.pos.x - goal.x).hypot(f64::from(h.pos.y - goal.y));
    assert!(d < 700.0, "arrived {d} subunits from the tile centre");
}

#[test]
fn an_agent_can_chase_an_object_by_name() {
    let mut room = arena(&[7], 5);
    room.define(ObjectDef::with_kind(
        "Crate",
        Kind::Entity {
            controlled: false,
            speed_tiles_per_s: 1,
        },
    ));
    room.place("Crate", 4, 6);
    room.objects
        .iter_mut()
        .find(|o| o.name == "Hunter")
        .unwrap()
        .agent
        .as_mut()
        .unwrap()
        .target = AgentTarget::Named("Crate".into());
    let mut app = start(room);
    run_until(&mut app, 600, |app| status(app) == NavStatus::Arrived);
    let (h, c) = (body(&mut app, "Hunter"), body(&mut app, "Crate"));
    assert!(dist_tiles(&h, &c) < 1.3);
    assert!(
        h.pos.x < WALL_X * TILE,
        "it should not have gone after the player"
    );
}

#[test]
fn an_agents_speed_comes_from_its_definition() {
    let travelled = |speed: u32| {
        let mut room = arena(&[7], speed);
        room.placed.retain(|p| p.object != "Wall" || p.x != WALL_X);
        let mut app = start(room);
        let before = body(&mut app, "Hunter");
        run(&mut app, 120);
        let after = body(&mut app, "Hunter");
        (after.pos.x - before.pos.x, after.vel.x)
    };
    let (slow, slow_v) = travelled(2);
    let (fast, fast_v) = travelled(5);
    assert!(slow_v <= ianus_sim::MAX_SPEED * 2 / 5 + 1, "{slow_v}");
    assert!(fast_v <= ianus_sim::MAX_SPEED);
    // 120 ticks at 2 tiles/s is at most 4 tiles; at 5, well over 8.
    assert!(slow <= 4 * TILE + TILE / 8, "{slow}");
    assert!(fast >= 8 * TILE, "{fast}");
}

#[test]
fn walking_agents_do_not_make_the_nav_map_rebuild() {
    let mut app = start(arena(&[7], 5));
    let version = app.world().resource::<NavMap>().version;
    run_until(&mut app, 900, |app| status(app) == NavStatus::Arrived);
    assert_eq!(
        app.world().resource::<NavMap>().version,
        version,
        "a creature walking about must not invalidate everyone's routes"
    );
}

#[derive(Resource, Default)]
struct ChangeCount(u32);

#[derive(Resource, Default)]
struct SolidMapChanges(u32);

fn count_solid_map_changes(solids: Res<SolidMap>, mut n: ResMut<SolidMapChanges>) {
    if solids.is_changed() {
        n.0 += 1;
    }
}

#[test]
fn a_creature_walking_does_not_touch_the_solid_map() {
    let mut app = start(arena(&[7], 5));
    app.init_resource::<SolidMapChanges>();
    app.add_systems(
        FixedUpdate,
        count_solid_map_changes.after(crate::IanusSet::Detect),
    );
    run(&mut app, 10); // the first ticks build everything
    app.world_mut().resource_mut::<SolidMapChanges>().0 = 0;
    run(&mut app, 200);
    assert!(
        body(&mut app, "Hunter").pos.x > 4 * TILE,
        "the agent should be walking"
    );
    assert_eq!(
        app.world().resource::<SolidMapChanges>().0,
        0,
        "moving a non-solid collider replaced the solid map, so the nav map would be rebuilt every tick"
    );
}

fn count_changes(mut n: ResMut<ChangeCount>, q: Query<(), (With<NavAgent>, Changed<SimBody>)>) {
    n.0 += q.iter().count() as u32;
}

#[test]
fn an_agent_at_rest_is_not_flagged_as_changed() {
    let mut app = start(arena(&[7], 5));
    app.init_resource::<ChangeCount>();
    app.add_systems(FixedUpdate, count_changes.after(crate::IanusSet::Detect));
    run_until(&mut app, 900, |app| status(app) == NavStatus::Arrived);
    run(&mut app, 120); // let it come to rest
    app.world_mut().resource_mut::<ChangeCount>().0 = 0;
    run(&mut app, 120);
    assert_eq!(
        app.world().resource::<ChangeCount>().0,
        0,
        "a resting agent churned its change flag"
    );
}

fn hash(app: &mut App) -> u64 {
    let mut q = app.world_mut().query::<&SimBody>();
    world_hash(q.iter(app.world()))
}

#[test]
fn stop_and_play_again_reproduces_the_agent_exactly() {
    let mut app = start(arena(&[7], 5));
    run(&mut app, 240);
    let first = hash(&mut app);
    assert!(stop(app.world_mut()));
    assert!(play(app.world_mut()));
    run(&mut app, 240);
    assert_eq!(hash(&mut app), first, "a second run planned differently");
}

#[test]
fn replays_with_agents_in_them_still_match() {
    let mut app = start(arena(&[7], 5));
    start_recording(app.world_mut(), 11).unwrap();
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
            assert_eq!(matched, Some(true), "agents made the replay diverge");
            return;
        }
    }
    panic!("playback never finished");
}

#[test]
fn restart_clears_routes() {
    let mut app = start(arena(&[7], 5));
    run(&mut app, 100);
    let e = entity(&mut app, "Hunter");
    assert_eq!(
        app.world().get::<NavPath>(e).unwrap().status,
        NavStatus::Following
    );
    restart_world(app.world_mut()).unwrap();
    assert_eq!(*app.world().get::<NavPath>(e).unwrap(), NavPath::default());
}

#[test]
fn a_big_hitbox_is_a_validator_warning_not_a_crash() {
    let mut room = arena(&[7], 5);
    room.objects
        .iter_mut()
        .find(|o| o.name == "Hunter")
        .unwrap()
        .hitbox = Hitbox::square(100);
    let problems = room.validate();
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("hitbox of 100x100"))
    );
}

#[test]
fn an_agent_too_big_for_the_gap_gives_up_instead_of_jittering_forever() {
    // Routes are planned for a 0.75-tile body. A 1.1-tile one cannot fit the
    // one-tile gap, so it gets stuck at the wall; after three tries it stops.
    let mut room = arena(&[4], 5);
    room.objects
        .iter_mut()
        .find(|o| o.name == "Hunter")
        .unwrap()
        .hitbox = Hitbox::square(110);
    let mut app = start(room);
    run_until(&mut app, 1500, |app| status(app) == NavStatus::NoPath);
    let h = body(&mut app, "Hunter");
    assert!(
        h.pos.x < WALL_X * TILE,
        "squeezed through a gap it cannot fit"
    );
    let settled = body(&mut app, "Hunter");
    run(&mut app, 200);
    assert_eq!(
        body(&mut app, "Hunter"),
        settled,
        "it should stand still once it gives up"
    );
}

#[test]
fn the_shipped_chase_room_does_what_its_description_says() {
    let room = Room::from_text(include_str!("../../../rooms/chase.room")).expect("parses");
    assert_eq!(room.validate(), vec![], "the demo room must be clean");
    let mut app = test_app(1.0);
    load_room(app.world_mut(), room).unwrap();
    play(app.world_mut());

    let (mut bot_in_water, mut wader_in_water) = (false, false);
    let mut bot_done = false;
    let mut wader_done = false;
    let is_water = |t: (i32, i32)| t.0 == 12 && t.1 != 6 && t.1 != 7;
    for _ in 0..5000 {
        app.update();
        let (bot, wader) = (body(&mut app, "Nextbot"), body(&mut app, "Wader"));
        bot_in_water |= is_water(tile(&bot));
        wader_in_water |= is_water(tile(&wader));
        for b in [&bot, &wader] {
            assert!(!inside_a_solid(&mut app, b), "an agent went into a wall");
        }
        let status_of = |app: &mut App, name: &str| {
            let e = entity(app, name);
            app.world().get::<NavPath>(e).unwrap().status
        };
        bot_done = status_of(&mut app, "Nextbot") == NavStatus::Arrived;
        wader_done = status_of(&mut app, "Wader") == NavStatus::Arrived;
        if bot_done && wader_done {
            break;
        }
    }
    assert!(
        bot_done && wader_done,
        "both agents should reach the player"
    );
    assert!(
        !bot_in_water,
        "the Nextbot avoids water, so it must use the bridge"
    );
    assert!(
        wader_in_water,
        "the Wader has no avoid list and takes the short way"
    );
    let p = body(&mut app, "Player");
    for name in ["Nextbot", "Wader"] {
        let b = body(&mut app, name);
        assert!(
            dist_tiles(&b, &p) < 1.3,
            "{name} stopped {} tiles away",
            dist_tiles(&b, &p)
        );
    }
}

/// Reachability by flood fill, written separately from the A* it checks:
/// eight directions, and no diagonal squeeze between two blocked corners.
fn reachable(open: &[Vec<bool>], from: (usize, usize), to: (usize, usize)) -> bool {
    let (w, h) = (open[0].len() as i32, open.len() as i32);
    let ok = |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && open[y as usize][x as usize];
    let mut seen = vec![vec![false; w as usize]; h as usize];
    let mut stack = vec![(from.0 as i32, from.1 as i32)];
    seen[from.1][from.0] = true;
    while let Some((x, y)) = stack.pop() {
        if (x as usize, y as usize) == to {
            return true;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (nx, ny) = (x + dx, y + dy);
                if (dx, dy) == (0, 0) || !ok(nx, ny) || seen[ny as usize][nx as usize] {
                    continue;
                }
                if dx != 0 && dy != 0 && !(ok(x + dx, y) && ok(x, y + dy)) {
                    continue;
                }
                seen[ny as usize][nx as usize] = true;
                stack.push((nx, ny));
            }
        }
    }
    false
}

#[test]
fn in_random_mazes_an_agent_arrives_exactly_when_the_player_is_reachable() {
    let (w, h) = (16usize, 12usize);
    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut next = move |n: u32| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 33) as u32) % n
    };
    let (mut arrived, mut cut_off) = (0, 0);
    for case in 0..60 {
        let mut open = vec![vec![true; w]; h];
        for (y, row) in open.iter_mut().enumerate() {
            for (x, cell) in row.iter_mut().enumerate() {
                let border = x == 0 || y == 0 || x == w - 1 || y == h - 1;
                *cell = !border && next(100) >= 30;
            }
        }
        let pick = |next: &mut dyn FnMut(u32) -> u32, open: &[Vec<bool>]| loop {
            let (x, y) = (next(w as u32) as usize, next(h as u32) as usize);
            if open[y][x] {
                return (x, y);
            }
        };
        let p = pick(&mut next, &open);
        let a = loop {
            let c = pick(&mut next, &open);
            if c != p {
                break c;
            }
        };

        let mut room = arena(&[], 5);
        room.placed.clear();
        for (y, row) in open.iter().enumerate() {
            for (x, &cell) in row.iter().enumerate() {
                if !cell {
                    room.place("Wall", x as i32, y as i32);
                }
            }
        }
        room.width = w as u32;
        room.height = h as u32;
        room.place("Player", p.0 as i32, p.1 as i32);
        room.place("Hunter", a.0 as i32, a.1 as i32);

        let expect_arrive = reachable(&open, a, p);
        let mut app = start(room);
        let mut end = NavStatus::Idle;
        for _ in 0..3000 {
            app.update();
            let hb = body(&mut app, "Hunter");
            assert!(
                !inside_a_solid(&mut app, &hb),
                "case {case}: agent inside a wall"
            );
            end = status(&mut app);
            if matches!(end, NavStatus::Arrived | NavStatus::NoPath) {
                break;
            }
        }
        if expect_arrive {
            arrived += 1;
            if end != NavStatus::Arrived {
                let mut map = String::new();
                for (y, row) in open.iter().enumerate().rev() {
                    for (x, &c) in row.iter().enumerate() {
                        map.push(match (x, y) {
                            t if t == a => 'H',
                            t if t == p => 'P',
                            _ if c => '.',
                            _ => '#',
                        });
                    }
                    map.push('\n');
                }
                let e = entity(&mut app, "Hunter");
                let pos = body(&mut app, "Hunter").pos;
                let path = app.world().get::<NavPath>(e).unwrap().clone();
                panic!(
                    "case {case}: reachable, but ended {end:?}\n{map}hunter at {pos:?}\n{path:?}"
                );
            }
            let (hb, pb) = (body(&mut app, "Hunter"), body(&mut app, "Player"));
            assert!(
                dist_tiles(&hb, &pb) < 1.6,
                "case {case}: stopped {} tiles away",
                dist_tiles(&hb, &pb)
            );
        } else {
            cut_off += 1;
            assert_eq!(
                end,
                NavStatus::NoPath,
                "case {case}: cut off, but ended {end:?}"
            );
        }
    }
    assert!(
        arrived > 15 && cut_off > 5,
        "weak test: {arrived} reachable, {cut_off} cut off"
    );
}
