//! L'usage de la lance par un mob (`SpearUseGoal` des zombies, `PiglinSpear` des piglins : un seul module, des paramètres).
//! Avec une arme cinétique en main et une cible : marcher jusqu'à la distance d'approche, lever la lance pendant le délai,
//! charger, frapper ce qui est à portée, puis reculer vers un point à 9-11 blocs et finir (`restart: false`) ou recommencer
//! (`restart: true`). La monture (`dismount`) et le recul (`knockback`) de la frappe sont portés ; la direction de visée est
//! celle de la cible et la vitesse de la cible est nulle (voir les notes, section 5).
use super::*;
use crate::{brain, cow::*, goals::*, hostile::strike, mount::Riding, spots::{self, Pick}, targeting::{info, Target, Who}};
use crate::rules::spear_use as rules;
use myriade_engine_lib::spear::{hits_in_reach, Kinetic, REACH};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed_charge: f64,
    pub speed_reposition: f64,
    /// Distance from the target at which the use starts (farther: walk to it).
    pub approach: f64,
    /// Distance to the target at which the charge gives way to a spot 6 to 7 blocks away.
    pub target_radius: f64,
    /// Ticks of fleeing before the goal gives up (`MAX_FLEEING_TIME`).
    pub flee_max: i32,
    /// Starts over after the retreat (piglin) instead of ending (zombie).
    pub restart: bool,
    /// The use duration is halved (the zombie goal's `reducedTickDelay`).
    pub reduced_use: bool,
    /// The charge speed and the two blocks of distance a mount adds (zombie horse 1.4 and camel husk 4.0 in vanilla).
    pub mount_speed: f64,
    pub mount_distance: i32,
}
impl Params {
    /// `SpearUse::new(1.0, 1.0, 10.0, 2.0)` of the zombies.
    pub const DEFAULT: Self = Self {
        speed_charge: 1.0,
        speed_reposition: 1.0,
        approach: 10.0,
        target_radius: 2.0,
        flee_max: 50,
        restart: false,
        reduced_use: true,
        mount_speed: 1.0,
        mount_distance: 0,
    };
}
/// `PiglinSpear`: the approach and the attack at the speed 1.0, the retreat, then again; its flee limit is `reducedTickDelay(200)`.
pub const PIGLIN: Params = Params { flee_max: 100, restart: true, reduced_use: false, ..Params::DEFAULT };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Approach,
    Charge,
    Retreat,
}
/// The goal's state. `engage` counts down the use (`-1`: not engaged yet), `fleeing` the retreat (`-1`: not fleeing).
/// What stays on the animal: the species' parameters (referenced) and the goal's rank.
#[derive(Component, Clone, Copy, Debug)]
pub struct SpearDef {
    pub def: &'static Params,
    pub slot: u8,
}
/// The state of a use in progress (sparse: laid by `start`, taken off by `stop`; the animals that never use it carry none).
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Spear {
    pub def: &'static Params,
    pub slot: u8,
    pub phase: Phase,
    pub engage: i32,
    pub used: i32,
    pub fleeing: i32,
    pub away: Option<[f64; 3]>,
    /// The targets stabbed and the clock tick of each stab (the contact cooldown).
    pub stabbed: Vec<(Who, u64)>,
    /// The position at the previous tick and what the body moved since (`Mob.previous`: the known speed of the attacker).
    pub last: Vec3,
    pub moved: Vec3,
}
/// The goal runs: its system visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Spearing;

pub static MODULE: Module = Module {
    name: "spear_use",
    flags: MOVE | LOOK,
    resources,
    equip,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion, Cap::Targets],
    ..Module::DEFAULT
};

/// The target service wakes the goal when its creature gets a target: `ableToAttack` is asked then (an event, not a poll).
fn resources(w: &mut World) {
    w.resource_mut::<crate::targeting::TargetWatchers>().0.push(|w, e| {
        if let Some(slot) = w.get::<SpearDef>(e).map(|s| s.slot) {
            w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
        }
    });
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let def = params::<Params>(p);
    w.entity_mut(e).insert(SpearDef { def, slot: inst.slot });
}
/// `ableToAttack`: a target and a kinetic weapon in the main hand.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Target>(e).is_some() && kinetic(w, e).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let Some(d) = w.get::<SpearDef>(e).copied() else { return };
    w.entity_mut(e).insert((Spear { def: d.def, slot: d.slot, phase: Phase::Approach, engage: -1, used: 0, fleeing: -1, away: None, stabbed: Vec::new(), last: Vec3::ZERO, moved: Vec3::ZERO }, Spearing));
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<(Spearing, Spear)>();
}
/// The goal runs while its use is not over (`fired` is what the scheduler reads: the goal wants to go on).
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Spearing>(e).is_none()
}
fn kinetic(w: &World, e: Entity) -> Option<Kinetic> {
    main_hand(w, e).and_then(|s| Kinetic::of(&s.item))
}
/// `getKineticWeaponUseDuration`: the delay and the damage window, halved for the zombies.
fn use_duration(k: &Kinetic, def: &Params) -> i32 {
    rules::halved_use(k.damage_use_duration(), def.reduced_use)
}
/// The charge speed and the distance bonus of a mount (none without one).
fn mount_terms(w: &World, e: Entity, def: &Params) -> (f64, i32) {
    rules::mount_bonus(w.get::<Riding>(e).is_some(), def.mount_speed, def.mount_distance)
}
fn tick(w: &mut World) {
    let list = crate::goals::entities::<With<Spearing>>(w);
    for e in list {
        step(w, e);
    }
}
/// `SpearUseGoal.tick`: the approach, the engagement, the retreat, and the stabs of the window.
fn step(w: &mut World, e: Entity) {
    let Some(mut s) = w.get::<Spear>(e).cloned() else { return };
    let (Some(who), Some(k)) = (w.get::<Target>(e).map(|t| t.who), kinetic(w, e)) else { return };
    let Some(t) = info(w, who) else { return };
    let me = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    s.moved = if s.last == Vec3::ZERO { Vec3::ZERO } else { Vec3::new(me.x - s.last.x, me.y - s.last.y, me.z - s.last.z) };
    s.last = me;
    w.get_mut::<Spear>(e).unwrap().last = me;
    let d = rules::dist(me, [t.pos.x, t.pos.y, t.pos.z]);
    let (mount_speed, bonus) = mount_terms(w, e, &s.def);
    let at = [t.pos.x, t.pos.y, t.pos.z];
    if s.phase == Phase::Approach {
        if d > s.def.approach {
            go_to_at(w, e, at, s.def.speed_reposition * mount_speed, 1);
            return;
        }
        s.engage = use_duration(&k, &s.def);
        s.used = 0;
        s.phase = Phase::Charge;
    }
    s.used += 1;
    if s.engage > 0 {
        s.engage -= 1;
        if s.engage == 0 {
            // The use is over: the spear is lowered and the retreat starts toward a spot 9 to 11 blocks from the target.
            let (lo, hi) = rules::ring(9, 11, bonus, d);
            s.away = spot(w, e, at, lo, hi, 0.0, 1.0);
            s.fleeing = 1;
            s.phase = Phase::Retreat;
        }
    }
    if s.engage >= 0 && s.used > k.delay_ticks {
        let used = s.used - k.delay_ticks;
        stab(w, e, &mut s, &k, who, used);
    }
    if s.fleeing > 0 {
        s.fleeing += 1;
        if s.fleeing > s.def.flee_max {
            return finish(w, e, s);
        }
    }
    match s.away {
        Some(a) => {
            go_to_at(w, e, a, s.def.speed_reposition * mount_speed, 1);
            if brain::nav_done(w, e) {
                if s.fleeing > 0 {
                    return finish(w, e, s);
                }
                s.away = None;
            }
        }
        None => {
            go_to_at(w, e, at, s.def.speed_charge * mount_speed, 1);
            if d < s.def.target_radius || brain::nav_done(w, e) {
                let (lo, hi) = rules::ring(6, 7, bonus, d);
                s.away = spot(w, e, at, lo, hi, f64::NEG_INFINITY, f64::NEG_INFINITY);
            }
        }
    }
    *w.get_mut::<Spear>(e).unwrap() = s;
}
/// The end of the goal: the zombie stops (`Req::End`), the piglin starts over at the approach.
fn finish(w: &mut World, e: Entity, mut s: Spear) {
    if s.def.restart {
        s = Spear { phase: Phase::Approach, engage: -1, fleeing: -1, away: None, used: 0, ..s };
        *w.get_mut::<Spear>(e).unwrap() = s;
        return;
    }
    w.entity_mut(e).remove::<(Spearing, Spear)>();
    w.resource_mut::<Requests>().0.push(Req::End(e, s.slot));
}
/// A spot at a distance between `min` and `max` of the target, as `land_random_pos_away_between` (the minimum is at least
/// `floor` and the maximum at least `ceil`, the vertical reach 7).
fn spot(w: &mut World, e: Entity, at: [f64; 3], min: f64, max: f64, floor: f64, ceil: f64) -> Option<[f64; 3]> {
    let (h_min, h_max) = rules::away_band(min, max, floor, ceil);
    spots::pick_for(w, e, Pick::LandAway { min_h: h_min, max_h: h_max, v: 7, from: at })
}
/// The stab of the window: the targets the reach allows, once in their contact cooldown; each one gets its damage.
/// A target that passes the `dismount` condition gets off its mount (`stab_dismount`).
fn stab(w: &mut World, e: Entity, s: &mut Spear, k: &Kinetic, who: Who, ticks_used: i32) {
    let Some(t) = info(w, who) else { return };
    let Some(me) = info(w, Who::Entity(e)) else { return };
    let eye = Vec3::new(me.pos.x, me.eye, me.pos.z);
    let look = rules::look_dir(eye, t.pos, t.eye);
    let vel = s.moved;
    // The attacker's speed per second along its view; the target's speed is not read here (zero).
    let attacker_speed = look.dot(vel.scale(20.0));
    let reach = (REACH.min * REACH.mob_factor, REACH.max * REACH.mob_factor, REACH.hitbox_margin);
    let hits = hits_in_reach(&[(0, t.bounds)], eye, look, vel, reach, |_, _| None);
    if hits.is_empty() {
        return;
    }
    let now = w.resource::<Clock>().0;
    let recent = s.stabbed.iter().any(|&(w2, at)| w2 == who && now.saturating_sub(at) < k.contact_cooldown_ticks as u64);
    if recent {
        return;
    }
    match s.stabbed.iter_mut().find(|(w2, _)| *w2 == who) {
        Some(entry) => entry.1 = now,
        None => s.stabbed.push((who, now)),
    }
    let relative = attacker_speed.max(0.0);
    if k.damage.test(ticks_used, attacker_speed, relative, 0.2) {
        let base = w.get::<AttackDamage>(e).map_or(0.0, |a| a.0);
        let amount = base + (relative * f64::from(k.damage_multiplier)).floor() as f32;
        strike(w, e, who, amount);
    }
    if k.knockback.test(ticks_used, attacker_speed, relative, 0.2) {
        // `stabAttack`: a push of 0.4, then the attacker's own `attack_knockback` push.
        crate::hostile::push_along_facing(w, e, who, 0.4);
        let knockback = w.get::<crate::cow::SpeciesId>(e).map_or(0.0, |s| crate::attr_mod::read(w, e, "attack_knockback", w.resource::<crate::cow::SpeciesTable>().0[s.0 as usize].attack_knockback));
        crate::hostile::extra_knockback(w, e, who, knockback);
    }
    if k.dismount.test(ticks_used, attacker_speed, relative, 0.2) {
        crate::stab_dismount::stop_riding(w, who);
    }
}
