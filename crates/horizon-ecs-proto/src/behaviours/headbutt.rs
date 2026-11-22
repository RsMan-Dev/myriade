//! The hoglin's headbutt (`HoglinBase.throwTarget`, `mobs/piglin.rs:2061-2090`, and `hoglin_hurt_target` `:2112`): a strike that
//! lands on a creature throws it away from the attacker and up, by the attacker's knockback. The attack sound plays. It is
//! the melee's `on_blow` (the sound, the retreat) and `on_hit` (the throw, only when the blow landed), set by `equip`: the melee goal must
//! be equipped before this one. The throw subtracts the target's knockback resistance and adds to its velocity; a player is pushed by the host.
use super::*;
use crate::hostile::Attack;
use crate::rules::headbutt::throw_velocity;
use crate::targeting::{Target, Who};
use crate::cow::*;
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

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let knockback = params::<Params>(p).knockback;
    w.entity_mut(e).insert(Headbutt(knockback));
    if let Some(mut attack) = w.get_mut::<Attack>(e) {
        attack.on_blow = Some(blow);
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
    let piglin = w.resource::<SpeciesTable>().0.iter().find(|s| s.name == crate::species::PIGLIN).map(|s| s.id);
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

/// Every strike (`Hoglin.doHurtTarget` before the blow): the attack sound, and the retreat of `HoglinAi.onHitTarget`.
fn blow(w: &mut World, e: Entity) {
    if let Some(p) = w.get::<Pos>(e).map(|p| p.0) {
        w.resource_mut::<crate::goals::Events>().host.sound("entity.hoglin.attack", [p.x, p.y, p.z], 1.0, 1.0);
    }
    if let Some(Who::Entity(t)) = w.get::<Target>(e).map(|t| t.who) {
        on_hit_target(w, e, t);
    }
}

/// A blow that landed (`HoglinBase.hurtAndThrowTarget`): an adult throws its target.
fn hit(w: &mut World, e: Entity, who: Who, _dealt: f32) {
    if w.get::<GrowAt>(e).is_none() {
        throw(w, e, who);
    }
}

/// `HoglinBase.throwTarget`: the attacker's knockback less the target's resistance (nothing when it is not positive), the target pushed
/// away and up; a player is pushed by the host.
fn throw(w: &mut World, e: Entity, who: Who) {
    let Some(Headbutt(knockback)) = w.get::<Headbutt>(e).copied() else { return };
    let resist = match who {
        Who::Entity(t) => w.get::<Resist>(t).map_or(0.0, |r| r.0),
        Who::Player(_) => 0.0,
    };
    let strength = knockback - resist;
    let (Some(from), Some(to)) = (w.get::<Pos>(e).map(|p| p.0), crate::targeting::info(w, who).map(|i| i.pos)) else { return };
    if strength <= 0.0 {
        return;
    }
    let away = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
    let u = [unit(w, e), unit(w, e), unit(w, e)];
    let velocity = throw_velocity(away, strength, u);
    match who {
        Who::Entity(t) => {
            if let Some(mut v) = w.get_mut::<Vel>(t) {
                v.0 = v.0.add_vector(velocity);
                w.entity_mut(t).insert(Moving);
            }
        }
        Who::Player(id) => w.resource_mut::<crate::goals::Events>().host.push_player(id, [velocity.x, velocity.y, velocity.z]),
    }
}
