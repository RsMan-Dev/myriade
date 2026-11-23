//! The vibration ear (`VibrationUser` and `receiveVibration` in `mobs/warden.rs`). The sculk layer (socle, not written here) finds the
//! listeners and asks `listens`; when the vibration has arrived it calls `receive`. Nothing here scans: the cooldown and the
//! disturbance are own-time deadlines. The module has no goal: its `equip` gives the species' values to `receive`.
use super::Inst;
use super::act_phase::{ActPhase, Phase, RareState};
use super::{go_to_disturbance, params, suspect_anger::{self, Level}, Module};
use crate::rules::vibration_listen::{RANGE, block_of, dist_sqr, hear_delay, in_range};
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

/// A living source the wardens do not notice (`Warden.canTargetEntity` refuses it: an armor stand, another warden). Set by its species' rule.
#[derive(Component, Clone, Copy, Debug)]
pub struct Unnoticed;

/// The rule that gives a species the `Unnoticed` marker.
pub static UNNOTICED: crate::rules::Rule = crate::rules::Rule { name: "unnoticed_by_wardens", equip: |w, e, _| { w.entity_mut(e).insert(Unnoticed); }, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };

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
            // `Warden.canReceiveVibration` (VibrationUser): a living source the warden cannot target is not heard.
            // `Warden.canTargetEntity` also refuses an armor stand and another warden as a living source (the `Unnoticed` marker).
            if v.source.is_some_and(|s| !targeting::can_target(w, e, s) || matches!(s, Who::Entity(t) if w.get::<Unnoticed>(t).is_some())) {
                continue;
            }
            let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { continue };
            let d2 = dist_sqr(v.pos, [p.x, p.y, p.z]);
            if !in_range(d2) {
                continue;
            }
            w.entity_mut(e).insert(Heard(Some(v)));
            after(w, e, hear_delay(d2), deliver);
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

/// Within the owner range of the entity.
fn near(w: &World, e: Entity, who: Who, range_sqr: f64) -> bool {
    let (Some(i), Some(p)) = (targeting::info(w, who), w.get::<Pos>(e)) else { return false };
    dist_sqr([i.pos.x, i.pos.y, i.pos.z], [p.0.x, p.0.y, p.0.z]) < range_sqr
}

/// `receiveVibration`: the tendrils click, the source's anger grows, and the place becomes the disturbance unless it is angry.
pub fn receive(w: &mut World, e: Entity, v: &Vibration) {
    let now = now_of(w, e);
    let c = w.get::<Cfg>(e).unwrap().0;
    w.get_mut::<ActPhase>(e).unwrap().vibration_cooldown_until = now + c.cooldown;
    super::act_phase::sound(w, e, "entity.warden.tendril_clicks", 5.0, 1.0);
    let mut at = block_of(v.pos);
    match v.owner {
        Some(owner) => {
            if near(w, e, owner, c.owner_range_sqr) {
                if w.get::<ActPhase>(e).unwrap().recent_projectile_until > now {
                    if targeting::can_target(w, e, owner)
                        && let Some(i) = targeting::info(w, owner)
                    {
                        at = block_of([i.pos.x, i.pos.y, i.pos.z]);
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
