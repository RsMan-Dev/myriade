//! The spikes of a guardian (`Guardian.hurtServer`): while it does not move its spikes are out, and the living thing that hits it
//! takes 2 thorns damage from the guardian's own position. A projectile's blow (not `direct`) and a blow with no attacker are
//! exempt; so is `#avoids_guardian_thorns` (magic, thorns, explosions): only a blow of the cause `Entity` is returned.
//! The spikes' look (`SPIKES`, the client animation) is not ported here.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::targeting::Who;

pub const THORNS: f32 = 2.0;

/// The guardian carries its spikes: `cow::apply_damage` returns them for every blow that reaches it, before the guards (`Guardian.hurtServer`
/// runs the spikes before `super.hurtServer`: a blow the invulnerability window refuses and the blow that kills draw them too).
#[derive(Component, Clone, Copy, Debug)]
pub struct Spiky;

pub static MODULE: Module = Module { name: "guardian_thorns", flags: 0, equip, can_use: never, requires: &[Cap::Body, Cap::Living, Cap::Locomotion], ..Module::DEFAULT };

fn equip(w: &mut World, e: Entity, _: Inst, _: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Spiky);
}
/// The thorns of a blow aimed at a still guardian: only a living direct cause (`Cause::Entity`, `direct`) that is not `#avoids_guardian_thorns`.
pub fn blow(w: &mut World, e: Entity, by: Option<Who>, direct: bool, cause: Cause) {
    if w.get::<Drive>(e).is_some_and(|d| d.moving) || !direct || cause != Cause::Entity {
        return;
    }
    if let Some(who @ (Who::Player(_) | Who::Entity(_))) = by {
        crate::hostile::strike(w, e, who, THORNS);
    }
}
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
