//! The pathfinding side of the editor: an agent's settings in the inspector, a
//! brush for painting nav tags onto tiles, and overlays that draw routes, the
//! search window and tagged tiles in the game view.
//!
//! Everything here is a plain function over plain data (a `NavAgent`, a
//! `NavPath`, a list of tag names, a screen-conversion closure), not a Bevy
//! system, so it can be exercised in tests with an egui context and no window.
//! `gui.rs` does the wiring.
//!
//! Edits made here change the *loaded* room and the running agents. They are
//! not written back to the room file (there is no save yet), so reloading the
//! file puts back what it says.

use bevy_egui::egui;
use egui::{Color32, Stroke, StrokeKind};
use ianus::prelude::*;
use ianus_sim::tile_of;

use crate::Action;

/// Which tool the mouse has in the game view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Brush {
    /// Clicking selects things.
    #[default]
    Off,
    /// Paint nav tag number `n` (its position in the room's tag list).
    Paint(usize),
    /// Take every painted tag off the tile.
    Erase,
}

/// Editor state for the pathfinding tools.
#[derive(bevy::prelude::Resource, Debug, Clone, PartialEq, Eq)]
pub struct NavTools {
    pub show_routes: bool,
    pub show_tags: bool,
    pub brush: Brush,
    /// The last tile painted in this drag, so holding the button over one tile
    /// sends one request, not sixty a second.
    pub last_tile: Option<(i32, i32)>,
}

impl Default for NavTools {
    fn default() -> Self {
        Self {
            show_routes: true,
            show_tags: true,
            brush: Brush::Off,
            last_tile: None,
        }
    }
}

// ---- Pure helpers (tested without a window) ----

/// The tile under a point in world units (a tile `(x, y)` is centred on
/// `(x, y) * tile_size`).
pub(crate) fn tile_at(world_x: f32, world_y: f32, tile_size: f32) -> (i32, i32) {
    (
        (world_x / tile_size).round() as i32,
        (world_y / tile_size).round() as i32,
    )
}

/// The request a mouse press or drag over `tile` should send, if any. Returns
/// `None` when the brush is off, or the drag is still on the tile it last
/// painted.
pub(crate) fn paint_action(
    brush: Brush,
    right_button: bool,
    tile: (i32, i32),
    tags: &[String],
    last: &mut Option<(i32, i32)>,
) -> Option<Action> {
    if brush == Brush::Off || *last == Some(tile) {
        return None;
    }
    let op = if right_button || brush == Brush::Erase {
        PaintOp::Clear
    } else if let Brush::Paint(i) = brush {
        PaintOp::Add(tags.get(i)?.clone())
    } else {
        return None;
    };
    *last = Some(tile);
    Some(Action::PaintNavTile {
        x: tile.0,
        y: tile.1,
        op,
    })
}

/// A colour for tag number `i`, the same everywhere (overlay and legend).
pub(crate) fn tag_colour(i: usize, alpha: u8) -> Color32 {
    const PALETTE: [(u8, u8, u8); 8] = [
        (70, 130, 255),
        (235, 170, 60),
        (230, 80, 80),
        (90, 200, 120),
        (180, 110, 230),
        (70, 200, 210),
        (230, 130, 190),
        (160, 160, 90),
    ];
    let (r, g, b) = PALETTE[i % PALETTE.len()];
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// Tiles per second for a top speed in subunits per tick.
pub(crate) fn tiles_per_second(max_speed: i32) -> f32 {
    max_speed as f32 * TICK_HZ as f32 / SUBUNITS_PER_TILE as f32
}

/// Top speed in subunits per tick for tiles per second, rounded down like the
/// engine's own conversion, so 5 gives the player's 341.
pub(crate) fn speed_from_tiles(tiles_per_second: f32) -> i32 {
    ((tiles_per_second * SUBUNITS_PER_TILE as f32 / TICK_HZ as f32 + 1e-3).floor() as i32).max(0)
}

pub(crate) fn tiles_to_subunits(tiles: f32) -> i32 {
    ((tiles * SUBUNITS_PER_TILE as f32).round() as i32).max(0)
}

fn bit(mask: u32, i: usize) -> bool {
    i < 32 && mask >> i & 1 == 1
}

fn set_bit(mask: u32, i: usize, on: bool) -> u32 {
    if i >= 32 {
        mask
    } else if on {
        mask | 1 << i
    } else {
        mask & !(1 << i)
    }
}

/// The agent with tag `i` allowed or not.
pub(crate) fn with_allowed(agent: &NavAgent, i: usize, on: bool) -> NavAgent {
    let mut a = agent.clone();
    a.filter = NavFilter {
        allow: set_bit(a.filter.allow, i, on),
        ..a.filter
    };
    a
}

/// The agent with tag `i` avoided or not.
pub(crate) fn with_avoided(agent: &NavAgent, i: usize, on: bool) -> NavAgent {
    let mut a = agent.clone();
    a.filter = NavFilter {
        deny: set_bit(a.filter.deny, i, on),
        ..a.filter
    };
    a
}

/// The agent with tag `i` costing `extra` more to step on.
pub(crate) fn with_cost(agent: &NavAgent, i: usize, extra: u8) -> NavAgent {
    let mut a = agent.clone();
    a.costs = a.costs.with(i, extra);
    a
}

fn target_label(target: &NavTarget) -> String {
    match target {
        NavTarget::None => "nothing".into(),
        NavTarget::Controlled => "the player".into(),
        NavTarget::Named(n) => format!("the object '{n}'"),
        NavTarget::Entity(e) => format!("entity {}", e.index()),
        NavTarget::Tile(x, y) => format!("tile ({x}, {y})"),
    }
}

fn status_colour(status: NavStatus) -> Color32 {
    match status {
        NavStatus::Idle => Color32::from_gray(150),
        NavStatus::Following => Color32::from_rgb(120, 170, 255),
        NavStatus::Arrived => Color32::from_rgb(110, 210, 130),
        NavStatus::NoPath => Color32::from_rgb(240, 110, 110),
    }
}

fn status_text(status: NavStatus) -> &'static str {
    match status {
        NavStatus::Idle => "Idle (no target, or has not planned yet)",
        NavStatus::Following => "Following its route",
        NavStatus::Arrived => "Arrived",
        NavStatus::NoPath => "No route found",
    }
}

// ---- Inspector widgets ----

/// An agent's settings and route status. Returns the changed agent when the
/// user edited something, so the caller only writes (and so only flags a
/// change) when there really was one. With `editable` false everything is
/// shown greyed out.
pub(crate) fn agent_editor(
    ui: &mut egui::Ui,
    agent: &NavAgent,
    path: &NavPath,
    agent_tile: (i32, i32),
    tags: &[String],
    editable: bool,
) -> Option<NavAgent> {
    let mut edit = agent.clone();

    ui.horizontal(|ui| {
        ui.colored_label(status_colour(path.status), status_text(path.status));
    });
    ui.weak(format!(
        "chasing {} · {} waypoint{} ahead · route to tile ({}, {})",
        target_label(&agent.target),
        path.waypoints_ahead().len(),
        if path.waypoints_ahead().len() == 1 {
            ""
        } else {
            "s"
        },
        path.goal_tile().0,
        path.goal_tile().1,
    ));
    if path.status != NavStatus::Idle && agent.range != SearchRange::Whole {
        let radius = goal_distance_radius(agent_tile, path.goal_tile());
        ui.weak(format!(
            "search window starts {radius} tile{} each way (the distance to the goal, rounded up)",
            if radius == 1 { "" } else { "s" }
        ));
    }

    ui.add_enabled_ui(editable, |ui| {
        ui.add_space(4.0);
        // Target.
        ui.horizontal(|ui| {
            ui.label("Target");
            let mut named = match &edit.target {
                NavTarget::Named(n) => n.clone(),
                _ => String::new(),
            };
            let (mut tx, mut ty) = match edit.target {
                NavTarget::Tile(x, y) => (x, y),
                _ => (0, 0),
            };
            egui::ComboBox::from_id_salt("nav_target")
                .selected_text(target_label(&edit.target))
                .show_ui(ui, |ui| {
                    if ui.selectable_label(edit.target == NavTarget::None, "nothing").clicked() {
                        edit.target = NavTarget::None;
                    }
                    if ui
                        .selectable_label(edit.target == NavTarget::Controlled, "the player")
                        .clicked()
                    {
                        edit.target = NavTarget::Controlled;
                    }
                    if ui
                        .selectable_label(matches!(edit.target, NavTarget::Named(_)), "a named object")
                        .clicked()
                        && !matches!(edit.target, NavTarget::Named(_))
                    {
                        edit.target = NavTarget::Named(String::new());
                    }
                    if ui
                        .selectable_label(matches!(edit.target, NavTarget::Tile(..)), "a tile")
                        .clicked()
                        && !matches!(edit.target, NavTarget::Tile(..))
                    {
                        edit.target = NavTarget::Tile(0, 0);
                    }
                });
            if matches!(edit.target, NavTarget::Named(_))
                && ui.text_edit_singleline(&mut named).changed()
            {
                edit.target = NavTarget::Named(named);
            }
            if matches!(edit.target, NavTarget::Tile(..)) {
                let a = ui.add(egui::DragValue::new(&mut tx).prefix("x ")).changed();
                let b = ui.add(egui::DragValue::new(&mut ty).prefix("y ")).changed();
                if a || b {
                    edit.target = NavTarget::Tile(tx, ty);
                }
            }
        });

        // Search window.
        ui.horizontal(|ui| {
            ui.label("Search window");
            egui::ComboBox::from_id_salt("nav_range")
                .selected_text(match edit.range {
                    SearchRange::Growing => "growing",
                    SearchRange::Strict => "strict",
                    SearchRange::Whole => "whole map",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut edit.range, SearchRange::Growing, "growing")
                        .on_hover_text(
                            "Search a square as wide as the distance to the goal. If no route is found in it, try one twice as wide, and so on. Never misses a reachable goal.",
                        );
                    ui.selectable_value(&mut edit.range, SearchRange::Strict, "strict")
                        .on_hover_text(
                            "Search only that square. Gives up fast on a goal whose way round is outside it.",
                        );
                    ui.selectable_value(&mut edit.range, SearchRange::Whole, "whole map")
                        .on_hover_text("No window: always search the whole map.");
                });
        });

        // Speed and stop distance.
        ui.horizontal(|ui| {
            ui.label("Speed");
            let mut speed = tiles_per_second(edit.max_speed);
            if ui
                .add(egui::DragValue::new(&mut speed).speed(0.05).range(0.0..=30.0).suffix(" tiles/s"))
                .changed()
            {
                edit.max_speed = speed_from_tiles(speed);
            }
            ui.label("Stops");
            let mut stop = edit.stop_distance as f32 / SUBUNITS_PER_TILE as f32;
            if ui
                .add(egui::DragValue::new(&mut stop).speed(0.02).range(0.0..=20.0).suffix(" tiles away"))
                .changed()
            {
                edit.stop_distance = tiles_to_subunits(stop);
            }
        });

        // Walk rules: one row per tag.
        ui.add_space(4.0);
        if tags.is_empty() {
            ui.weak("This room declares no nav tags, so there is nothing to allow, avoid or put a cost on.");
        } else {
            ui.label("Where it may walk");
            egui::Grid::new("nav_rules").num_columns(4).striped(true).show(ui, |ui| {
                ui.weak("tag");
                ui.weak("only on")
                    .on_hover_text("If any tag is ticked here, the agent walks only on tiles with one of them. Plain ground has no tags.");
                ui.weak("avoid").on_hover_text("Never step on a tile with this tag.");
                ui.weak("extra cost")
                    .on_hover_text("Added to the cost of stepping on such a tile (a plain step is 10). The cheapest total route wins.");
                ui.end_row();
                for (i, name) in tags.iter().enumerate().take(32) {
                    ui.colored_label(tag_colour(i, 255), name);
                    let mut allow = bit(edit.filter.allow, i);
                    if ui.checkbox(&mut allow, "").changed() {
                        edit = with_allowed(&edit, i, allow);
                    }
                    let mut avoid = bit(edit.filter.deny, i);
                    if ui.checkbox(&mut avoid, "").changed() {
                        edit = with_avoided(&edit, i, avoid);
                    }
                    let mut cost = i32::from(edit.costs.get(i));
                    if ui
                        .add_enabled(
                            !avoid,
                            egui::DragValue::new(&mut cost).range(0..=255),
                        )
                        .changed()
                    {
                        edit = with_cost(&edit, i, cost.clamp(0, 255) as u8);
                    }
                    ui.end_row();
                }
            });
        }
    });

    (edit != *agent).then_some(edit)
}

/// The bits of the pathfinding tools that are not about one agent: what to draw,
/// the brush, the tag legend.
pub(crate) fn nav_tools_panel(
    ui: &mut egui::Ui,
    tools: &mut NavTools,
    tags: &[String],
    editing: bool,
    map: Option<(u64, u32, u32)>,
) {
    ui.checkbox(&mut tools.show_routes, "Show agent routes");
    ui.checkbox(&mut tools.show_tags, "Show nav tags on tiles");
    ui.add_space(4.0);

    ui.label("Mouse tool");
    ui.add_enabled_ui(editing, |ui| {
        ui.selectable_value(&mut tools.brush, Brush::Off, "Select things")
            .on_hover_text("Clicking in the game view selects what is under the mouse.");
        for (i, name) in tags.iter().enumerate().take(32) {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, tag_colour(i, 255));
                ui.selectable_value(&mut tools.brush, Brush::Paint(i), format!("Paint '{name}'"))
                    .on_hover_text(
                        "Click or drag over tiles to put this tag on them. Right-click clears.",
                    );
            });
        }
        ui.selectable_value(&mut tools.brush, Brush::Erase, "Erase paint")
            .on_hover_text(
                "Click or drag to take painted tags off tiles. Tags that objects bring stay.",
            );
    });
    if !editing {
        // Painting changes the room, which Stop does not undo.
        ui.weak("Painting works while editing. Press Stop first.");
        tools.brush = Brush::Off;
    } else if tags.is_empty() {
        ui.weak("This room declares no nav tags, so there is nothing to paint.");
    } else {
        ui.weak("Painting changes the loaded room, not the file. Reloading the file puts it back.");
    }
    if let Some((version, w, h)) = map {
        ui.add_space(4.0);
        ui.weak(format!("nav map version {version}, {w}x{h} tiles"));
    }
}

// ---- Game view overlays ----

/// One agent's route, ready to draw.
pub(crate) struct RouteView<'a> {
    pub pos: Vec2i,
    pub path: &'a NavPath,
    pub range: SearchRange,
    pub selected: bool,
}

/// Draw tagged tiles, routes and the selected agent's search window.
/// `to_screen` turns a position in subunits into a screen point (or `None`
/// off screen).
pub(crate) fn draw_nav_overlays(
    painter: &egui::Painter,
    to_screen: &dyn Fn(i32, i32) -> Option<egui::Pos2>,
    nav: Option<&NavMap>,
    tools: &NavTools,
    routes: &[RouteView],
) {
    let half = SUBUNITS_PER_TILE / 2;

    if (tools.show_tags || tools.brush != Brush::Off)
        && let Some(nav) = nav
    {
        {
            let grid = &nav.grid;
            for y in 0..grid.height() as i32 {
                for x in 0..grid.width() as i32 {
                    let tags = grid.tags(x, y);
                    if tags == 0 {
                        continue;
                    }
                    let (cx, cy) = (x * SUBUNITS_PER_TILE, y * SUBUNITS_PER_TILE);
                    let (Some(a), Some(b)) = (
                        to_screen(cx - half, cy - half),
                        to_screen(cx + half, cy + half),
                    ) else {
                        continue;
                    };
                    let rect = egui::Rect::from_two_pos(a, b);
                    // One colour stripe per tag on the tile.
                    let n = tags.count_ones() as f32;
                    let mut k = 0.0;
                    for i in 0..32 {
                        if bit(tags, i) {
                            let w = rect.width() / n;
                            let stripe = egui::Rect::from_min_size(
                                egui::pos2(rect.min.x + k * w, rect.min.y),
                                egui::vec2(w, rect.height()),
                            );
                            painter.rect_filled(stripe, 0.0, tag_colour(i, 90));
                            k += 1.0;
                        }
                    }
                }
            }
        }
    }

    if !tools.show_routes {
        return;
    }
    for route in routes {
        let ahead = route.path.waypoints_ahead();
        let selected = route.selected;
        let (line, dot) = if selected {
            (
                Stroke::new(2.5, Color32::from_rgb(255, 210, 90)),
                Color32::from_rgb(255, 210, 90),
            )
        } else {
            (
                Stroke::new(1.5, Color32::from_rgba_unmultiplied(120, 200, 255, 150)),
                Color32::from_rgba_unmultiplied(120, 200, 255, 150),
            )
        };
        if !ahead.is_empty() {
            let mut points = Vec::with_capacity(ahead.len() + 1);
            if let Some(p) = to_screen(route.pos.x, route.pos.y) {
                points.push(p);
            }
            for w in ahead {
                if let Some(p) = to_screen(w.x, w.y) {
                    points.push(p);
                    if selected {
                        painter.circle_filled(p, 3.5, dot);
                    }
                }
            }
            if points.len() >= 2 {
                painter.add(egui::Shape::line(points, line));
            }
        }
        if route.path.status == NavStatus::NoPath {
            // A red cross on the tile it could not get to.
            let (gx, gy) = route.path.goal_tile();
            if let Some(c) = to_screen(gx * SUBUNITS_PER_TILE, gy * SUBUNITS_PER_TILE) {
                let s = 6.0;
                let red = Stroke::new(2.0, Color32::from_rgb(240, 90, 90));
                painter.line_segment([c + egui::vec2(-s, -s), c + egui::vec2(s, s)], red);
                painter.line_segment([c + egui::vec2(-s, s), c + egui::vec2(s, -s)], red);
            }
        }
        if selected && route.range != SearchRange::Whole && route.path.status != NavStatus::Idle {
            // The first search window: centred on the agent's tile, as wide as
            // the distance to the goal.
            let tile = (tile_of(route.pos.x), tile_of(route.pos.y));
            let r = goal_distance_radius(tile, route.path.goal_tile());
            let (cx, cy) = (tile.0 * SUBUNITS_PER_TILE, tile.1 * SUBUNITS_PER_TILE);
            let reach = r.saturating_mul(SUBUNITS_PER_TILE).saturating_add(half);
            if let (Some(a), Some(b)) = (
                to_screen(cx.saturating_sub(reach), cy.saturating_sub(reach)),
                to_screen(cx.saturating_add(reach), cy.saturating_add(reach)),
            ) {
                painter.rect_stroke(
                    egui::Rect::from_two_pos(a, b),
                    0.0,
                    Stroke::new(1.5, Color32::from_rgb(235, 170, 60)),
                    StrokeKind::Middle,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i32 = SUBUNITS_PER_TILE;

    fn tags() -> Vec<String> {
        vec!["water".into(), "road".into(), "fire".into()]
    }

    /// Run `f` inside one egui frame with no window, and return what it drew.
    fn frame<R>(f: impl FnOnce(&egui::Context, &mut egui::Ui) -> R) -> (R, usize) {
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput::default());
        let mut ui = egui::Ui::new(
            ctx.clone(),
            "test_root".into(),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::background())
                .max_rect(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(480.0, 900.0),
                )),
        );
        let result = f(&ctx, &mut ui);
        let mut output = ctx.end_pass();
        // egui panics if a finished frame is dropped with texture updates
        // nobody applied; there is no renderer here, so discard them.
        output.textures_delta.clear();
        (result, output.shapes.len())
    }

    #[test]
    fn tiles_under_the_mouse_round_to_the_nearest_centre() {
        // A tile (x, y) is centred on (x, y) * size, so a point within half a
        // tile of the centre belongs to that tile.
        assert_eq!(tile_at(0.0, 0.0, 32.0), (0, 0));
        assert_eq!(tile_at(15.9, -15.9, 32.0), (0, 0));
        assert_eq!(tile_at(16.1, -16.1, 32.0), (1, -1));
        assert_eq!(tile_at(16.1, 47.9, 32.0), (1, 1));
        assert_eq!(tile_at(-16.1, 0.0, 32.0), (-1, 0));
        assert_eq!(tile_at(96.0, 64.0, 32.0), (3, 2));
    }

    #[test]
    fn a_drag_sends_one_request_per_tile() {
        let t = tags();
        let mut last = None;
        // Off: nothing, and nothing remembered.
        assert_eq!(paint_action(Brush::Off, false, (1, 1), &t, &mut last), None);
        assert_eq!(last, None);
        // Paint tag 1 ("road").
        let a = paint_action(Brush::Paint(1), false, (4, 2), &t, &mut last);
        assert_eq!(
            a,
            Some(Action::PaintNavTile {
                x: 4,
                y: 2,
                op: PaintOp::Add("road".into())
            })
        );
        // Same tile again while the button is held: nothing.
        assert_eq!(
            paint_action(Brush::Paint(1), false, (4, 2), &t, &mut last),
            None
        );
        // Next tile: another request.
        assert!(paint_action(Brush::Paint(1), false, (5, 2), &t, &mut last).is_some());
        // Right button clears, whatever the brush.
        last = None;
        assert_eq!(
            paint_action(Brush::Paint(0), true, (1, 1), &t, &mut last),
            Some(Action::PaintNavTile {
                x: 1,
                y: 1,
                op: PaintOp::Clear
            })
        );
        // The erase brush clears too.
        last = None;
        assert_eq!(
            paint_action(Brush::Erase, false, (2, 2), &t, &mut last),
            Some(Action::PaintNavTile {
                x: 2,
                y: 2,
                op: PaintOp::Clear
            })
        );
        // A brush pointing at a tag that is not in the list does nothing.
        last = None;
        assert_eq!(
            paint_action(Brush::Paint(9), false, (2, 2), &t, &mut last),
            None
        );
        assert_eq!(last, None, "and is not remembered as painted");
    }

    #[test]
    fn speeds_convert_the_way_the_engine_does() {
        assert_eq!(speed_from_tiles(5.0), ianus::speed_from_tiles_per_second(5));
        assert_eq!(speed_from_tiles(5.0), 341, "the player's own speed");
        for whole in 0..=20u32 {
            assert_eq!(
                speed_from_tiles(whole as f32),
                ianus::speed_from_tiles_per_second(whole),
                "{whole}"
            );
        }
        assert_eq!(speed_from_tiles(-3.0), 0);
        assert!((tiles_per_second(341) - 4.995).abs() < 0.01);
        assert_eq!(tiles_to_subunits(1.0), T);
        assert_eq!(tiles_to_subunits(0.5), T / 2);
        assert_eq!(tiles_to_subunits(-1.0), 0);
    }

    #[test]
    fn editing_one_tag_leaves_the_others_alone() {
        let a = NavAgent::default();
        let a = with_allowed(&a, 1, true);
        let a = with_avoided(&a, 0, true);
        let a = with_cost(&a, 2, 120);
        assert_eq!(
            a.filter,
            NavFilter {
                allow: 0b010,
                deny: 0b001
            }
        );
        assert_eq!(a.costs.get(2), 120);
        assert_eq!(a.costs.get(0), 0);
        // Turning them off again restores the original.
        let b = with_cost(&with_avoided(&with_allowed(&a, 1, false), 0, false), 2, 0);
        assert_eq!(b, NavAgent::default());
        // Out-of-range tags are ignored, not a panic.
        assert_eq!(with_allowed(&b, 40, true), b);
        assert_eq!(with_avoided(&b, 99, true), b);
    }

    #[test]
    fn colours_repeat_after_the_palette_and_keep_their_alpha() {
        assert_eq!(tag_colour(0, 255), tag_colour(8, 255));
        assert_ne!(tag_colour(0, 255), tag_colour(1, 255));
        assert_eq!(tag_colour(2, 90).a(), 90);
    }

    #[test]
    fn the_agent_editor_draws_and_reports_no_change_when_nothing_is_touched() {
        let path = NavPath::default();
        for agent in [
            NavAgent::default(),
            NavAgent {
                target: NavTarget::Named("Crate".into()),
                range: SearchRange::Strict,
                ..NavAgent::default()
            },
            NavAgent {
                target: NavTarget::Tile(3, 4),
                range: SearchRange::Whole,
                filter: NavFilter {
                    allow: 0b10,
                    deny: 0b01,
                },
                costs: ianus::prelude::NavCosts::NONE.with(2, 200),
                ..NavAgent::default()
            },
        ] {
            for editable in [true, false] {
                let (changed, shapes) =
                    frame(|_, ui| agent_editor(ui, &agent, &path, (2, 2), &tags(), editable));
                assert_eq!(changed, None, "showing it must not change it ({agent:?})");
                assert!(shapes > 0, "something should have been drawn");
            }
        }
        // A room with no tags, and one with the maximum.
        let (changed, _) =
            frame(|_, ui| agent_editor(ui, &NavAgent::default(), &path, (0, 0), &[], true));
        assert_eq!(changed, None);
        let many: Vec<String> = (0..40).map(|i| format!("t{i}")).collect();
        let (changed, _) =
            frame(|_, ui| agent_editor(ui, &NavAgent::default(), &path, (0, 0), &many, true));
        assert_eq!(changed, None);
    }

    #[test]
    fn the_tool_panel_draws_and_switches_the_brush_off_when_not_editing() {
        let mut tools = NavTools {
            brush: Brush::Paint(1),
            ..NavTools::default()
        };
        let before = tools.clone();
        let (_, shapes) =
            frame(|_, ui| nav_tools_panel(ui, &mut tools, &tags(), true, Some((3, 20, 12))));
        assert!(shapes > 0);
        assert_eq!(
            tools, before,
            "looking at the panel must not change anything"
        );
        // While playing, painting is off, so a brush left on is switched off.
        frame(|_, ui| nav_tools_panel(ui, &mut tools, &tags(), false, None));
        assert_eq!(tools.brush, Brush::Off);
    }

    /// A map position in subunits to a screen point: 32 pixels a tile.
    fn to_screen(x: i32, y: i32) -> Option<egui::Pos2> {
        Some(egui::pos2(
            x as f32 / T as f32 * 32.0,
            y as f32 / T as f32 * 32.0,
        ))
    }

    #[test]
    fn overlays_draw_tags_routes_crosses_and_the_window() {
        let mut nav_grid = ianus_sim::NavGrid::new(10, 10);
        nav_grid.add_tags(3, 3, 0b01);
        nav_grid.add_tags(4, 3, 0b11);
        let nav = NavMap {
            grid: nav_grid,
            version: 1,
        };
        let path = NavPath::default();
        let draw = |tools: &NavTools, routes: &[RouteView]| {
            frame(|ctx, _| {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("overlay_test"),
                ));
                draw_nav_overlays(&painter, &to_screen, Some(&nav), tools, routes);
            })
            .1
        };
        let mut tools = NavTools::default();
        let routes = [RouteView {
            pos: Vec2i::new(T, T),
            path: &path,
            range: SearchRange::Growing,
            selected: true,
        }];
        // Tagged tiles only: three stripes (one tag on one tile, two on another).
        tools.show_routes = false;
        let tags_only = draw(&tools, &routes);
        assert!(tags_only >= 3, "{tags_only}");
        // Nothing shown at all: nothing drawn.
        tools.show_tags = false;
        assert_eq!(draw(&tools, &routes), 0);
        // Routes on, tags off: an idle agent with no route draws nothing yet.
        tools.show_routes = true;
        assert_eq!(draw(&tools, &routes), 0);
        // The brush being on shows tags even if the checkbox is off.
        tools.brush = Brush::Paint(0);
        assert!(draw(&tools, &[]) >= 3);
    }
}
