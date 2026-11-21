//! A happy ghast heals while it lives (`happy_ghast.rs` `ai_step`, 604-623): one health per heal, up to its maximum; the interval is
//! `dry` ticks, or `wet` ticks while it stands in clouds or in precipitation. The constants come from the species. The goal ends
//! at each heal and re-arms for the next interval: no scan per tick.
//! DOUTE : la pluie et les nuages viennent du monde : `Wet` est un marqueur que le socle posera (aucun composant de météo ici).
//! DOUTE : la vie maximale (attribut vanille) n'est pas lue dans le cadre : `max` vient de l'espèce.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::rules::heal_weather::{heal_interval, healed};

/// Standing in clouds or in precipitation (set by the world, `Wet`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Wet;

/// The heal's constants (species).
#[derive(Component, Clone, Copy, Debug)]
pub struct Params {
    pub dry: i32,
    pub wet: i32,
    pub amount: f32,
    pub max: f32,
}
impl Params {
    pub const DEFAULT: Self = Self { dry: 600, wet: 20, amount: 1.0, max: 20.0 };
}

pub static MODULE: Module = Module {
    name: "heal_weather",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(*params::<Params>(p));
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let Some(cfg) = w.get::<Params>(e).copied() else { return };
    let interval = heal_interval(w.get::<Wet>(e).is_some(), cfg.dry, cfg.wet);
    crate::goals::arm(w, e, inst.slot, interval);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Health>(e).is_some_and(|h| h.0 > 0.0) && w.get::<Params>(e).is_some()
}
/// One heal of this tick, up to the maximum; the goal ends and re-arms.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(cfg) = w.get::<Params>(e).copied() else { return };
    if let Some(mut h) = w.get_mut::<Health>(e)
        && h.0 > 0.0
        && h.0 < cfg.max
    {
        h.0 = healed(h.0, cfg.amount, cfg.max);
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    arm(w, e, inst);
}
