//! The hoglin's headbutt (`HoglinBase.throwTarget`, `mobs/piglin.rs:2061-2090`, and `hoglin_hurt_target` `:2112`): a strike that
//! lands on a creature throws it away from the attacker and up, by the attacker's knockback. The attack sound plays. It is
//! the melee's `on_hit` (as the bee's sting), set by `equip`: the melee goal must be equipped before this one.
//! Not ported: the target's knockback resistance (not read here) and a player target (no ECS velocity hook for players).
use super::*;
use crate::hostile::Attack;
use crate::targeting::{Target, Who};
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `Attributes.ATTACK_KNOCKBACK` of the attacker.
    pub knockback: f64,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Headbutt(f64);

pub static MODULE: Module = Module {
    name: "headbutt",
    flags: 0,
    equip,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    let knockback = params::<Params>(p).knockback;
    w.entity_mut(e).insert(Headbutt(knockback));
    if let Some(mut attack) = w.get_mut::<Attack>(e) {
        attack.on_hit = Some(hit);
    }
}

/// A uniform draw in `[0, 1)` from the attacker's random source.
fn unit(w: &mut World, e: Entity) -> f64 {
    w.get_mut::<Rng>(e).map_or(0.5, |mut r| f64::from(r.next_u32()) / f64::from(u32::MAX))
}

/// `HoglinAi.onHitTarget`: an adult that hits a piglin while the adult piglins it sees outnumber the adult hoglins retreats from it
/// and tells the adult hoglins it sees to (`broadcastRetreat`: each handles it at its own next tick, here at once).
fn on_hit_target(w: &mut World, e: Entity, target: Entity) {
    if w.get::<crate::cow::GrowAt>(e).is_some() || !crate::brain::sensors::hoglin_sight::piglins_outnumber(w, e) {
        return;
    }
    let piglin = w.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:piglin").map(|s| s.id);
    if piglin.is_none() || crate::targeting::info(w, Who::Entity(target)).map(|i| i.species) != piglin {
        return;
    }
    let who = Who::Entity(target);
    super::retreat_on_hurt::avoid_target(w, e, who);
    if let Some(crate::brain::MemValue::Whos(others)) = crate::brain::get(w, e, &crate::brain::sensors::hoglin_sight::HOGLINS).cloned() {
        for o in others {
            if let Who::Entity(o) = o {
                super::retreat_on_hurt::retreat_from_nearest(w, o, who);
            }
        }
    }
}

fn hit(w: &mut World, e: Entity) {
    let struck = w.get::<Target>(e).map(|t| t.who);
    throw(w, e);
    // After the throw, which reads the target that the retreat forgets.
    if let Some(Who::Entity(t)) = struck {
        on_hit_target(w, e, t);
    }
}

fn throw(w: &mut World, e: Entity) {
    if let Some(p) = w.get::<Pos>(e).map(|p| p.0) {
        w.resource_mut::<crate::goals::Events>().host.sound("entity.hoglin.attack", [p.x, p.y, p.z], 1.0, 1.0);
    }
    let Some(Headbutt(knockback)) = w.get::<Headbutt>(e).copied() else { return };
    let Some(Who::Entity(t)) = w.get::<Target>(e).map(|t| t.who) else { return };
    let (Some(from), Some(to)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Pos>(t).map(|p| p.0)) else { return };
    if knockback <= 0.0 || w.get::<Vel>(t).is_none() {
        return;
    }
    let away = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
    let angle = ((unit(w, e) * 21.0).floor().min(20.0) - 10.0).to_radians();
    let scale = knockback * (unit(w, e) * 0.5 + 0.2);
    let up = knockback * unit(w, e) * 0.5;
    let push = away.normalize().scale(scale).rotate_y(angle as f32);
    if let Some(mut v) = w.get_mut::<Vel>(t) {
        v.0 = Vec3::new(push.x, up, push.z);
    }
    w.entity_mut(t).insert(Moving);
}
