use bevy_app::{App, FixedUpdate, RunFixedMainLoop, RunFixedMainLoopSystems};
use bevy_ecs::prelude::*;
use bevy_time::{Fixed, Time};
use bevy_transform::components::Transform;
use ianus_sim::{Body, SUBUNITS_PER_TILE, Vec2i, step_body, step_body_solid};

use crate::collide::{Collider, SolidMap};
use crate::tick::{IanusSet, TickInput};

/// The simulated state of an entity. This, not `Transform`, is the truth;
/// `Transform` is only where it gets drawn.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[require(PrevPos, Transform)]
pub struct SimBody(pub Body);

/// Position one tick ago, kept so rendering can blend between ticks.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PrevPos(pub Vec2i);

/// Marks the entity that follows [`MoveIntent`] (the player's keys).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Controlled;

/// Rendering scale: how many `Transform` units one tile is. Default 32,
/// which suits 32x32-pixel tiles with a default 2D camera.
#[derive(Resource, Debug, Clone, Copy)]
pub struct WorldScale(pub f32);

impl Default for WorldScale {
    fn default() -> Self {
        Self(32.0)
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<WorldScale>()
        .add_systems(FixedUpdate, step_controlled.in_set(IanusSet::Sim))
        .add_systems(
            RunFixedMainLoop,
            (init_new_bodies, interpolate_transforms)
                .chain()
                .in_set(RunFixedMainLoopSystems::AfterFixedMainLoop),
        );
}

/// One simulation step for the player-controlled entity.
fn step_controlled(
    tick_input: Res<TickInput>,
    solid: Option<Res<SolidMap>>,
    mut bodies: Query<(&mut SimBody, &mut PrevPos, Option<&Collider>), With<Controlled>>,
) {
    let input = tick_input.0;
    for (mut body, mut prev, collider) in &mut bodies {
        // Resting body, no input: nothing to simulate and nothing to flag as
        // changed. Most entities in a big sim are in this state most ticks.
        if input == ianus_sim::MoveInput::NONE && body.0.vel == Vec2i::ZERO {
            if prev.0 != body.0.pos {
                prev.0 = body.0.pos;
            }
            continue;
        }
        prev.0 = body.0.pos;
        match (&solid, collider) {
            // A room is loaded and this entity has a hitbox: solids stop it.
            (Some(map), Some(c)) => step_body_solid(&mut body.0, input, &map.0, &c.footprint),
            // No room, or collision is off for this entity: it passes through
            // everything, as open ground.
            _ => step_body(&mut body.0, input),
        }
    }
}

/// A freshly spawned body has no previous position; without this it would
/// visibly slide in from the origin on its first frame.
fn init_new_bodies(mut bodies: Query<(&SimBody, &mut PrevPos), Added<SimBody>>) {
    for (body, mut prev) in &mut bodies {
        prev.0 = body.0.pos;
    }
}

/// Draw each body between its last two ticks, by how far into the next tick
/// the frame is (`alpha`, 0 to 1). Math is done in f64 so far-from-origin
/// positions do not lose precision before the final cast.
fn interpolate_transforms(
    time: Res<Time<Fixed>>,
    scale: Res<WorldScale>,
    mut bodies: Query<(&SimBody, &PrevPos, &mut Transform)>,
) {
    let alpha = f64::from(time.overstep_fraction());
    let to_world = f64::from(scale.0) / f64::from(SUBUNITS_PER_TILE);

    let blend = |prev: i32, cur: i32| -> f32 {
        let delta = (i64::from(cur) - i64::from(prev)) as f64;
        ((f64::from(prev) + delta * alpha) * to_world) as f32
    };

    for (body, prev, mut transform) in &mut bodies {
        let x = blend(prev.0.x, body.0.pos.x);
        let y = blend(prev.0.y, body.0.pos.y);
        if transform.translation.x != x || transform.translation.y != y {
            transform.translation.x = x;
            transform.translation.y = y;
        }
    }
}
