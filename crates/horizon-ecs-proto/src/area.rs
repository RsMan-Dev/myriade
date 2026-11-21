//! Who stands in a box, and the generic acts on them (hurt, push): called by the module that acts at its own deadline, never scanned.
use crate::cow::{Bounds, Clock, DamageQueue, Index, Moving, Pos, SpeciesId, Vel};
use crate::goals::Online;
use crate::hostile::PlayerState;
use crate::rules::area::centre_radius;
use crate::targeting::{player_box, Who};
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

/// The box of `e` inflated by `by` blocks on each axis (`Mob.bounds().inflate`).
pub fn reach_box(w: &World, e: Entity, by: f64) -> Aabb {
    w.get::<Bounds>(e).map_or_else(|| Aabb::new(Vec3::ZERO, Vec3::ZERO), |b| b.0.inflate(Vec3::new(by, by, by)))
}
/// The living players whose box meets `area`.
pub fn players_in(w: &World, area: Aabb) -> Vec<u32> {
    let (on, ps) = (w.resource::<Online>(), w.resource::<PlayerState>());
    let mut v = Vec::new();
    let (c, r) = centre_radius(area);
    on.for_near(c.0, c.1, r, |p| {
        if ps.alive(p.id) && player_box(&p).intersects(area) {
            v.push(p.id);
        }
    });
    v
}
/// The creatures other than `except` whose box meets `area`.
pub fn creatures_in(w: &World, area: Aabb, except: Entity) -> Vec<Entity> {
    let mut v = Vec::new();
    let (c, r) = centre_radius(area);
    w.resource::<Index>().for_radius(c.0, c.1, r + 2.0, |c| {
        if c != except && w.get::<Bounds>(c).is_some_and(|b| b.0.intersects(area)) {
            v.push(c);
        }
    });
    v
}
pub fn player_xz(w: &World, id: u32) -> (f64, f64) {
    w.resource::<Online>().get(id).map_or((0.0, 0.0), |p| (p.x, p.z))
}
pub fn creature_xz(w: &World, e: Entity) -> (f64, f64) {
    w.get::<Pos>(e).map_or((0.0, 0.0), |p| (p.0.x, p.0.z))
}
pub fn species_name(w: &World, e: Entity) -> &'static str {
    w.get::<SpeciesId>(e).map_or("", |s| crate::species::registry().species[usize::from(s.0)].name)
}
/// `by` hurts the player `id` (the game applies the armour, the effects and the knock-back).
pub fn hurt_player(w: &mut World, id: u32, amount: f32, by: Entity) {
    let tick = w.resource::<Clock>().0;
    let from = creature_xz(w, by);
    w.resource_mut::<PlayerState>().hurt(tick, id, amount, from, Some(Who::Entity(by)));
}
/// `by` hurts the creature `c` (its knock-back comes from `by`'s position).
pub fn hurt_creature(w: &mut World, c: Entity, by: Entity, amount: f32) {
    let from = creature_xz(w, by);
    w.resource_mut::<DamageQueue>().add_by_entity(c, amount, from, by);
}
/// A velocity added to the player (`MobEvent::Push`): a host output.
pub fn push_player(w: &mut World, id: u32, v: [f64; 3]) {
    w.resource_mut::<crate::goals::Events>().host.push_player(id, v);
}
/// A velocity added to the creature (`Entity.push`), its physics woken.
pub fn push_creature(w: &mut World, c: Entity, v: [f64; 3]) {
    if let Some(mut vel) = w.get_mut::<Vel>(c) {
        vel.0 = Vec3::new(vel.0.x + v[0], vel.0.y + v[1], vel.0.z + v[2]);
        w.entity_mut(c).insert(Moving);
    }
}
