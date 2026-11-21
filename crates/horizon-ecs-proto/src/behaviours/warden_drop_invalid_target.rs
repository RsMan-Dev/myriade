//! `StopAttackingIfTargetInvalid` du Warden (`warden_brain.rs:975-995`, Fight prio 11) : une cible morte, hors d'atteinte
//! ou non attaquable (plus angry) est lâchée. Si elle n'est plus attaquable, sa colère est effacée ; le creusement
//! reprend après `dig_cooldown` ticks. (Le remplacement par une autre cible du Mob n'est pas porté.)
use super::act_phase::ActPhase;
use super::suspect_anger::{self, Level, Suspects};
use super::{params, Inst, Module};
use crate::{cow::*, goals::*, targeting::{self, Target}};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub dig_cooldown: u64,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub &'static Params);

pub static MODULE: Module = Module { name: "warden_drop_invalid_target", flags: 0, equip, can_use, start, ..Module::DEFAULT };

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Cfg(params::<Params>(p)));
}

fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(t) = w.get::<Target>(e) else { return false };
    let who = t.who;
    targeting::info(w, who).is_none() || !targeting::can_target(w, e, who) || suspect_anger::level(w, e) != Level::Angry
}

fn start(w: &mut World, e: Entity, _: Inst) {
    let c = w.get::<Cfg>(e).unwrap().0;
    let Some(who) = w.get::<Target>(e).map(|t| t.who) else { return };
    targeting::clear_target(w, e);
    if !targeting::can_target(w, e, who) {
        if let Some(mut s) = w.get_mut::<Suspects>(e) {
            s.list.retain(|x| x.who != who);
        }
    }
    let now = now_of(w, e);
    w.get_mut::<ActPhase>(e).unwrap().dig_cooldown_until = now + c.dig_cooldown;
}
