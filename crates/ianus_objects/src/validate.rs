//! Checks a room and explains problems in plain English.
//!
//! The goal is that a beginner who typed something wrong reads one sentence
//! that says what is wrong, where, and what to do, instead of a Rust or
//! parser error.

use std::collections::HashMap;

use crate::{Action, AgentTarget, Category, Kind, MAX_NAV_TAGS, ObjectDef, Room};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The room cannot work as written.
    Error,
    /// The room runs, but this is probably not what was meant.
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub severity: Severity,
    pub message: String,
}

impl Problem {
    fn error(message: String) -> Self {
        Self {
            severity: Severity::Error,
            message,
        }
    }

    fn warning(message: String) -> Self {
        Self {
            severity: Severity::Warning,
            message,
        }
    }
}

impl core::fmt::Display for Problem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let tag = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{tag}: {}", self.message)
    }
}

/// How many single-character edits turn `a` into `b` (insert, delete, change).
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut diagonal = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let above = row[j];
            row[j] = (diagonal + usize::from(a[i - 1] != b[j - 1]))
                .min(row[j] + 1)
                .min(row[j - 1] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The closest known name to `wrong`, if it is close enough to be a typo.
fn did_you_mean<'a>(wrong: &str, known: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let limit = if wrong.chars().count() <= 4 { 1 } else { 2 };
    known
        .map(|k| (edit_distance(wrong, k), k))
        .filter(|&(d, _)| d <= limit)
        .min_by_key(|&(d, _)| d)
        .map(|(_, k)| k)
}

fn check_definition(def: &ObjectDef, problems: &mut Vec<Problem>) {
    let name = &def.name;
    if name.trim().is_empty() {
        problems.push(Problem::error(
            "An object has no name. Give every object a name so rooms can refer to it.".into(),
        ));
    }
    match &def.kind {
        Kind::SubstanceSource { rate_per_tick: 0, .. } => problems.push(Problem::warning(format!(
            "Substance source '{name}' adds 0 per tick, so it never produces anything. Try a rate of 1."
        ))),
        Kind::Interactable { radius_tiles: 0, .. } => problems.push(Problem::warning(format!(
            "Interactable '{name}' has radius 0, so nothing can ever trigger it. Try a radius of 1."
        ))),
        Kind::Environment { walkable: true, speed_percent: 0 } => problems.push(Problem::warning(format!(
            "Environment '{name}' is walkable but slows movement to 0%, so nothing can cross it. \
             Either make it not walkable or give it a speed above 0."
        ))),
        Kind::Entity { controlled: false, speed_tiles_per_s: 0 } => problems.push(Problem::warning(format!(
            "Entity '{name}' is not controlled and has speed 0, so it will never move."
        ))),
        _ => {}
    }

    if let Kind::Light {
        radius_tiles,
        intensity_percent,
        ..
    } = &def.kind
    {
        if *radius_tiles == 0 {
            problems.push(Problem::warning(format!(
                "Light '{name}' has radius 0, so it lights nothing. Try a radius of 3 or more."
            )));
        }
        if *intensity_percent > 100 {
            problems.push(Problem::warning(format!(
                "Light '{name}' has intensity {intensity_percent}%. The scale is 0 to 100; anything higher is treated as 100."
            )));
        }
    }

    // Hitbox and collision switches.
    if def.solid && !def.collision {
        problems.push(Problem::warning(format!(
            "'{name}' is solid but its collision is off, so nothing will stop at it. \
             Turn collision on, or turn solid off."
        )));
    }
    if def.collision && (def.hitbox.width == 0 || def.hitbox.height == 0) {
        let walls = if matches!(def.kind, Kind::Entity { .. }) {
            " An entity with no area will also walk through walls and out of the room."
        } else {
            ""
        };
        problems.push(Problem::warning(format!(
            "'{name}' has a hitbox of {}x{}, so nothing can ever touch it.{walls} \
             Give it a size (100 is one tile) or turn its collision off.",
            def.hitbox.width, def.hitbox.height
        )));
    }
    if matches!(def.kind, Kind::Wall { .. }) && def.collision && !def.solid {
        problems.push(Problem::warning(format!(
            "Wall '{name}' is not solid, so things will walk straight through it. Turn solid on."
        )));
    }

    // Tick rate.
    if def.ob_tick > MAX_OB_TICK {
        problems.push(Problem::warning(format!(
            "'{name}' has ob_tick {}, which is more than an hour between updates. \
             It is treated as {MAX_OB_TICK}. Use 0 if it should never update.",
            def.ob_tick
        )));
    }
    if def.ob_tick == 0
        && matches!(
            def.kind,
            Kind::Entity {
                controlled: true,
                ..
            }
        )
    {
        problems.push(Problem::warning(format!(
            "'{name}' is controlled by the keyboard but has ob_tick 0, so its own logic never runs. \
             It will still move. Use 1 unless that is what you want."
        )));
    }
}

/// Longest interval between updates (one hour at 60 ticks per second). Kept
/// equal to the scheduler's limit in `ianus_sim`.
pub const MAX_OB_TICK: u32 = 216_000;

impl Room {
    /// Everything wrong or suspicious about this room, worst first.
    pub fn validate(&self) -> Vec<Problem> {
        let mut problems = Vec::new();

        if self.width == 0 || self.height == 0 {
            problems.push(Problem::error(format!(
                "Room '{}' is {}x{} tiles, so there is no space to place anything. Give it a width and height of at least 1.",
                self.name, self.width, self.height
            )));
        }

        // World rules.
        if self.world.ob_tick_scale == 0 {
            problems.push(Problem::warning(
                "The world's ob_tick_scale is 0, so no object in this level ever updates. Use 1 for normal speed."
                    .into(),
            ));
        }

        // Definitions.
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for def in &self.objects {
            *seen.entry(def.name.as_str()).or_default() += 1;
            check_definition(def, &mut problems);
        }
        let mut duplicates: Vec<_> = seen.iter().filter(|&(_, &n)| n > 1).collect();
        duplicates.sort();
        for (name, count) in duplicates {
            problems.push(Problem::error(format!(
                "There are {count} objects called '{name}'. Names must be unique so placing '{name}' is unambiguous; rename one."
            )));
        }

        // The level's scale multiplies every interval; the result is capped.
        if self.world.ob_tick_scale > 1 {
            for def in &self.objects {
                if def.ob_tick <= MAX_OB_TICK
                    && def.ob_tick.saturating_mul(self.world.ob_tick_scale) > MAX_OB_TICK
                {
                    problems.push(Problem::warning(format!(
                        "'{}' has ob_tick {}, and the world's ob_tick_scale of {} stretches that past \
                         {MAX_OB_TICK} ticks (an hour). It is treated as {MAX_OB_TICK}.",
                        def.name, def.ob_tick, self.world.ob_tick_scale
                    )));
                }
            }
        }

        // A switch that toggles something that does not exist does nothing.
        let known = || self.objects.iter().map(|o| o.name.as_str());
        for def in &self.objects {
            if let Kind::Interactable {
                action: Action::Toggle(target),
                ..
            } = &def.kind
                && self.find_object(target).is_none()
            {
                let hint = match did_you_mean(target, known()) {
                    Some(k) => format!(" Did you mean '{k}'?"),
                    None => String::new(),
                };
                problems.push(Problem::warning(format!(
                    "Interactable '{}' toggles '{target}', but no object with that name is defined, so it will do nothing.{hint}",
                    def.name
                )));
            }
        }

        // Placements.
        let mut walls_at: HashMap<(i32, i32), usize> = HashMap::new();
        let mut non_wall_at: Vec<(&str, i32, i32)> = Vec::new();

        for p in &self.placed {
            let Some(def) = self.find_object(&p.object) else {
                let hint = match did_you_mean(&p.object, known()) {
                    Some(k) => format!(" Did you mean '{k}'?"),
                    None if self.objects.is_empty() => {
                        " This room defines no objects yet; create one first.".to_string()
                    }
                    None => format!(
                        " Defined objects: {}.",
                        known().collect::<Vec<_>>().join(", ")
                    ),
                };
                problems.push(Problem::error(format!(
                    "'{}' is placed at ({}, {}) but no object with that name is defined.{hint}",
                    p.object, p.x, p.y
                )));
                continue;
            };

            let inside =
                p.x >= 0 && p.y >= 0 && (p.x as u32) < self.width && (p.y as u32) < self.height;
            if !inside {
                problems.push(Problem::error(format!(
                    "'{}' is placed at ({}, {}), outside the {}x{} room. Valid tiles run from (0, 0) to ({}, {}).",
                    p.object,
                    p.x,
                    p.y,
                    self.width,
                    self.height,
                    self.width.saturating_sub(1),
                    self.height.saturating_sub(1)
                )));
                continue;
            }

            // Only whole-tile solids are checked here; odd-shaped ones would
            // need real geometry and are left to the game to show.
            if self.world.collision && def.blocks() && def.hitbox.is_full_tile() {
                *walls_at.entry((p.x, p.y)).or_default() += 1;
            } else if def.category() == Category::Entity && def.collision {
                non_wall_at.push((p.object.as_str(), p.x, p.y));
            }
        }

        let mut stacked: Vec<_> = walls_at.iter().filter(|&(_, &n)| n > 1).collect();
        stacked.sort();
        for (&(x, y), n) in stacked {
            problems.push(Problem::warning(format!(
                "{n} solid objects are stacked on tile ({x}, {y}). One is enough; the extras are just clutter."
            )));
        }
        for (name, x, y) in non_wall_at {
            if walls_at.contains_key(&(x, y)) {
                problems.push(Problem::warning(format!(
                    "'{name}' starts at ({x}, {y}), inside a solid object, so it will be stuck. Move it to an open tile."
                )));
            }
        }

        // Exactly one player.
        let controlled_placed = self
            .placed
            .iter()
            .filter(|p| {
                matches!(
                    self.find_object(&p.object).map(|d| &d.kind),
                    Some(Kind::Entity {
                        controlled: true,
                        ..
                    })
                )
            })
            .count();
        match controlled_placed {
            0 => problems.push(Problem::warning(
                "Nothing in this room is controlled by the keyboard, so WASD will do nothing. \
                 Place an Entity whose 'controlled' option is on."
                    .into(),
            )),
            1 => {}
            n => problems.push(Problem::error(format!(
                "{n} controlled entities are placed, but only one can follow the keyboard. Turn 'controlled' off on the others."
            ))),
        }

        self.check_nav(&mut problems);

        // Worst first, otherwise in the order found.
        problems.sort_by_key(|p| p.severity != Severity::Error);
        problems
    }

    /// Nav tags, tile painting and agents.
    fn check_nav(&self, problems: &mut Vec<Problem>) {
        let declared = || self.nav_tags.iter().map(String::as_str);
        let unknown_tag = |who: String, tag: &str, problems: &mut Vec<Problem>| {
            let hint = match did_you_mean(tag, declared()) {
                Some(k) => format!(" Did you mean '{k}'?"),
                None if self.nav_tags.is_empty() => {
                    " This room declares no nav tags yet.".to_string()
                }
                None => format!(
                    " Declared tags: {}.",
                    declared().collect::<Vec<_>>().join(", ")
                ),
            };
            problems.push(Problem::error(format!(
                "{who} uses the nav tag '{tag}', but the room does not declare it.{hint}"
            )));
        };

        if self.nav_tags.len() > MAX_NAV_TAGS {
            problems.push(Problem::error(format!(
                "The room declares {} nav tags, but at most {MAX_NAV_TAGS} are supported.",
                self.nav_tags.len()
            )));
        }
        let mut names: Vec<&str> = declared().collect();
        names.sort_unstable();
        for pair in names.windows(2) {
            if pair[0] == pair[1] {
                problems.push(Problem::error(format!(
                    "The nav tag '{}' is declared more than once. Each tag name must be unique.",
                    pair[0]
                )));
            }
        }
        if declared().any(|t| t.trim().is_empty()) {
            problems.push(Problem::error("A nav tag has an empty name.".into()));
        }

        for p in &self.nav_paint {
            if self.tag_bit(&p.tag).is_none() {
                unknown_tag(format!("The tile ({}, {})", p.x, p.y), &p.tag, problems);
            }
            let inside =
                p.x >= 0 && p.y >= 0 && (p.x as u32) < self.width && (p.y as u32) < self.height;
            if !inside {
                problems.push(Problem::error(format!(
                    "The nav tag '{}' is painted at ({}, {}), outside the {}x{} room.",
                    p.tag, p.x, p.y, self.width, self.height
                )));
            }
        }

        let controlled_placed = self.placed.iter().any(|p| {
            matches!(
                self.find_object(&p.object).map(|d| &d.kind),
                Some(Kind::Entity {
                    controlled: true,
                    ..
                })
            )
        });
        for def in &self.objects {
            let name = &def.name;
            for tag in &def.nav_tags {
                if self.tag_bit(tag).is_none() {
                    unknown_tag(format!("Object '{name}'"), tag, problems);
                }
            }
            let Some(agent) = &def.agent else { continue };
            match &def.kind {
                Kind::Entity { controlled: true, .. } => problems.push(Problem::error(format!(
                    "'{name}' is controlled by the keyboard and also has an agent, so the keys and the pathfinding would fight. Remove one."
                ))),
                Kind::Entity { .. } => {}
                _ => problems.push(Problem::error(format!(
                    "'{name}' has an agent but is not an Entity. Only entities can walk by themselves."
                ))),
            }
            for tag in agent.allow.iter().chain(&agent.avoid) {
                if self.tag_bit(tag).is_none() {
                    unknown_tag(format!("The agent '{name}'"), tag, problems);
                }
            }
            for (n, tc) in agent.costs.iter().enumerate() {
                if self.tag_bit(&tc.tag).is_none() {
                    unknown_tag(format!("The agent '{name}'"), &tc.tag, problems);
                }
                if tc.cost == 0 {
                    problems.push(Problem::warning(format!(
                        "Agent '{name}' has a cost of 0 on '{}', which changes nothing. Remove it or give it a cost (a plain step is 10).",
                        tc.tag
                    )));
                }
                if agent.avoid.contains(&tc.tag) {
                    problems.push(Problem::warning(format!(
                        "Agent '{name}' has a cost on '{}' and also avoids it. Avoiding wins, so the cost never applies.",
                        tc.tag
                    )));
                }
                if agent.costs[..n].iter().any(|o| o.tag == tc.tag) {
                    problems.push(Problem::warning(format!(
                        "Agent '{name}' lists a cost for '{}' more than once. The last one is used.",
                        tc.tag
                    )));
                }
            }
            if let Some(both) = agent.allow.iter().find(|t| agent.avoid.contains(t)) {
                problems.push(Problem::warning(format!(
                    "Agent '{name}' both allows and avoids the tag '{both}'. Avoiding wins, so it will never walk there."
                )));
            }
            if !def.collision {
                problems.push(Problem::warning(format!(
                    "Agent '{name}' has collision off, so it walks straight through walls. Its pathfinding still routes around them, which is probably not what you want."
                )));
            }
            if def.solid {
                problems.push(Problem::warning(format!(
                    "Agent '{name}' is solid, so every other agent treats it as a wall while it moves. Usually it should not be."
                )));
            }
            if def.hitbox.width > 75 || def.hitbox.height > 75 {
                problems.push(Problem::warning(format!(
                    "Agent '{name}' has a hitbox of {}x{}. Routes are planned for a body up to 75x75, so a bigger one can get stuck in one-tile gaps.",
                    def.hitbox.width, def.hitbox.height
                )));
            }
            if def.ob_tick == 0 {
                problems.push(Problem::warning(format!(
                    "Agent '{name}' has ob_tick 0, so it never plans a route and will stand still. Use 1 to replan every tick, or a larger number to replan less often."
                )));
            }
            match &agent.target {
                AgentTarget::Controlled if !controlled_placed => {
                    problems.push(Problem::warning(format!(
                        "Agent '{name}' chases the controlled entity, but none is placed in this room, so it has nothing to chase."
                    )));
                }
                AgentTarget::Named(target) if self.find_object(target).is_none() => {
                    let hint =
                        match did_you_mean(target, self.objects.iter().map(|o| o.name.as_str())) {
                            Some(k) => format!(" Did you mean '{k}'?"),
                            None => String::new(),
                        };
                    problems.push(Problem::warning(format!(
                        "Agent '{name}' chases '{target}', but no object with that name is defined, so it will stand still.{hint}"
                    )));
                }
                _ => {}
            }
        }
    }

    /// True if there are no [`Severity::Error`] problems.
    pub fn is_playable(&self) -> bool {
        self.validate()
            .iter()
            .all(|p| p.severity != Severity::Error)
    }
}

#[cfg(test)]
pub(crate) fn distance_for_tests(a: &str, b: &str) -> usize {
    edit_distance(a, b)
}

#[cfg(test)]
pub(crate) fn suggest_for_tests<'a>(wrong: &str, known: &[&'a str]) -> Option<&'a str> {
    did_you_mean(wrong, known.iter().copied())
}
