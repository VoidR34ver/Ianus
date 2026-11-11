//! Putting a [`Room`] into the world, and noticing when its file changes.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use ianus_objects::{Category, Kind, ParseError, Problem, Room, Severity};
use ianus_sim::{Body, SUBUNITS_PER_TILE, Vec2i};

use crate::body::{Controlled, SimBody};
use crate::collide::{Collider, ColliderIndex, SolidMap, Touching, WorldSettings};
use crate::input::MoveIntent;
use crate::objects::ObjectSchedule;
use crate::play::PlayMode;
use crate::tick::{ReplayMode, SimControl, SimRng, Tick, TickInput};

/// One object placed in the room. Every placed object also has a [`SimBody`]
/// at its tile, so it shows up in the scene list and the inspector.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct PlacedObject {
    /// The object definition's name.
    pub name: String,
    pub category: Category,
    /// Where it starts, in subunits. Restart puts it back here.
    pub home: Vec2i,
}

/// The room currently loaded.
#[derive(Resource, Debug, Clone)]
pub struct CurrentRoom(pub Room);

/// Why a room could not be loaded. The world is left untouched.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// The file could not be read.
    Io { path: PathBuf, reason: String },
    /// The file is not a valid room file.
    Parse(ParseError),
    /// The room has errors (listed with any warnings, errors first).
    Invalid(Vec<Problem>),
}

impl core::fmt::Display for LoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LoadError::Io { path, reason } => write!(f, "cannot read {}: {reason}", path.display()),
            LoadError::Parse(e) => write!(f, "{e}"),
            LoadError::Invalid(problems) => {
                let errors = problems
                    .iter()
                    .filter(|p| p.severity == Severity::Error)
                    .count();
                write!(f, "room has {errors} error(s) and was not loaded")
            }
        }
    }
}

impl core::error::Error for LoadError {}

fn tile_to_subunits(tile: i32) -> i32 {
    tile.saturating_mul(SUBUNITS_PER_TILE)
}

/// Replace whatever room is loaded with `room`, in edit mode and paused.
///
/// Refuses (changing nothing) if the room has errors. On success returns the
/// room's warnings, so the caller can show them.
pub fn load_room(world: &mut World, room: Room) -> Result<Vec<Problem>, LoadError> {
    let problems = room.validate();
    if problems.iter().any(|p| p.severity == Severity::Error) {
        return Err(LoadError::Invalid(problems));
    }

    // Out with the old room.
    let old: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<PlacedObject>>();
        q.iter(world).collect()
    };
    for entity in old {
        let _ = world.despawn(entity);
    }

    // In with the new.
    let level_collision = room.world.collision;
    let scale = room.world.ob_tick_scale;
    let mut schedule = ObjectSchedule::default();
    for p in &room.placed {
        let Some(def) = room.find_object(&p.object) else {
            continue; // validate() already rejected unknown names
        };
        let home = Vec2i::new(tile_to_subunits(p.x), tile_to_subunits(p.y));
        let mut entity = world.spawn((
            Name::new(format!("{} ({}, {})", p.object, p.x, p.y)),
            SimBody(Body::at(home)),
            PlacedObject {
                name: p.object.clone(),
                category: def.category(),
                home,
            },
        ));
        if matches!(
            def.kind,
            Kind::Entity {
                controlled: true,
                ..
            }
        ) {
            entity.insert(Controlled);
        }
        if let Some(collider) = Collider::for_def(def, level_collision) {
            entity.insert(collider);
        }
        // The level's scale stretches every object's interval; 0 stays 0.
        schedule.add(entity.id(), def.ob_tick.saturating_mul(scale));
    }

    world.insert_resource(SolidMap::from_room(&room));
    world.insert_resource(ColliderIndex::new(room.width, room.height));
    world.insert_resource(Touching::default());
    world.insert_resource(WorldSettings(room.world));
    world.insert_resource(schedule);
    world.insert_resource(CurrentRoom(room));
    world.insert_resource(Tick(0));
    world.insert_resource(SimRng::default());
    world.insert_resource(ReplayMode::Off);
    world.insert_resource(MoveIntent::default());
    world.insert_resource(TickInput::default());
    *world.resource_mut::<PlayMode>() = PlayMode::Editing;
    let mut control = world.resource_mut::<SimControl>();
    control.paused = true;
    control.step_requests = 0;

    let warnings = problems
        .into_iter()
        .filter(|p| p.severity == Severity::Warning)
        .collect();
    Ok(warnings)
}

/// Read a room file and load it. See [`load_room`].
pub fn load_room_file(world: &mut World, path: &Path) -> Result<Vec<Problem>, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|e| LoadError::Io {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    let room = Room::from_text(&text).map_err(LoadError::Parse)?;
    load_room(world, room)
}

/// What a [`FileWatch`] noticed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChange {
    Unchanged,
    /// Written (or created) since the last poll.
    Changed,
    /// Existed before, now missing.
    Gone,
}

/// Notices when a file's modification time changes. Poll it a few times a
/// second; it only looks at file metadata, so it is cheap.
#[derive(Debug, Clone)]
pub struct FileWatch {
    path: PathBuf,
    last: Option<SystemTime>,
}

impl FileWatch {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let last = modified(&path);
        Self { path, last }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn poll(&mut self) -> FileChange {
        match (modified(&self.path), self.last) {
            (Some(now), last) if Some(now) != last => {
                self.last = Some(now);
                FileChange::Changed
            }
            (Some(_), _) => FileChange::Unchanged,
            (None, Some(_)) => {
                self.last = None;
                FileChange::Gone
            }
            (None, None) => FileChange::Unchanged,
        }
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}
