//! The wall climber (`Spider`, `CaveSpider`): the walk with `climber: true`. Pressed against a wall (`Contact.horizontal` of the last tick) it
//! climbs it (`travel_one`: the velocity is clamped, then `y = 0.2` while the contact lasts). `WallClimberNavigation`: whatever the path,
//! the climb goes on straight at the target block when the path is over or when there is none: a wish per animal (`ClimbWish`), run by
//! `arbitrate` (no system of its own: nothing costs while no climber has a wish).
use crate::cow::*;
use crate::rules::climb::keep_climbing;
use bevy_ecs::prelude::*;

/// The walk's `climber` parameter: the path requests of this animal leave a wish (`wish`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Climber;
/// The animal is climbing straight at its target (its move control runs without a path).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Climbing;
/// `WallClimberNavigation.pathToPosition` and the speed: on the animal.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct ClimbWish {
    pub cell: [i32; 3],
    pub at: f64,
}

/// A path was asked toward `to` at `speed`: the climb will go on to it.
pub fn wish(w: &mut World, e: Entity, to: [i32; 3], speed: f64) {
    if let Ok(mut m) = w.get_entity_mut(e) {
        m.insert(ClimbWish { cell: to, at: speed });
    }
}
/// `WallClimberNavigation.tick` when the path is over: a wanted point at the target (the block's corner, as the `Mob` writes it) until the
/// animal is at its centre (within its width) or at its height.
pub fn tick(w: &mut World) {
    if !crate::goals::sparse_any::<ClimbWish>(w) {
        return;
    }
    let all: Vec<(Entity, [i32; 3], f64)> = crate::queries::with::<(Entity, &'static ClimbWish), (), _>(w, |q, w| q.iter(w).map(|(e, c)| (e, c.cell, c.at)).collect());
    for (e, t, speed) in all {
        let (Some(p), Some(b)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Body>(e).map(|b| b.width)) else {
            w.entity_mut(e).remove::<ClimbWish>();
            continue;
        };
        // The animal that follows a path is not climbing (`is_done` is false).
        if w.get::<Following>(e).is_some() {
            continue;
        }
        let width = f64::from(b);
        if keep_climbing(p, width, t) {
            if let Some(mut m) = w.get_mut::<MoveTo>(e) {
                *m = MoveTo { active: true, x: f64::from(t[0]), y: f64::from(t[1]), z: f64::from(t[2]), speed_modifier: speed };
            }
            w.entity_mut(e).insert((Climbing, Moving));
        } else {
            w.entity_mut(e).remove::<ClimbWish>();
            // The climb is over: the move control no longer drives it (the walk's own path end does the same).
            if let Some(mut m) = w.get_mut::<MoveTo>(e) {
                m.active = false;
            }
            if let Some(mut d) = w.get_mut::<Drive>(e) {
                d.zza = 0.0;
                d.speed = 0.0;
                d.jump = false;
            }
            w.entity_mut(e).remove::<Climbing>();
        }
    }
}
