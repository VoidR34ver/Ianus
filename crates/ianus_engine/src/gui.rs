//! The editor-style GUI: toolbar, scene list, inspector, simulation panel.
//!
//! Layout (the game renders in the middle, in whatever space is left):
//!
//! ```text
//! +--------------------------------------------------+
//! | toolbar: pause  step  speed  record  replay      |
//! +--------+--------------------------------+--------+
//! | Scene  |                                |Inspect.|
//! | list   |           game view            |  or    |
//! |        |                                | tuning |
//! +--------+--------------------------------+--------+
//! | Simulation: timeline, frame times, messages      |
//! +--------------------------------------------------+
//! ```

use std::collections::VecDeque;

use bevy::camera::Viewport;
use bevy::diagnostic::DiagnosticsStore;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use egui::{Align, Color32, Layout, Stroke, StrokeKind};
use ianus::prelude::*;
use ianus_sim::tile_of;

use crate::code_editor::CodeEditor;
use crate::navtools::{self, Brush, NavTools, RouteView};
use crate::{
    Action, Actions, EngineCamera, EngineConfig, EngineStatus, GuiVisible, SPEEDS, Selection,
};

/// How many recent frame times the graph keeps.
const HISTORY: usize = 240;

/// Recent frame times in milliseconds, oldest first.
#[derive(Resource, Default)]
pub struct FrameHistory(VecDeque<f32>);

pub(crate) fn record_frame_time(time: Res<Time<Real>>, mut history: ResMut<FrameHistory>) {
    if history.0.len() >= HISTORY {
        history.0.pop_front();
    }
    history.0.push_back(time.delta_secs() * 1000.0);
}

/// Read-only state the panels show. Grouped so the system stays under Bevy's
/// parameter limit.
#[derive(SystemParam)]
pub(crate) struct GuiData<'w> {
    tick: Res<'w, Tick>,
    control: Res<'w, SimControl>,
    mode: Res<'w, ReplayMode>,
    status: Res<'w, EngineStatus>,
    history: Res<'w, FrameHistory>,
    config: Res<'w, EngineConfig>,
    scale: Res<'w, WorldScale>,
    visible: Res<'w, GuiVisible>,
    _diagnostics: Res<'w, DiagnosticsStore>,
    play_mode: Res<'w, PlayMode>,
    room: Option<Res<'w, CurrentRoom>>,
    nav: Option<Res<'w, NavMap>>,
    room_file: Option<Res<'w, crate::rooms::RoomFile>>,
}

type BodyQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static Name>,
        &'static mut SimBody,
        &'static mut PrevPos,
        Has<Controlled>,
        &'static Transform,
    ),
    Without<EngineCamera>,
>;

// A Bevy system: its parameters are its inputs, so the count is not a design smell.
#[allow(clippy::too_many_arguments)]
pub(crate) fn gui_system(
    mut contexts: EguiContexts,
    window: Single<&Window, With<PrimaryWindow>>,
    mut game_camera: Single<(&mut Camera, &Transform), With<EngineCamera>>,
    data: GuiData,
    mut selection: ResMut<Selection>,
    mut actions: ResMut<Actions>,
    mut bodies: BodyQuery,
    mut placed: Query<&mut PlacedObject>,
    mut nav_tools: ResMut<NavTools>,
    mut agents: Query<(Entity, &mut NavAgent, &NavPath)>,
    object_tags: Query<&NavTags>,
    mut editor: ResMut<CodeEditor>,
    mut tab: ResMut<EditorTab>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let scale_factor = window.scale_factor();

    if !data.visible.0 {
        if game_camera.0.viewport.is_some() {
            game_camera.0.viewport = None;
        }
        return Ok(());
    }

    let mut root = egui::Ui::new(
        ctx.clone(),
        "ianus_root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    let top = egui::Panel::top("toolbar")
        .show(&mut root, |ui| {
            toolbar(ui, &data, &mut actions, &mut editor)
        })
        .response
        .rect
        .height();

    let tabs_h = egui::Panel::top("tabs")
        .show(&mut root, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut *tab, EditorTab::Main, "Main");
                ui.selectable_value(&mut *tab, EditorTab::Rooms, "Rooms");
            });
        })
        .response
        .rect
        .height();
    let top = top + tabs_h;

    let bottom = egui::Panel::bottom("simulation")
        .resizable(true)
        .default_size(150.0)
        .show(&mut root, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("simulation_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| simulation_panel(ui, &data))
        })
        .response
        .rect
        .height();

    let left = egui::Panel::left("scene")
        .resizable(true)
        .default_size(210.0)
        .show(&mut root, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut editor.open, false, "Scene");
                ui.selectable_value(&mut editor.open, true, "Assets");
            });
            ui.separator();
            if editor.open {
                editor.resources(ui, &mut actions);
            } else {
                scene_panel(
                    ui,
                    &data,
                    &mut selection,
                    &bodies,
                    &placed.as_readonly(),
                    &mut actions,
                )
            }
        })
        .response
        .rect
        .width();

    let right = egui::Panel::right("inspector")
        .resizable(true)
        .default_size(260.0)
        .show(&mut root, |ui| {
            // The inspector is taller than a small window, so it scrolls.
            egui::ScrollArea::vertical()
                .id_salt("inspector_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    inspector_panel(
                        ui,
                        &data,
                        &mut selection,
                        &mut bodies,
                        &mut placed,
                        &mut agents,
                        &object_tags,
                        &mut nav_tools,
                    )
                })
        })
        .response
        .rect
        .width();

    editor.show(&ctx);

    if *tab == EditorTab::Rooms {
        // Covers the game view; the camera keeps its viewport underneath.
        egui::CentralPanel::default().show(&mut root, |ui| rooms_tab(ui, &data, &mut actions));
        return Ok(());
    }

    // The game gets the space the panels left over.
    let size = UVec2::new(window.physical_width(), window.physical_height());
    let pos = UVec2::new((left * scale_factor) as u32, ((top) * scale_factor) as u32);
    let taken = UVec2::new(
        (right * scale_factor) as u32,
        (bottom * scale_factor) as u32,
    );
    let view_size = size.saturating_sub(pos).saturating_sub(taken);

    if view_size.x == 0 || view_size.y == 0 {
        game_camera.0.viewport = None;
        return Ok(());
    }
    let wanted = Viewport {
        physical_position: pos,
        physical_size: view_size,
        ..default()
    };
    let unchanged = game_camera.0.viewport.as_ref().is_some_and(|v| {
        v.physical_position == wanted.physical_position && v.physical_size == wanted.physical_size
    });
    if !unchanged {
        game_camera.0.viewport = Some(wanted);
    }

    // Clicks and the selection marker live in the game view.
    let view_min = egui::pos2(pos.x as f32 / scale_factor, pos.y as f32 / scale_factor);
    let view_rect = egui::Rect::from_min_size(
        view_min,
        egui::vec2(
            view_size.x as f32 / scale_factor,
            view_size.y as f32 / scale_factor,
        ),
    );
    let cam_tf = GlobalTransform::from(*game_camera.1);
    game_view(
        &ctx,
        view_rect,
        &game_camera.0,
        &cam_tf,
        &data,
        &mut selection,
        &bodies,
        &agents.as_readonly(),
        &mut nav_tools,
        &mut actions,
    );

    Ok(())
}

/// Which main screen is showing.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum EditorTab {
    /// Node tree, game view, inspector.
    #[default]
    Main,
    /// All room files; open one to edit it on Main.
    Rooms,
}

fn rooms_tab(ui: &mut egui::Ui, data: &GuiData, actions: &mut Actions) {
    ui.heading("Rooms");
    ui.weak(format!("Files in {}/", crate::rooms::ROOMS_DIR));
    ui.separator();
    let rooms = crate::rooms::list_rooms();
    if rooms.is_empty() {
        ui.weak("No .room files found.");
    }
    for path in rooms {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let current = data.room_file.as_ref().is_some_and(|f| f.path() == path);
        ui.horizontal(|ui| {
            ui.label(&name);
            if current {
                ui.weak("(open)");
            } else if ui.small_button("Open").clicked() {
                actions.push(Action::OpenRoom(path));
            }
        });
    }
}

fn toolbar(ui: &mut egui::Ui, data: &GuiData, actions: &mut Actions, editor: &mut CodeEditor) {
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        let paused = data.control.paused;
        let editing = *data.play_mode == PlayMode::Editing;

        // Play and Stop are one button, like every editor: Play runs the game,
        // Stop puts everything back as it was when you pressed Play.
        let (label, tip, action, color) = if editing {
            (
                "Play  (F5)",
                "Run the game. Stop will put everything back.",
                Action::Play,
                Color32::from_rgb(70, 160, 90),
            )
        } else {
            (
                "Stop  (F5)",
                "Stop and put everything back as it was when you pressed Play",
                Action::Stop,
                Color32::from_rgb(190, 80, 80),
            )
        };
        if ui
            .add(egui::Button::new(label).fill(color))
            .on_hover_text(tip)
            .clicked()
        {
            actions.push(action);
        }

        ui.separator();
        if ui
            .add_enabled(
                !editing,
                egui::Button::new(if paused {
                    "Resume  (F1)"
                } else {
                    "Pause  (F1)"
                }),
            )
            .clicked()
        {
            actions.push(Action::TogglePause);
        }
        if ui
            .button("Step  (F2)")
            .on_hover_text("Run exactly one tick")
            .clicked()
        {
            actions.push(Action::Step);
        }

        ui.separator();
        ui.label("Speed");
        for s in SPEEDS {
            let selected = (data.control.speed - s).abs() < 1e-3;
            if ui.selectable_label(selected, format!("{s}x")).clicked() {
                actions.push(Action::SetSpeed(s));
            }
        }

        ui.separator();
        let recording = matches!(*data.mode, ReplayMode::Recording(_));
        let playing = matches!(*data.mode, ReplayMode::Playing(_));
        let record_label = if recording {
            "Stop & save  (F7)"
        } else {
            "Record  (F7)"
        };
        if ui
            .add_enabled(!playing, egui::Button::new(record_label))
            .on_hover_text("Records every tick's input so the session can be replayed exactly")
            .clicked()
        {
            actions.push(Action::ToggleRecording);
        }
        if ui
            .add_enabled(!recording, egui::Button::new("Replay  (F6)"))
            .on_hover_text("Restart the world and play the saved recording back")
            .clicked()
        {
            actions.push(Action::PlaySavedReplay);
        }
        if ui
            .selectable_label(editor.open, "Resources")
            .on_hover_text("Open the code editor window (scripts/ folder)")
            .clicked()
        {
            editor.toggle();
        }
        if ui
            .add_enabled(!recording && !playing, egui::Button::new("Restart").small())
            .on_hover_text("Put everything back at its starting place, tick 0")
            .clicked()
        {
            actions.push(Action::Restart);
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let fps = average_fps(&data.history.0);
            ui.label(format!("tick {}   {:.0} fps", data.tick.0, fps));
            if *data.play_mode == PlayMode::Editing {
                ui.colored_label(Color32::from_rgb(120, 170, 255), "EDITING");
            } else if paused {
                ui.colored_label(Color32::from_rgb(255, 190, 80), "PAUSED");
            }
            match &*data.mode {
                ReplayMode::Recording(r) => {
                    ui.colored_label(Color32::from_rgb(255, 90, 90), format!("REC {}", r.len()));
                }
                ReplayMode::Playing(_) => {
                    ui.colored_label(Color32::from_rgb(120, 200, 255), "PLAYING");
                }
                _ => {}
            }
        });
    });
}

fn scene_panel(
    ui: &mut egui::Ui,
    data: &GuiData,
    selection: &mut Selection,
    bodies: &BodyQuery,
    placed: &Query<&PlacedObject>,
    actions: &mut Actions,
) {
    ui.heading("Scene");
    if let Some(room) = &data.room {
        ui.horizontal(|ui| {
            ui.strong(&room.0.name);
            ui.weak(format!("{} x {} tiles", room.0.width, room.0.height));
        });
        if ui
            .small_button("Reload room  (F8)")
            .on_hover_text("Read the room file again. Rooms also reload by themselves when saved.")
            .clicked()
        {
            actions.push(Action::ReloadRoom);
        }
    }
    ui.separator();
    let items: Vec<(Entity, Option<Category>, String)> = bodies
        .iter()
        .map(|(entity, name, _, _, controlled, _)| {
            let label = match (name, controlled) {
                (Some(n), true) => format!("{n}  (player)"),
                (Some(n), false) => n.to_string(),
                (None, true) => "Player".to_string(),
                (None, false) => format!("Body {}", entity.index()),
            };
            let category = placed.get(entity).ok().map(|p| p.category);
            (entity, category, label)
        })
        .collect();

    if items.is_empty() {
        ui.weak("No simulated bodies yet.");
        ui.weak("Spawn entities with a SimBody component.");
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (group, rows) in group_by_category(items) {
            // Walls and floor tiles are many and rarely interesting, so they
            // start folded; everything else starts open.
            let open = !matches!(group, "Wall" | "Environment");
            egui::CollapsingHeader::new(format!("{group} ({})", rows.len()))
                .id_salt(group)
                .default_open(open)
                .show(ui, |ui| {
                    for (entity, label) in rows {
                        let selected = selection.0 == Some(entity);
                        if ui.selectable_label(selected, label).clicked() {
                            selection.0 = if selected { None } else { Some(entity) };
                        }
                    }
                });
        }
    });
}

/// Sort scene rows into one group per category, in the order of
/// [`Category::ALL`], with anything not from a room under "Other". Empty
/// groups are dropped, and rows keep a stable order so the list does not
/// jump around between frames.
fn group_by_category(
    items: Vec<(Entity, Option<Category>, String)>,
) -> Vec<(&'static str, Vec<(Entity, String)>)> {
    let mut groups: Vec<(&'static str, Vec<(Entity, String)>)> = Category::ALL
        .iter()
        .map(|c| (c.label(), Vec::new()))
        .collect();
    groups.push(("Other", Vec::new()));
    for (entity, category, label) in items {
        let index = category
            .and_then(|c| Category::ALL.iter().position(|&x| x == c))
            .unwrap_or(Category::ALL.len());
        groups[index].1.push((entity, label));
    }
    for (_, rows) in &mut groups {
        // By entity index, which follows spawn order, i.e. the order objects
        // are written in the room file. (`to_bits` is stable but not in order.)
        rows.sort_by_key(|(e, _)| e.index());
    }
    groups.retain(|(_, rows)| !rows.is_empty());
    groups
}

// The inspector shows everything about the selection, so it takes every query.
#[allow(clippy::too_many_arguments)]
fn inspector_panel(
    ui: &mut egui::Ui,
    data: &GuiData,
    selection: &mut Selection,
    bodies: &mut BodyQuery,
    placed: &mut Query<&mut PlacedObject>,
    agents: &mut Query<(Entity, &mut NavAgent, &NavPath)>,
    object_tags: &Query<&NavTags>,
    nav_tools: &mut NavTools,
) {
    ui.heading("Inspector");
    ui.separator();

    let sub = f64::from(SUBUNITS_PER_TILE);
    let locked = !matches!(*data.mode, ReplayMode::Off | ReplayMode::Finished { .. });

    match selection.0.map(|e| (e, bodies.get_mut(e))) {
        Some((entity, Ok((_, name, mut body, mut prev, controlled, _)))) => {
            ui.strong(match name {
                Some(n) => n.to_string(),
                None => format!("Entity {}", entity.index()),
            });
            if controlled {
                ui.label("Controlled by the keyboard");
            }
            if let Ok(object) = placed.get(entity) {
                ui.label(format!("{}: {}", object.category.label(), object.name));
                ui.weak(object.category.blurb());
                if let Some(def) = data
                    .room
                    .as_ref()
                    .and_then(|r| r.0.find_object(&object.name))
                {
                    egui::CollapsingHeader::new("Object options (read-only for now)")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.monospace(format!("{:#?}", def.kind));
                        });
                }
            }
            if *data.play_mode == PlayMode::Playing {
                ui.weak("Changes made while playing are undone by Stop.");
            }

            // Pathfinding: the nav tags this object puts on its tile, and, if
            // it is an agent, its settings and route.
            let tag_names = data
                .room
                .as_ref()
                .map(|r| r.0.nav_tags.clone())
                .unwrap_or_default();
            if let Ok(tags) = object_tags.get(entity) {
                let shown: Vec<&str> = tag_names
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| i < 32 && tags.0 >> i & 1 == 1)
                    .map(|(_, n)| n.as_str())
                    .collect();
                ui.label(format!("Nav tags on its tile: {}", shown.join(", ")));
            }
            if let Ok((_, mut agent, path)) = agents.get_mut(entity) {
                ui.add_space(6.0);
                egui::CollapsingHeader::new("Pathfinding")
                    .default_open(true)
                    .show(ui, |ui| {
                        let tile = (tile_of(body.0.pos.x), tile_of(body.0.pos.y));
                        if let Some(new) =
                            navtools::agent_editor(ui, &agent, path, tile, &tag_names, !locked)
                        {
                            *agent = new;
                        }
                        ui.weak("Changes here are not saved to the room file.");
                    });
            }
            ui.add_space(6.0);

            if locked {
                ui.colored_label(
                    Color32::from_rgb(255, 190, 80),
                    "Editing is off while recording or replaying: it would make the replay wrong.",
                );
            }
            ui.add_enabled_ui(!locked, |ui| {
                ui.label("Position (tiles)");
                let mut x = f64::from(body.0.pos.x) / sub;
                let mut y = f64::from(body.0.pos.y) / sub;
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("x");
                    changed |= ui.add(egui::DragValue::new(&mut x).speed(0.05)).changed();
                    ui.label("y");
                    changed |= ui.add(egui::DragValue::new(&mut y).speed(0.05)).changed();
                });
                if changed {
                    // Snap to the simulation's own grid, and drop the old
                    // "previous" position so the body does not glide there.
                    body.0.pos.x = (x * sub).round() as i32;
                    body.0.pos.y = (y * sub).round() as i32;
                    body.0.vel = Vec2i::ZERO;
                    prev.0 = body.0.pos;
                    // In edit mode this is moving the object's starting point,
                    // so Restart and recording must not put it back. While
                    // playing it is a temporary nudge that Stop undoes.
                    if *data.play_mode == PlayMode::Editing
                        && let Ok(mut object) = placed.get_mut(entity)
                    {
                        object.home = body.0.pos;
                    }
                }
                if ui
                    .button("Stop")
                    .on_hover_text("Set velocity to zero")
                    .clicked()
                {
                    body.0.vel = Vec2i::ZERO;
                }
            });

            ui.add_space(6.0);
            ui.label("Velocity");
            let per_second = f64::from(TICK_HZ) / sub;
            ui.monospace(format!(
                "{:+.2}, {:+.2} tiles/s",
                f64::from(body.0.vel.x) * per_second,
                f64::from(body.0.vel.y) * per_second
            ));
            ui.label("Raw (subunits)");
            ui.monospace(format!(
                "pos {}, {}\nvel {}, {}",
                body.0.pos.x, body.0.pos.y, body.0.vel.x, body.0.vel.y
            ));
            ui.label("State hash");
            ui.monospace(format!("{:016x}", body.0.state_hash()));
        }
        Some((_, Err(_))) => {
            // The entity was despawned.
            selection.0 = None;
        }
        None => {
            ui.weak("Nothing selected.");
            ui.weak("Pick something in the Scene list, or click it in the game view.");
        }
    }

    ui.add_space(10.0);
    ui.separator();
    ui.collapsing("Movement tuning (read-only)", |ui| {
        egui::Grid::new("tuning").num_columns(2).show(ui, |ui| {
            for (name, value) in [
                ("Tick rate", format!("{TICK_HZ} Hz")),
                (
                    "Top speed",
                    format!("{} subunits/tick", ianus_sim::MAX_SPEED),
                ),
                ("Acceleration", format!("{} /tick", ianus_sim::ACCEL)),
                ("Friction", format!("{} /tick", ianus_sim::FRICTION)),
                ("Tile", format!("{SUBUNITS_PER_TILE} subunits")),
            ] {
                ui.label(name);
                ui.monospace(value);
                ui.end_row();
            }
        });
    });
    ui.collapsing("View", |ui| {
        ui.label(format!("{} tiles tall", data.config.view_height_tiles));
        ui.label(format!("{} world units per tile", data.scale.0));
    });
    ui.add_space(6.0);
    egui::CollapsingHeader::new("Pathfinding tools")
        .default_open(true)
        .show(ui, |ui| {
            let tags = data
                .room
                .as_ref()
                .map(|r| r.0.nav_tags.clone())
                .unwrap_or_default();
            let map = data
                .nav
                .as_ref()
                .map(|n| (n.version, n.grid.width(), n.grid.height()));
            navtools::nav_tools_panel(
                ui,
                nav_tools,
                &tags,
                *data.play_mode == PlayMode::Editing,
                map,
            );
        });
}

fn simulation_panel(ui: &mut egui::Ui, data: &GuiData) {
    ui.heading("Simulation");
    ui.separator();
    ui.columns(2, |cols| {
        // Left: timeline and messages.
        let ui = &mut cols[0];
        match &*data.mode {
            ReplayMode::Recording(r) => {
                ui.label(format!(
                    "Recording: {} ticks ({:.1} s)",
                    r.len(),
                    r.len() as f32 / TICK_HZ as f32
                ));
            }
            ReplayMode::Playing(r) => {
                let done = data.tick.0 as f32 / r.len().max(1) as f32;
                ui.add(egui::ProgressBar::new(done.clamp(0.0, 1.0)).text(format!(
                    "Replay {} / {}",
                    data.tick.0.min(r.len() as u64),
                    r.len()
                )));
            }
            _ => {
                ui.label(format!(
                    "Tick {}  ({:.1} s of game time)",
                    data.tick.0,
                    data.tick.0 as f32 / TICK_HZ as f32
                ));
            }
        }
        ui.add_space(4.0);
        ui.label("Messages");
        egui::ScrollArea::vertical()
            .id_salt("messages")
            .max_height(90.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let mut any = false;
                for line in data.status.lines() {
                    ui.monospace(line);
                    any = true;
                }
                if !any {
                    ui.weak("(nothing yet)");
                }
            });

        // Right: frame-time graph.
        let ui = &mut cols[1];
        ui.label(format!(
            "Frame time (last {} frames)  {:.1} ms avg",
            data.history.0.len(),
            average_ms(&data.history.0)
        ));
        frame_graph(ui, &data.history.0);
    });
}

fn average_ms(samples: &VecDeque<f32>) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().sum::<f32>() / samples.len() as f32
}

fn average_fps(samples: &VecDeque<f32>) -> f32 {
    let ms = average_ms(samples);
    if ms > 0.0 { 1000.0 / ms } else { 0.0 }
}

/// A small line graph of frame times with guides at 60 and 30 fps.
fn frame_graph(ui: &mut egui::Ui, samples: &VecDeque<f32>) {
    const TOP_MS: f32 = 40.0;
    let height = ui.available_height().clamp(50.0, 120.0);
    let (response, painter) = ui.allocate_painter(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    let rect = response.rect;
    painter.rect_filled(rect, 2.0, Color32::from_gray(22));

    let y_of = |ms: f32| rect.bottom() - (ms.min(TOP_MS) / TOP_MS) * rect.height();
    for (ms, label) in [(16.7, "60"), (33.3, "30")] {
        let y = y_of(ms);
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, Color32::from_gray(60)),
        );
        painter.text(
            egui::pos2(rect.left() + 3.0, y - 1.0),
            egui::Align2::LEFT_BOTTOM,
            label,
            egui::FontId::monospace(10.0),
            Color32::from_gray(110),
        );
    }

    if samples.len() >= 2 {
        let step = rect.width() / (HISTORY - 1) as f32;
        let offset = (HISTORY - samples.len()) as f32 * step;
        let points: Vec<egui::Pos2> = samples
            .iter()
            .enumerate()
            .map(|(i, &ms)| egui::pos2(rect.left() + offset + i as f32 * step, y_of(ms)))
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(1.5, Color32::from_rgb(110, 220, 150)),
        ));
    }
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, Color32::from_gray(70)),
        StrokeKind::Inside,
    );
}

/// Clicking in the game view selects the nearest body (or, with a paint brush
/// chosen, paints tiles); the selected one gets an outline, and pathfinding
/// overlays are drawn on top.
// The game view needs the camera, the selection, the bodies, the agents, the
// tools and the action queue.
#[allow(clippy::too_many_arguments)]
fn game_view(
    ctx: &egui::Context,
    view: egui::Rect,
    camera: &Camera,
    camera_tf: &GlobalTransform,
    data: &GuiData,
    selection: &mut Selection,
    bodies: &BodyQuery,
    agents: &Query<(Entity, &NavAgent, &NavPath)>,
    nav_tools: &mut NavTools,
    actions: &mut Actions,
) {
    let tile = data.scale.0;
    let screen_per_world = view.height() / (data.config.view_height_tiles * tile);

    // Select on click, but only when the click landed in the game view and
    // not on a panel or window floating over it.
    let click = ctx.input(|i| {
        i.pointer
            .primary_clicked()
            .then(|| i.pointer.interact_pos())
            .flatten()
    });
    // A paint brush takes over the mouse (while editing): dragging paints one
    // request per tile, right button clears.
    let painting = nav_tools.brush != Brush::Off && *data.play_mode == PlayMode::Editing;
    if painting {
        let (primary, secondary, at) = ctx.input(|i| {
            (
                i.pointer.primary_down(),
                i.pointer.secondary_down(),
                i.pointer.interact_pos(),
            )
        });
        match at {
            Some(p)
                if (primary || secondary) && view.contains(p) && !ctx.is_pointer_over_egui() =>
            {
                if let Ok(world) = camera
                    .viewport_to_world_2d(camera_tf, Vec2::new(p.x - view.min.x, p.y - view.min.y))
                {
                    let tags = data
                        .room
                        .as_ref()
                        .map(|r| r.0.nav_tags.clone())
                        .unwrap_or_default();
                    let tile = navtools::tile_at(world.x, world.y, data.scale.0);
                    if let Some(action) = navtools::paint_action(
                        nav_tools.brush,
                        secondary,
                        tile,
                        &tags,
                        &mut nav_tools.last_tile,
                    ) {
                        actions.push(action);
                    }
                }
            }
            _ if !(primary || secondary) => nav_tools.last_tile = None,
            _ => {}
        }
    }

    if let Some(p) = click
        && !painting
        && view.contains(p)
        && !ctx.is_pointer_over_egui()
        && let Ok(world) =
            camera.viewport_to_world_2d(camera_tf, Vec2::new(p.x - view.min.x, p.y - view.min.y))
    {
        let reach = tile * 0.75;
        selection.0 = bodies
            .iter()
            .map(|(e, _, _, _, _, t)| (e, t.translation.truncate().distance(world)))
            .filter(|&(_, d)| d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(e, _)| e);
    }

    // Pathfinding overlays: tagged tiles, routes, the search window.
    let to_world = data.scale.0 / SUBUNITS_PER_TILE as f32;
    let to_screen = |x: i32, y: i32| -> Option<egui::Pos2> {
        camera
            .world_to_viewport(
                camera_tf,
                Vec3::new(x as f32 * to_world, y as f32 * to_world, 0.0),
            )
            .ok()
            .map(|v| egui::pos2(view.min.x + v.x, view.min.y + v.y))
    };
    let routes: Vec<RouteView> = agents
        .iter()
        .filter_map(|(entity, agent, path)| {
            let (_, _, body, ..) = bodies.get(entity).ok()?;
            Some(RouteView {
                pos: body.0.pos,
                path,
                range: agent.range,
                selected: selection.0 == Some(entity),
            })
        })
        .collect();
    let overlay = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("nav_overlay"),
        ))
        .with_clip_rect(view);
    navtools::draw_nav_overlays(
        &overlay,
        &to_screen,
        data.nav.as_deref(),
        nav_tools,
        &routes,
    );

    let Some(selected) = selection.0 else { return };
    let Ok((_, _, _, _, _, t)) = bodies.get(selected) else {
        return;
    };
    let Ok(on_screen) = camera.world_to_viewport(camera_tf, t.translation) else {
        return;
    };
    let center = egui::pos2(view.min.x + on_screen.x, view.min.y + on_screen.y);
    let half = tile * 0.5 * screen_per_world + 3.0;
    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("selection_marker"),
        ))
        .with_clip_rect(view);
    painter.rect_stroke(
        egui::Rect::from_center_size(center, egui::vec2(half * 2.0, half * 2.0)),
        2.0,
        Stroke::new(2.0, Color32::from_rgb(255, 210, 90)),
        StrokeKind::Middle,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_rows_group_by_category_in_a_fixed_order() {
        let mut world = World::new();
        let mut e = || world.spawn_empty().id();
        let (a, b, c, d) = (e(), e(), e(), e());
        let groups = group_by_category(vec![
            (a, Some(Category::Light), "torch".into()),
            (b, Some(Category::Wall), "wall".into()),
            (c, None, "loose".into()),
            (d, Some(Category::Wall), "wall2".into()),
        ]);
        let names: Vec<_> = groups.iter().map(|(g, _)| *g).collect();
        assert_eq!(
            names,
            ["Wall", "Light", "Other"],
            "category order, empties dropped"
        );
        assert_eq!(groups[0].1.len(), 2);
        // Stable within a group: by entity, not by label.
        assert_eq!(groups[0].1[0].0, b);
        assert_eq!(groups[0].1[1].0, d);
    }

    #[test]
    fn averages_handle_empty_history() {
        let empty = VecDeque::new();
        assert_eq!(average_ms(&empty), 0.0);
        assert_eq!(average_fps(&empty), 0.0);
    }

    #[test]
    fn fps_is_the_inverse_of_frame_time() {
        let samples: VecDeque<f32> = std::iter::repeat_n(16.0, 10).collect();
        assert!((average_fps(&samples) - 62.5).abs() < 0.01);
    }
}
