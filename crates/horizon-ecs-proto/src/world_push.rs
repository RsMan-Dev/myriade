//! The pistons' push of the creatures (`PistonMovingBlockEntity.moveCollidedEntities`, `moveStuckEntities`, its creature part; the
//! geometry is in `horizon_world::piston_push`). The host calls `piston_push` when a moving piston's progress changes, with the
//! moved block's boxes and what the block is. The creatures come from the index cells the swept boxes touch, never from a scan;
//! a creature asleep (`Dormant`) that the push reaches is woken first. Not done here: the players, the shulker lids.
use crate::cow::*;
use crate::zone::{wake, Dormant};
use bevy_ecs::prelude::*;
use horizon_client_render::collisions::SceneCollisions;
use horizon_core::geometry::{Aabb, Direction, Vec3};
use horizon_core::position::BlockPos;
use horizon_world::level::data::PushReaction;
use horizon_world::collision_context::CollisionContext;
use horizon_world::ground_support::{find_supporting_block, GroundState};
use horizon_world::piston_movement::PistonMovement;
use horizon_world::piston_push::{
    bounce_velocity, collided_amount, fix_within_piston_base, move_by_position_and_progress, movement_search_area, sticky_area,
    sticky_entity_selected, step_vector,
};
use horizon_world::piston_shape::MovingPistonState;
use std::collections::HashMap;

/// The per-tick budget of each creature pushed by pistons (`Entity.limitPistonMovement`, 0.51 a tick per axis). Filled only by
/// a push; cleared at the first push of a new tick, and only when it holds something.
#[derive(Resource, Default)]
pub struct PistonBudget {
    tick: u64,
    moves: HashMap<Entity, PistonMovement>,
}

/// The moved block as the push reads it: its boxes (block-relative), slime and honey (a bounce, a sticky pull) and, for honey,
/// the top of the moved state's own box (`sticky_top`).
pub struct PushBlock<'a> {
    pub boxes: &'a [Aabb],
    pub slime: bool,
    pub honey: bool,
    pub sticky_top: f64,
}

/// `PistonMovingBlockEntity.moveCollidedEntities` and `moveStuckEntities` for the creatures: the piston at `pos` goes from
/// `piston.progress` to `new_progress` along `movement`.
pub fn piston_push(w: &mut World, pos: BlockPos, piston: MovingPistonState, new_progress: f32, movement: Direction, block: &PushBlock) {
    let delta = f64::from(new_progress - piston.progress);
    if delta <= 0.0 || block.boxes.is_empty() {
        return;
    }
    w.init_resource::<PistonBudget>();
    let now = w.resource::<Clock>().0;
    let area = block
        .boxes
        .iter()
        .map(|b| movement_search_area(move_by_position_and_progress(pos, *b, piston), movement, delta))
        .reduce(|a, b| a.union_bounds(b))
        .unwrap_or(Aabb::new(Vec3::ZERO, Vec3::ZERO));
    let (mn, mx, centre) = (area.min_position(), area.max_position(), area.center());
    let mut found: Vec<Entity> = Vec::new();
    // The displacement each collided creature got in this push: the move its ground support is read with (honey).
    let mut moved_by: HashMap<Entity, Vec3> = HashMap::new();
    w.resource::<Index>().for_near_touching(centre.x, centre.z, (mn.x, mn.z), (mx.x, mx.z), |e| found.push(e));
    for &e in &found {
        let Some(bounds) = bounds_of(w, e) else { continue };
        if !area.intersects(bounds) {
            continue;
        }
        if block.slime {
            if let Some(mut v) = w.get_mut::<Vel>(e) {
                v.0 = bounce_velocity(movement, v.0);
            }
        }
        let amount = collided_amount(block.boxes, pos, piston, new_progress, movement, bounds);
        if amount <= 0.0 {
            continue;
        }
        wake_if_asleep(w, e, now);
        let moved = push_by(w, e, step_vector(movement).scale(amount), now);
        moved_by.insert(e, moved);
        // `fixEntityWithinPistonBase`: a retracting source piston drags an entity that is still inside its own block.
        if !piston.extending && piston.source {
            if let Some(after) = bounds_of(w, e) {
                if let Some(fix) = fix_within_piston_base(pos, after, movement, delta) {
                    push_by(w, e, step_vector(movement.opposite()).scale(fix), now);
                }
            }
        }
    }
    // Honey: after the collided ones moved, the creatures on a honey block's sticky box are pulled along the axis of the move.
    if block.honey && matches!(movement, Direction::North | Direction::South | Direction::East | Direction::West) {
        let sticky = sticky_area(pos, piston, block.sticky_top);
        for &e in &found {
            let Some(bounds) = bounds_of(w, e) else { continue };
            let (on_ground, position) = (w.get::<Contact>(e).is_some_and(|c| c.on_ground), w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0));
            let supported = supported_by(w, e, pos, moved_by.get(&e).copied());
            if sticky_entity_selected(sticky, bounds, PushReaction::PushPull, on_ground, supported, position) {
                wake_if_asleep(w, e, now);
                push_by(w, e, step_vector(movement).scale(delta), now);
            }
        }
    }
}

/// `Entity.isSupportedBy(pos)`: the block under the box is the piston's. The ground is read now (`GroundState`, the support query
/// of `setOnGroundWithMovement`) because the ECS keeps no supporting block. `movement` is the move this push gave the creature, when
/// it gave one: the query retries at the footprint before that move, as the game's does.
fn supported_by(w: &World, e: Entity, pos: BlockPos, movement: Option<Vec3>) -> bool {
    let (Some(bounds), Some(p), Some(c)) = (bounds_of(w, e), w.get::<Pos>(e), w.get::<Contact>(e)) else { return false };
    let level = *w.resource::<Level>();
    let mw = level.mw.0;
    let mut scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
    let mut ground = GroundState::default();
    let read = ground.check_support(c.on_ground, bounds, movement, |foot| find_supporting_block(&mut scene, CollisionContext::empty(false), p.0, foot, 1_000_000));
    read.is_ok() && ground.supporting_block == Some(pos)
}

fn bounds_of(w: &World, e: Entity) -> Option<Aabb> {
    w.get::<Bounds>(e).map(|b| b.0)
}

fn wake_if_asleep(w: &mut World, e: Entity, now: u64) {
    if w.get::<Dormant>(e).is_some() {
        wake(w, e, now);
    }
}

/// `move_pushed`: the displacement limited by the tick's budget, then run through the block collisions, and applied to the
/// creature's position and box.
fn push_by(w: &mut World, e: Entity, want: Vec3, now: u64) -> Vec3 {
    let limited = {
        let mut budget = w.resource_mut::<PistonBudget>();
        if budget.tick != now {
            if !budget.moves.is_empty() {
                budget.moves.clear();
            }
            budget.tick = now;
        }
        let state = budget.moves.entry(e).or_default();
        state.limit(want, || Ok::<i64, ()>(now as i64)).unwrap_or(Vec3::ZERO)
    };
    let Some(bounds) = bounds_of(w, e) else { return Vec3::ZERO };
    let level = *w.resource::<Level>();
    let mw = level.mw.0;
    let (on_ground, step) = (w.get::<Contact>(e).is_some_and(|c| c.on_ground), w.get::<Body>(e).map_or(0.0, |b| f64::from(b.step_height)));
    let scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
    let moved = scene.resolve_in_border_with(bounds, limited, step as f32, on_ground, None, &[]).unwrap_or(Vec3::ZERO);
    if let Some(mut p) = w.get_mut::<Pos>(e) {
        p.0 = Vec3::new(p.0.x + moved.x, p.0.y + moved.y, p.0.z + moved.z);
    }
    if let Some(mut b) = w.get_mut::<Bounds>(e) {
        b.0 = b.0.move_by(moved);
    }
    w.entity_mut(e).insert((Moving, Recheck));
    moved
}
