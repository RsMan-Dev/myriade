//! The spikes of a guardian (`Guardian.hurtServer`): while it does not move its spikes are out, and the living thing that hits it
//! takes 2 thorns damage from the guardian's own position. A projectile's blow (not `direct`) and a blow with no attacker are
//! exempt; so is `#avoids_guardian_thorns` (magic, thorns, explosions), which the `Damaged` event does not carry yet:
//! DOUTE : tant que `Damaged` n'a pas de cause, les dégâts magiques ou d'explosion `direct` seraient renvoyés à tort.
//! The spikes' look (`SPIKES`, the client animation) is not ported here.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::targeting::Who;

pub const THORNS: f32 = 2.0;

pub static MODULE: Module = Module { name: "guardian_thorns", flags: 0, damaged, requires: &[Cap::Body, Cap::Living], ..Module::DEFAULT };

/// The thorns of a blow that reached a still guardian.
fn damaged(w: &mut World, e: Entity, _: Inst, d: &Damaged) {
    if w.get::<Moving>(e).is_some() || !d.direct {
        return;
    }
    if let Some(who @ (Who::Player(_) | Who::Entity(_))) = d.who {
        crate::hostile::strike(w, e, who, THORNS);
    }
}
