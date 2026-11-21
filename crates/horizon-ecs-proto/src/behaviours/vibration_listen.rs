//! The vibration ear (`VibrationUser` and `receiveVibration` in `mobs/warden.rs`). The sculk layer (socle, not written here) finds the
//! listeners and asks `listens`; when the vibration has arrived it calls `receive`. Nothing here scans: the cooldown and the
//! disturbance are own-time deadlines. The module has no goal: its `equip` gives the species' values to `receive`.
use super::Inst;
use super::act_phase::{ActPhase, Phase, RareState};
use super::{go_to_disturbance, params, suspect_anger::{self, Level}, Module};
use crate::{cow::*, goals::*, targeting::{self, Who}};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Ticks after a vibration before the next one is heard.
    pub cooldown: u64,
    /// Anger from a source that is not a projectile, and from a projectile's owner (`DEFAULT_ANGER`, `PROJECTILE_ANGER`).
    pub anger: u16,
    pub projectile_anger: u16,
    /// Ticks after a projectile in which the owner's next shot angers the full amount and moves the disturbance to the owner.
    pub projectile_window: u64,
    /// The owner counts only within this squared distance.
    pub owner_range_sqr: f64,
    /// Ticks the place of a vibration is kept.
    pub disturbance_expiry: u64,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub &'static Params);

/// The vibration as the sculk layer hands it over: where it came from and who made it (its owner, a projectile's shooter).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vibration {
    pub pos: [f64; 3],
    pub source: Option<Who>,
    pub owner: Option<Who>,
    pub projectile: bool,
}

/// The listening range in blocks (`VibrationUser` of the warden: 16).
const RANGE: f64 = 16.0;

/// The vibration a listener heard and that is still travelling to its ear: `deliver` takes it when its delay is over.
#[derive(Component, Default)]
pub struct Heard(Option<Vibration>);

pub static MODULE: Module = Module {
    name: "vibration_listen",
    flags: 0,
    equip,
    tick: Some(vibration_dispatch),
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Cfg(p));
    w.entity_mut(e).insert_if_new(ActPhase::default());
    w.entity_mut(e).insert_if_new(RareState::default());
    w.entity_mut(e).insert_if_new(Heard::default());
}

/// Each vibration of the tick goes to the listeners within `RANGE` (the index's columns, then the true distance): each one
/// that `listens` gets it after as many ticks of its own time as the distance in blocks.
fn vibration_dispatch(w: &mut World) {
    let vs: Vec<Vibration> = w.resource::<Events>().vibration.iter().collect();
    for v in vs {
        let mut near_ids = Vec::new();
        w.resource::<Index>().for_radius(v.pos[0], v.pos[2], RANGE, |e| near_ids.push(e));
        for e in near_ids {
            if w.get::<Cfg>(e).is_none() || !listens(w, e) {
                continue;
            }
            let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { continue };
            let d2 = (v.pos[0] - p.x).powi(2) + (v.pos[1] - p.y).powi(2) + (v.pos[2] - p.z).powi(2);
            if d2 > RANGE * RANGE {
                continue;
            }
            w.entity_mut(e).insert(Heard(Some(v)));
            after(w, e, d2.sqrt() as i32, deliver);
        }
    }
}

/// The wheel's call: the listener receives the vibration it kept (`Heard`), once.
fn deliver(w: &mut World, e: Entity) {
    let Some(v) = w.get_mut::<Heard>(e).and_then(|mut h| h.0.take()) else { return };
    receive(w, e, &v);
}

/// `canReceiveVibration`: not in its cooldown, not burrowing and not emerging.
pub fn listens(w: &World, e: Entity) -> bool {
    let now = now_of(w, e);
    let st = *w.get::<ActPhase>(e).unwrap();
    now >= st.vibration_cooldown_until && !matches!(st.phase, Phase::Burrowing | Phase::Emerging)
}

fn block(p: [f64; 3]) -> [i32; 3] {
    p.map(|c| c.floor() as i32)
}

/// Within the owner range of the entity.
fn near(w: &World, e: Entity, who: Who, range_sqr: f64) -> bool {
    let (Some(i), Some(p)) = (targeting::info(w, who), w.get::<Pos>(e)) else { return false };
    let d = [i.pos.x - p.0.x, i.pos.y - p.0.y, i.pos.z - p.0.z];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < range_sqr
}

/// `receiveVibration`: the tendrils click, the source's anger grows, and the place becomes the disturbance unless it is angry.
pub fn receive(w: &mut World, e: Entity, v: &Vibration) {
    let now = now_of(w, e);
    let c = w.get::<Cfg>(e).unwrap().0;
    w.get_mut::<ActPhase>(e).unwrap().vibration_cooldown_until = now + c.cooldown;
    super::act_phase::sound(w, e, "entity.warden.tendril_clicks", 5.0, 1.0);
    let mut at = block(v.pos);
    match v.owner {
        Some(owner) => {
            if near(w, e, owner, c.owner_range_sqr) {
                if w.get::<ActPhase>(e).unwrap().recent_projectile_until > now {
                    if targeting::can_target(w, e, owner)
                        && let Some(i) = targeting::info(w, owner)
                    {
                        at = block([i.pos.x, i.pos.y, i.pos.z]);
                    }
                    suspect_anger::raise(w, e, owner, c.anger);
                } else {
                    suspect_anger::raise(w, e, owner, c.projectile_anger);
                }
            }
            w.get_mut::<ActPhase>(e).unwrap().recent_projectile_until = now + c.projectile_window;
        }
        None => {
            if let Some(src) = v.source {
                suspect_anger::raise(w, e, src, c.anger);
            }
        }
    }
    if suspect_anger::level(w, e) != Level::Angry {
        let active = suspect_anger::live(w, e).first().map(|x| x.0);
        if v.owner.is_some() || active.is_none() || active == v.source {
            let mut r = w.get_mut::<RareState>(e).unwrap();
            r.disturbance = Some(at);
            r.disturbance_until = now + c.disturbance_expiry;
            drop(r);
            arm_module(w, e, &go_to_disturbance::MODULE, 1);
            arm_module(w, e, &super::warden_look_target::MODULE, 1);
        }
    }
}
