//! The pistons' push of the creatures (`PistonMovingBlockEntity.moveCollidedEntities`, `moveStuckEntities`, its creature part; the
//! geometry is in `horizon_world::piston_push`). The host calls `piston_push` when a moving piston's progress changes, with the
//! moved block's boxes and what the block is. The creatures come from the index cells the swept boxes touch, never from a scan;
//! a creature asleep (`Dormant`) that the push reaches is woken first. Not done here: the players, the shulker lids.
use crate::cow::*;
use crate::zone::{wake, Dormant};
use bevy_ecs::prelude::*;
use horizon_client_render::collisions::{with_piston_noclip, SceneCollisions};
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

/// `Entity.getPistonPushReaction` is `IGNORE_ENTITY` (marker, interaction, displays, effect cloud, ominous spawner): a piston moves
/// none of them. Put at creation by the species' `Cap::PistonIgnored`; a marker armor stand is the `Stand` flag, read in `reaction`.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct PistonIgnored;

/// `canBeCollidedWith` holds while it lives (the shulker): a pushed entity stops at its box. Put at creation by `Cap::Collidable`.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Collidable;

/// `HappyGhast.canBeCollidedWith`: an adult on its still timeout. Put at creation by `Cap::CollidableWhenStill`.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct CollidableWhenStill;

/// The `entity_move` decision of a mod for one push (`Game::entity_move`): the host arms it with the entity's network id, its type
/// name and the displacement asked; `None` cancels the push, `Some(d)` runs it with `d`. `alloc` gives a free network id to an entity
/// that has none yet. Given only while a mod listens: without it a push costs nothing more.
pub struct PushGate<'a> {
    pub alloc: &'a mut dyn FnMut() -> u32,
    pub arm: &'a mut dyn FnMut(u32, &str, Vec3) -> Option<Vec3>,
}

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
pub fn piston_push(
    w: &mut World,
    pos: BlockPos,
    piston: MovingPistonState,
    new_progress: f32,
    movement: Direction,
    block: &PushBlock,
    mut gate: Option<PushGate<'_>>,
) {
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
        // `moveCollidedEntities` skips an `IGNORE_ENTITY` entity before anything else, the slime bounce included.
        if reaction(w, e) == PushReaction::IgnoreEntity {
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
        let moved = push_by(w, e, step_vector(movement).scale(amount), movement, now, &mut gate);
        moved_by.insert(e, moved);
        // `fixEntityWithinPistonBase`: a retracting source piston drags an entity that is still inside its own block.
        if !piston.extending && piston.source {
            if let Some(after) = bounds_of(w, e) {
                if let Some(fix) = fix_within_piston_base(pos, after, movement, delta) {
                    push_by(w, e, step_vector(movement.opposite()).scale(fix), movement, now, &mut gate);
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
            if sticky_entity_selected(sticky, bounds, reaction(w, e), on_ground, supported, position) {
                wake_if_asleep(w, e, now);
                push_by(w, e, step_vector(movement).scale(delta), movement, now, &mut gate);
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

/// `Entity.getPistonPushReaction`: `IgnoreEntity` for the kinds that carry `PistonIgnored` and for a marker armor stand (its flag,
/// which changes at run time), else the normal reaction.
fn reaction(w: &World, e: Entity) -> PushReaction {
    if w.get::<PistonIgnored>(e).is_some() || w.get::<crate::stand::Stand>(e).is_some_and(|s| s.flags & crate::stand::MARKER != 0) {
        PushReaction::IgnoreEntity
    } else {
        PushReaction::PushPull
    }
}

/// `Entity.canCollideWith` of `other` for the pushed `e`: `canBeCollidedWith` (a boat, a living shulker, an adult happy ghast on its
/// still timeout) unless one rides the other or both ride the same vehicle. A happy ghast carrying a rider, for a happy ghast
/// mover, is not read.
fn collides_with(w: &World, e: Entity, other: Entity) -> bool {
    if other == e {
        return false;
    }
    let (mine, theirs) = (w.get::<crate::seat::Seated>(e).map(|s| s.vehicle), w.get::<crate::seat::Seated>(other).map(|s| s.vehicle));
    if mine == Some(other) || theirs == Some(e) || mine.is_some() && mine == theirs {
        return false;
    }
    if w.get::<crate::boat_state::BoatMarker>(other).is_some() {
        return true;
    }
    let alive = w.get::<Health>(other).is_none_or(|h| h.0 > 0.0);
    if w.get::<Collidable>(other).is_some() {
        return alive;
    }
    // `HappyGhast.canBeCollidedWith(entity)`: a ghast with a passenger (a creature or a player) is not solid for another happy ghast.
    let carrying = w.get::<crate::seat::Seats>(other).is_some_and(|s| s.riders.iter().flatten().next().is_some());
    w.get::<CollidableWhenStill>(other).is_some()
        && alive
        && !(carrying && w.get::<CollidableWhenStill>(e).is_some())
        && w.get::<GrowAt>(other).is_none()
        && w.get::<crate::behaviours::still_under_riders::StillUntil>(other).is_some_and(|u| now_of(w, other) < u.0)
}

/// `EntityGetter.getEntityCollisions` for a pushed creature: the boxes of the solid entities its sweep (the box and the box moved by
/// `movement`) reaches, from the index cells it touches. A boat or a cart collides by `canVehicleCollide` instead: none here.
pub fn solid_colliders(w: &World, e: Entity, bounds: Aabb, movement: Vec3) -> Vec<Aabb> {
    if w.get::<crate::boat_state::BoatMarker>(e).is_some() || w.get::<crate::cart_state::Cart>(e).is_some() {
        return Vec::new();
    }
    let sweep = bounds.union_bounds(bounds.move_by(movement));
    let (mn, mx, centre) = (sweep.min_position(), sweep.max_position(), sweep.center());
    let mut out = Vec::new();
    w.resource::<Index>().for_near_touching(centre.x, centre.z, (mn.x, mn.z), (mx.x, mx.z), |o| {
        if let Some(b) = bounds_of(w, o) {
            if sweep.intersects(b) && collides_with(w, e, o) {
                out.push(b);
            }
        }
    });
    out
}

/// The mover of `solid_boxes`, an entity the level does not hold: a player (its feet at `y`, and the vehicle it rides, which does not
/// stop it), or any other (a dropped item, a stand, a falling block).
#[derive(Clone, Copy, Debug)]
pub enum Mover {
    Other,
    Player { y: f64, vehicle: Option<Entity> },
}

/// `getEntityCollisions` for a mover the level does not hold, over `sweep`: a boat, a living shulker and an adult happy ghast (on its
/// still timeout, or, for a player at or above its top, always: `HappyGhast.canBeCollidedWith`). From the index cells `sweep` touches.
pub fn solid_boxes(w: &World, sweep: Aabb, mover: Mover) -> Vec<Aabb> {
    let (mn, mx, centre) = (sweep.min_position(), sweep.max_position(), sweep.center());
    let mut out = Vec::new();
    w.resource::<Index>().for_near_touching(centre.x, centre.z, (mn.x, mn.z), (mx.x, mx.z), |o| {
        let Some(b) = bounds_of(w, o).filter(|b| sweep.intersects(*b)) else { return };
        if matches!(mover, Mover::Player { vehicle: Some(v), .. } if v == o) {
            return;
        }
        let alive = w.get::<Health>(o).is_none_or(|h| h.0 > 0.0);
        let solid = w.get::<crate::boat_state::BoatMarker>(o).is_some()
            || w.get::<Collidable>(o).is_some() && alive
            || w.get::<CollidableWhenStill>(o).is_some()
                && alive
                && w.get::<GrowAt>(o).is_none()
                && (matches!(mover, Mover::Player { y, .. } if y >= b.max_position().y)
                    || w.get::<crate::behaviours::still_under_riders::StillUntil>(o).is_some_and(|u| now_of(w, o) < u.0));
        if solid {
            out.push(b);
        }
    });
    out
}

fn wake_if_asleep(w: &mut World, e: Entity, now: u64) {
    if w.get::<Dormant>(e).is_some() {
        wake(w, e, now);
    }
}

/// `move_pushed`: the displacement limited by the tick's budget, then run through the block collisions, and applied to the
/// creature's position and box.
fn push_by(w: &mut World, e: Entity, want: Vec3, dir: Direction, now: u64, gate: &mut Option<PushGate<'_>>) -> Vec3 {
    // `Entity.move` asks the mods first (`entity_move`), before the piston limit: a cancelled push spends none of the budget.
    let want = match gate {
        Some(g) => {
            let id = match crate::bridge::ids::id_of(w, e) {
                Some(id) => id,
                None => loop {
                    let id = (g.alloc)();
                    if crate::bridge::ids::bind(w, e, id) {
                        break id;
                    }
                },
            };
            let kind = w.get::<TypeName>(e).map_or("", |t| t.0);
            match (g.arm)(id, kind, want) {
                Some(d) => d,
                None => return Vec3::ZERO,
            }
        }
        None => want,
    };
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
    // `limitPistonMovement` gives zero once the tick's budget is spent: `Entity.move` stops there.
    if limited == Vec3::ZERO {
        return Vec3::ZERO;
    }
    let Some(bounds) = bounds_of(w, e) else { return Vec3::ZERO };
    let level = *w.resource::<Level>();
    let mw = level.mw.0;
    let (on_ground, step) = (w.get::<Contact>(e).is_some_and(|c| c.on_ground), w.get::<Body>(e).map_or(0.0, |b| f64::from(b.step_height)));
    let scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
    let solids = solid_colliders(w, e, bounds, limited);
    // The moving pistons of the push's own direction let it through (`PistonMovingBlockEntity.NOCLIP`).
    let moved = with_piston_noclip(dir, || scene.resolve_in_border_with(bounds, limited, step as f32, on_ground, None, &solids)).unwrap_or(Vec3::ZERO);
    // `Entity.move`: a step shorter than 1.0E-7 (squared) that a collision cut is not applied.
    let sq = |v: Vec3| v.x * v.x + v.y * v.y + v.z * v.z;
    if sq(moved) > 1.0e-7 || sq(limited) - sq(moved) < 1.0e-7 {
        if let Some(mut p) = w.get_mut::<Pos>(e) {
            p.0 = Vec3::new(p.0.x + moved.x, p.0.y + moved.y, p.0.z + moved.z);
        }
        if let Some(mut b) = w.get_mut::<Bounds>(e) {
            b.0 = b.0.move_by(moved);
        }
    }
    // On the ground only when the downward part of the push was blocked.
    if let Some(mut c) = w.get_mut::<Contact>(e) {
        c.on_ground = limited.y < 0.0 && limited.y != moved.y;
    }
    restitute(w, e, limited, moved);
    w.entity_mut(e).insert((Moving, Recheck));
    moved
}

/// `Entity.restituteMovementAfterCollisions` after a push: an axis the collision cut off loses its velocity, and a cut-off fall
/// onto a slime block (the block 0.2 under the feet) bounces by the block's restitution times the entity's own (`Restitution`
/// declared by its kind, else `OBJECT_RESTITUTION`) and its gravity (`Body`), with the
/// gravity and the air drag (0.98) the rest of the tick would have added.
fn restitute(w: &mut World, e: Entity, limited: Vec3, moved: Vec3) {
    let differs = |a: f64, b: f64| (b - a).abs() >= f64::from(1.0e-5_f32);
    let (cx, cz) = (differs(limited.x, moved.x), differs(limited.z, moved.z));
    let vertical = limited.y != moved.y;
    if !(cx || cz || limited.y.abs() > 0.0 && vertical) {
        return;
    }
    let Some(old) = w.get::<Vel>(e).map(|v| v.0) else { return };
    let mut v = old;
    if cx {
        v.x = 0.0;
    }
    if cz {
        v.z = 0.0;
    }
    if vertical {
        let gravity = w.get::<Body>(e).map_or(crate::rules::minecart::GRAVITY, |b| b.gravity);
        let restitution = w.get::<Restitution>(e).map_or(OBJECT_RESTITUTION, |r| r.0);
        let p = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
        let on = [p.x.floor() as i32, (p.y - 0.2).floor() as i32, p.z.floor() as i32];
        v.y = if limited.y < 0.0 && crate::level::block_name(w, on) == "minecraft:slime_block" && -old.y > gravity {
            let portion = moved.y / old.y;
            (portion * gravity - old.y) * (1.0 + portion * (f64::from(0.98_f32) - 1.0)) * restitution
        } else {
            0.0
        };
    }
    if let Some(mut vel) = w.get_mut::<Vel>(e) {
        vel.0 = v;
    }
}
