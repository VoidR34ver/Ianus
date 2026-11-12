//! Objects and rooms, the way GameMaker thinks about them.
//!
//! An **object** is a named thing with a *category* that decides what options
//! it has: a wall blocks, an entity moves, an interactable reacts, and so on.
//! A **room** lists the object definitions it uses and where each one is
//! placed. Everything is plain data, so rooms can be saved, diffed in git and
//! reloaded while the game runs.
//!
//! This crate has no Bevy and no rendering: it is the data model and the
//! checks that run on it. [`Room::validate`] explains problems in plain
//! English, with "did you mean" suggestions for typos.

mod validate;

pub use validate::{MAX_OB_TICK, Problem, Severity};

use serde::{Deserialize, Serialize};

/// The six kinds of object. Each has built-in options (see [`Kind`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Wall,
    Entity,
    Interactable,
    Environment,
    Light,
    SubstanceSource,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Wall,
        Category::Entity,
        Category::Interactable,
        Category::Environment,
        Category::Light,
        Category::SubstanceSource,
    ];

    /// Name as shown in the editor's "create object" list.
    pub fn label(self) -> &'static str {
        match self {
            Category::Wall => "Wall",
            Category::Entity => "Entity",
            Category::Interactable => "Interactable",
            Category::Environment => "Environment",
            Category::Light => "Light",
            Category::SubstanceSource => "Substance source",
        }
    }

    /// One line telling a beginner what this category is for.
    pub fn blurb(self) -> &'static str {
        match self {
            Category::Wall => "Solid. Blocks movement and, later, light and substances.",
            Category::Entity => "Something that moves: the player, a creature, a vehicle.",
            Category::Interactable => {
                "Reacts when something steps on it or uses it: a door, a button."
            }
            Category::Environment => "Ground and scenery that changes how things move across it.",
            Category::Light => "Lights up the area around it.",
            Category::SubstanceSource => {
                "Keeps adding a substance (water, heat, gas, smoke) to the world."
            }
        }
    }
}

/// What a substance source puts into the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Substance {
    Water,
    Heat,
    Gas,
    Smoke,
}

/// What an interactable does when triggered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Show a message to the player.
    Message(String),
    /// Switch another object on or off, by object name.
    Toggle(String),
}

/// An object's category plus that category's built-in options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Wall {
        /// Hit points; `None` means it cannot be destroyed.
        durability: Option<u32>,
    },
    Entity {
        /// Follows the keyboard. A room should have exactly one of these.
        controlled: bool,
        /// Top speed in tiles per second.
        speed_tiles_per_s: u32,
    },
    Interactable {
        action: Action,
        /// How close (in tiles) something has to be to trigger it.
        radius_tiles: u32,
        /// Trigger only the first time.
        once: bool,
    },
    Environment {
        /// Can things walk on it at all?
        walkable: bool,
        /// Movement speed on it, as a percentage (100 = normal, 50 = mud).
        speed_percent: u32,
    },
    Light {
        radius_tiles: u32,
        /// Red, green, blue.
        color: [u8; 3],
        /// 0 to 100.
        intensity_percent: u32,
    },
    SubstanceSource {
        substance: Substance,
        /// Amount added each simulation tick.
        rate_per_tick: u32,
    },
}

impl Kind {
    pub fn category(&self) -> Category {
        match self {
            Kind::Wall { .. } => Category::Wall,
            Kind::Entity { .. } => Category::Entity,
            Kind::Interactable { .. } => Category::Interactable,
            Kind::Environment { .. } => Category::Environment,
            Kind::Light { .. } => Category::Light,
            Kind::SubstanceSource { .. } => Category::SubstanceSource,
        }
    }

    /// A sensible starting point for a new object of this category, so a
    /// beginner gets something that works before changing any option.
    pub fn default_for(category: Category) -> Kind {
        match category {
            Category::Wall => Kind::Wall { durability: None },
            Category::Entity => Kind::Entity {
                controlled: false,
                speed_tiles_per_s: 3,
            },
            Category::Interactable => Kind::Interactable {
                action: Action::Message("Hello!".into()),
                radius_tiles: 1,
                once: false,
            },
            Category::Environment => Kind::Environment {
                walkable: true,
                speed_percent: 100,
            },
            Category::Light => Kind::Light {
                radius_tiles: 5,
                color: [255, 220, 160],
                intensity_percent: 100,
            },
            Category::SubstanceSource => Kind::SubstanceSource {
                substance: Substance::Water,
                rate_per_tick: 1,
            },
        }
    }
}

/// How far an agent's route search may look from where it stands. The search
/// window is a square centred on the agent, as wide as the distance to the goal
/// (rounded up).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchMode {
    /// Search only that window. Cheap, and it gives up quickly on a goal whose
    /// way round lies outside it (such as one just across a long wall).
    Strict,
    /// Search that window; if there is no route in it, a window twice as wide,
    /// and so on until a route is found or the whole map has been searched.
    /// Never misses a reachable goal. The default.
    #[default]
    Growing,
    /// Always search the whole map.
    Whole,
}

/// Who an agent goes after.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentTarget {
    /// The entity the keyboard controls (the player).
    #[default]
    Controlled,
    /// The first placed object with this name.
    Named(String),
}

fn is_default_search(mode: &SearchMode) -> bool {
    *mode == SearchMode::default()
}

fn hundred() -> u32 {
    100
}

/// How much extra it costs an agent to step onto a tile carrying a tag. Unlike
/// `avoid`, the tag is not forbidden: the agent takes the route with the lowest
/// total cost, so it wades through shallow water to save a long detour but goes
/// round fire.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagCost {
    pub tag: String,
    /// Added to the cost of stepping onto such a tile. A plain step costs 10, so
    /// 40 means "as bad as four extra tiles of walking".
    pub cost: u8,
}

/// Makes an entity walk to its target by itself, around walls, using the
/// level's pathfinding. Movement is the same physics the player has, so an
/// agent stops at walls the same way and is as fast as its
/// `speed_tiles_per_s` allows.
///
/// Where it may walk is decided by *tags*: names the room declares in
/// [`Room::nav_tags`] and puts on tiles ([`Room::nav_paint`]) or objects
/// ([`ObjectDef::nav_tags`]). Walls always block. Plain ground has no tags.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDef {
    #[serde(default)]
    pub target: AgentTarget,
    /// If not empty, the agent only walks on tiles carrying at least one of
    /// these tags. Because plain ground carries none, a non-empty list means
    /// "only on these", such as a road-bound cart.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Tiles carrying any of these tags are never entered. Wins over `allow`.
    #[serde(default)]
    pub avoid: Vec<String>,
    /// Tags that are allowed but cost extra to step on. The agent picks the
    /// cheapest total route, so these decide between a short way through a
    /// hazard and a long way round.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub costs: Vec<TagCost>,
    /// How far a route search may look. See [`SearchMode`].
    #[serde(default, skip_serializing_if = "is_default_search")]
    pub search: SearchMode,
    /// Stops this close to the target, in hundredths of a tile.
    #[serde(default = "hundred")]
    pub stop_distance: u32,
}

impl Default for AgentDef {
    fn default() -> Self {
        Self {
            target: AgentTarget::Controlled,
            allow: Vec::new(),
            avoid: Vec::new(),
            costs: Vec::new(),
            search: SearchMode::default(),
            stop_distance: 100,
        }
    }
}

/// One tile marked with a nav tag.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavPaint {
    pub x: i32,
    pub y: i32,
    pub tag: String,
}

/// An object's hitbox, in hundredths of a tile (`100` is one tile wide).
/// The offset moves the box away from the object's position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hitbox {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub offset_x: i32,
    #[serde(default)]
    pub offset_y: i32,
}

impl Hitbox {
    /// Exactly one tile, centred.
    pub const TILE: Hitbox = Hitbox {
        width: 100,
        height: 100,
        offset_x: 0,
        offset_y: 0,
    };

    pub const fn square(size: u32) -> Self {
        Hitbox {
            width: size,
            height: size,
            offset_x: 0,
            offset_y: 0,
        }
    }

    /// Does this box cover exactly its own tile? Those are stored as one bit
    /// in the tile grid instead of as a box, which is the fastest case.
    pub const fn is_full_tile(&self) -> bool {
        self.width == 100 && self.height == 100 && self.offset_x == 0 && self.offset_y == 0
    }
}

impl Default for Hitbox {
    fn default() -> Self {
        Self::TILE
    }
}

fn yes() -> bool {
    true
}

fn one() -> u32 {
    1
}

/// A reusable object. Every object is a thing with a hitbox; the [`Kind`]
/// only adds that category's options.
///
/// In the room file the settings come first, in this order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectDef {
    pub name: String,
    /// How often this object updates: once every `ob_tick` simulation ticks.
    /// `1` is every tick, `60` is once a second, `0` is never. Objects that
    /// are not due cost nothing. Movement and collision always run every
    /// tick; this is for the object's own logic.
    #[serde(default = "one")]
    pub ob_tick: u32,
    /// Master switch. Off: the object has no hitbox as far as the game is
    /// concerned. Nothing touches it and nothing stops at it.
    #[serde(default = "yes")]
    pub collision: bool,
    /// Other things stop when they run into this. Only matters when
    /// `collision` is on.
    #[serde(default)]
    pub solid: bool,
    #[serde(default)]
    pub hitbox: Hitbox,
    /// Nav tags this object puts on the tile it stands on (they must be
    /// declared in [`Room::nav_tags`]). Agents use them to decide where they
    /// may walk. Move or delete the object and the tags go with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nav_tags: Vec<String>,
    /// Makes this entity chase something by itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentDef>,
    pub kind: Kind,
}

impl ObjectDef {
    /// A new object with that category's default options.
    pub fn new(name: impl Into<String>, category: Category) -> Self {
        Self::with_kind(name, Kind::default_for(category))
    }

    /// A new object of the given kind, with sensible settings for it:
    /// walls and non-walkable ground are solid and never update, entities are
    /// three-quarters of a tile so they fit through one-tile gaps.
    pub fn with_kind(name: impl Into<String>, kind: Kind) -> Self {
        let solid = matches!(
            kind,
            Kind::Wall { .. }
                | Kind::Environment {
                    walkable: false,
                    ..
                }
        );
        let hitbox = match kind {
            Kind::Entity { .. } => Hitbox::square(75),
            _ => Hitbox::TILE,
        };
        let ob_tick = match kind {
            Kind::Wall { .. } | Kind::Environment { .. } | Kind::Light { .. } => 0,
            _ => 1,
        };
        Self {
            name: name.into(),
            ob_tick,
            collision: true,
            solid,
            hitbox,
            nav_tags: Vec::new(),
            agent: None,
            kind,
        }
    }

    pub fn category(&self) -> Category {
        self.kind.category()
    }

    /// Does this object stop things? Needs both switches on.
    pub fn blocks(&self) -> bool {
        self.collision && self.solid
    }
}

/// Settings for the whole level, applied before any object updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRules {
    /// Level-wide master switch for collision. Off: nothing in this level
    /// touches or stops anything, whatever the objects say.
    #[serde(default = "yes")]
    pub collision: bool,
    /// Multiplies every object's `ob_tick`. `1` is normal; `2` makes every
    /// object update half as often in this level.
    #[serde(default = "one")]
    pub ob_tick_scale: u32,
}

impl Default for WorldRules {
    fn default() -> Self {
        Self {
            collision: true,
            ob_tick_scale: 1,
        }
    }
}

/// One object placed in a room, by definition name and tile position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placed {
    pub object: String,
    pub x: i32,
    pub y: i32,
}

/// A level: its size in tiles, the objects it uses, and where they sit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Level-wide settings, read before any object.
    #[serde(default)]
    pub world: WorldRules,
    /// The names of the nav tags this room uses, at most 32. Position in this
    /// list is the tag's bit, so only ever add to the end.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nav_tags: Vec<String>,
    /// Single tiles marked with a nav tag (on top of what objects add).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nav_paint: Vec<NavPaint>,
    pub objects: Vec<ObjectDef>,
    pub placed: Vec<Placed>,
}

/// Most nav tags a room can declare (they are bits of a `u32`).
pub const MAX_NAV_TAGS: usize = 32;

/// Why a room file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "room file, line {} column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl core::error::Error for ParseError {}

impl Room {
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            name: name.into(),
            width,
            height,
            world: WorldRules::default(),
            nav_tags: Vec::new(),
            nav_paint: Vec::new(),
            objects: Vec::new(),
            placed: Vec::new(),
        }
    }

    /// Declare a nav tag (if new) and return its bit. `None` if the room is
    /// already at [`MAX_NAV_TAGS`].
    pub fn declare_tag(&mut self, name: impl Into<String>) -> Option<u32> {
        let name = name.into();
        if let Some(bit) = self.tag_bit(&name) {
            return Some(bit);
        }
        if self.nav_tags.len() >= MAX_NAV_TAGS {
            return None;
        }
        self.nav_tags.push(name);
        Some(1 << (self.nav_tags.len() - 1))
    }

    /// The bit for a declared tag name.
    pub fn tag_bit(&self, name: &str) -> Option<u32> {
        self.nav_tags
            .iter()
            .position(|t| t == name)
            .filter(|&i| i < MAX_NAV_TAGS)
            .map(|i| 1u32 << i)
    }

    /// The bits for a list of tag names. Names the room does not declare are
    /// ignored (the validator reports them as errors).
    pub fn tag_mask(&self, names: &[String]) -> u32 {
        names
            .iter()
            .filter_map(|n| self.tag_bit(n))
            .fold(0, |acc, b| acc | b)
    }

    /// Mark a tile with a nav tag, declaring the tag if needed.
    pub fn paint_tag(&mut self, tag: impl Into<String>, x: i32, y: i32) {
        let tag = tag.into();
        self.declare_tag(tag.clone());
        self.nav_paint.push(NavPaint { x, y, tag });
    }

    pub fn find_object(&self, name: &str) -> Option<&ObjectDef> {
        self.objects.iter().find(|o| o.name == name)
    }

    /// Add a definition. Returns `false` (and changes nothing) if the name is
    /// already taken.
    pub fn define(&mut self, def: ObjectDef) -> bool {
        if self.find_object(&def.name).is_some() {
            return false;
        }
        self.objects.push(def);
        true
    }

    pub fn place(&mut self, object: impl Into<String>, x: i32, y: i32) {
        self.placed.push(Placed {
            object: object.into(),
            x,
            y,
        });
    }

    /// The room file text. Stable, human-readable, friendly to git diffs.
    pub fn to_text(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .expect("a Room always serialises")
    }

    pub fn from_text(text: &str) -> Result<Self, ParseError> {
        ron::from_str(text).map_err(|e| ParseError {
            line: e.span.start.line,
            column: e.span.start.col,
            message: e.code.to_string(),
        })
    }
}

#[cfg(test)]
mod tests;
