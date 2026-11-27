//! `NearestAttackableTargetGoal`: the nearest creature or player of a described set, within the follow range, that the
//! attacker can see (`TargetingConditions.forCombat`). The set is parameters: players, entity types, an entity-type tag,
//! a named predicate (baby, adult); an animal can have several instances (the zombie: player, villagers, golem, baby
//! turtle), each with its priority. No flag but TARGET: a lower number replaces the instance that holds the target.
//! What wakes an instance: a deadline of the wheel, drawn like the selector's 1-in-`interval` ask every other tick
//! (armed when the animal enters the active zone, when a refused ask is over, and when its target ends); nothing else.
//! The candidates come from the spatial index, only when the ask comes up.
use super::*;
use crate::rules::nearest_target::{arm_wait, dist2, heal_chance, in_box, in_sphere, INVISIBLE, reach_range, reduced, watch_polled, watch_radius};
use crate::brain::behaviours::start_attacking::{TargetAnswer, TargetWatched};
use crate::targeting::{self, FollowRange, Target, Who};
use horizon_core::geometry::Vec3;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The nearest player is a candidate.
    pub players: bool,
    /// The creatures that are candidates: types by name, and/or an entity-type tag.
    pub types: &'static [&'static str],
    pub tag: Option<&'static str>,
    /// A condition on the creature: `"baby"`, `"baby_on_land"` or `"adult"` (named, resolved at creation).
    pub predicate: Option<&'static str>,
    /// `TargetGoal.mustSee`, `unseenMemoryTicks`, and the follow distance as a fraction of the follow range.
    pub must_see: bool,
    pub unseen_memory: i32,
    pub follow_scale: f64,
    /// `reducedTickDelay(interval)`: one chance in this many (halved) per selector ask.
    pub interval: i32,
    /// Creatures nearer than this squared distance are not picked (`GuardianAttackSelector`).
    pub beyond_sqr: Option<f64>,
    /// The owner (`tame::Owner`) is never picked.
    pub spare_owner: bool,
    /// Every creature is a candidate (`Prey::Filter(!(A | B))`), but the species of `except` (names), which no candidate ever is.
    pub any: bool,
    pub except: &'static [&'static str],
    /// The monsters (type category `monster`: the snow golem's `Prey::Filter(hostile)`) are candidates.
    pub hostile: bool,
    /// The distance of the search in blocks, instead of the follow range times `follow_scale`.
    pub range: Option<f64>,
    /// `NearestHealableRaiderTargetGoal`: only while the raid is active, on a draw of one in two, and it starts the wait (`HealWait`) of 200 ticks.
    pub heal: bool,
    /// `NearestAttackableWitchTargetGoal`: not while the wait of the healing goal runs.
    pub after_heal: bool,

    /// A condition on the creature (`Prey::Filter`: the llama's untamed wolf), read for each candidate.
    pub candidate_when: Option<fn(&World, Entity) -> bool>,
    /// Candidates further than this in height from the attacker's feet are not picked (the slime's player target: `|dy| <= 4`).
    pub max_dy: Option<f64>,
    /// The player is a candidate only within this distance along the axis of the face the attacker is attached to (the shulker's `|d| <= 4.5`).
    pub attach_reach: Option<f64>,
    /// `ATTACK_TARGET_COOLDOWN` (`NautilusAi.findNearestValidAttackTarget`): a search with no cooldown sets one, drawn in these
    /// ticks, and then picks nothing on one ask in two; no search runs while it lasts.
    pub attack_cooldown: Option<(i32, i32)>,
    /// The first tests of the search (`findNearestValidAttackTarget`): false, no search and no cooldown.
    pub may_search: Option<fn(&World, Entity) -> bool>,
    /// `ANGRY_AT` (`setAngerTarget`): the attacker that hurt the animal is its target while it stays attackable in the water
    /// (`findNearestValidAttackTarget`'s first search), for 400 ticks after the hurt.
    pub anger_memory: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { players: false, types: &[], tag: None, predicate: None, must_see: true, unseen_memory: 60, follow_scale: 1.0, interval: 10, beyond_sqr: None, spare_owner: false, any: false, except: &[], hostile: false, range: None, heal: false, after_heal: false, candidate_when: None, max_dy: None, attach_reach: None, attack_cooldown: None, may_search: None, anger_memory: false };
    /// The nearest player (`NearestAttackableTargetGoal(Player)`).
    pub const PLAYER: Self = Self { players: true, ..Self::DEFAULT };
}
/// One instance: its parameters resolved once (the species it takes, as a mask by `SpeciesId`) and its state.
#[derive(Clone, Copy, Debug)]
pub struct Nearest {
    pub p: Params,
    pub slot: u8,
    mask: crate::species::SpeciesSet,
    baby: Option<bool>,
    /// `baby_on_land`: the candidate must also be out of the water (`baby_turtle_on_land`).
    land: bool,
    /// Its deadline is in the wheel.
    armed: bool,
    /// No creature of its species is alive: no deadline is in the wheel until one is born (`unpark`).
    parked: bool,
    pick: Option<Who>,
}
impl Nearest {
    /// An instance that can never find anything costs nothing: it is never armed.
    fn inert(&self) -> bool {
        !self.p.players && self.mask.is_empty()
    }
}
/// The instances of the animal, by `Inst::nth`.
#[derive(Component, Clone, Debug)]
pub struct NearestTargets(pub Vec<Nearest>);

/// `ATTACK_TARGET_COOLDOWN`: the date (game time) before which the search picks nothing. Set by the search or at the birth; absent when over.
/// The countdown of the zombie nautilus is the same date, with no tick of its own.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct AttackCooldown(pub u64);

/// `ANGRY_AT` (`setAngerTarget`): the attacker and the game time its memory expires (the hurt's time plus 400 ticks).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct AngerMemory {
    pub who: Who,
    pub until: u64,
}

/// The ticks left of the attack cooldown, None when it is over.
pub fn attack_cooldown_left(w: &World, e: Entity) -> Option<u64> {
    let now = now_of(w, e);
    w.get::<AttackCooldown>(e).map(|c| c.0).filter(|d| *d > now).map(|d| d - now)
}

/// Sets the ticks left of the attack cooldown (`None`: over, the component is taken away).
pub fn set_attack_cooldown_left(w: &mut World, e: Entity, left: Option<u64>) {
    match left {
        Some(t) => {
            let date = now_of(w, e) + t;
            w.entity_mut(e).insert(AttackCooldown(date));
        }
        None => {
            w.entity_mut(e).remove::<AttackCooldown>();
        }
    }
}

/// The birth draw of the cooldown (a `Store::Call` of the species draws, `NautilusAi.initMemories`), in ticks.
pub(crate) fn spawn_attack_cooldown(w: &mut World, e: Entity, v: u32) {
    set_attack_cooldown_left(w, e, Some(u64::from(v)));
}

pub static MODULE: Module = Module {
    name: "nearest_target",
    flags: TARGET,
    blocked: Some(blocked),
    equip,
    arm,
    fired,
    can_use,
    start,
    systems,
    stop,
    tick: Some(tick),
    requires: &[Cap::Targets],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    let mw = w.resource::<Level>().mw.0;
    let mut ids = Vec::new();
    for (i, sp) in crate::species::registry().species.iter().enumerate() {
        let named = p.types.contains(&sp.name) || p.tag.is_some_and(|t| mw.data.entity_has(t, sp.name));
        // A monster that is not an `Enemy` (the rule `not_enemy`, read from the fiche) is no prey of `hostile`.
        let hostile = p.hostile && crate::cow::is_monster(sp.name) && !sp.rules.iter().any(|r| r.rule.name == "not_enemy");
        if (named || p.any || hostile) && !p.except.contains(&sp.name) {
            ids.push(i);
        }
    }
    let mask = crate::species::SpeciesSet::of(ids);
    let baby = match p.predicate {
        Some("baby") => Some(true),
        Some("adult") => Some(false),
        Some("baby_on_land") => Some(true),
        Some(other) => panic!("unknown target predicate {other}"),
        None => None,
    };
    let n = Nearest { p, slot: inst.slot, mask, baby, land: p.predicate == Some("baby_on_land"), armed: false, parked: false, pick: None };
    match w.get_mut::<NearestTargets>(e) {
        Some(mut ts) => {
            debug_assert_eq!(ts.0.len(), inst.nth as usize);
            ts.0.push(n);
        }
        None => {
            w.entity_mut(e).insert(NearestTargets(vec![n]));
        }
    }
}
fn state(w: &World, e: Entity, inst: Inst) -> Option<Nearest> {
    w.get::<NearestTargets>(e)?.0.get(inst.nth as usize).copied()
}
fn edit(w: &mut World, e: Entity, inst: Inst, f: impl FnOnce(&mut Nearest)) {
    if let Some(mut ts) = w.get_mut::<NearestTargets>(e)
        && let Some(n) = ts.0.get_mut(inst.nth as usize)
    {
        f(n);
    }
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let Some(n) = state(w, e, inst) else { return };
    if n.armed || n.inert() || n.parked {
        return;
    }
    // An anger memory is read by every ask, whatever creature is alive: such an instance never parks.
    if !n.p.players && !n.p.anger_memory && w.get_resource::<crate::goals::Carriers>().is_some_and(|c| !n.mask.any_alive(&c.1.0)) {
        edit(w, e, inst, |n| n.parked = true);
        return;
    }
    // A player goal with no player in reach has nothing to find: it sleeps until a player comes within reach (`PlayerWatch`, `tick`).
    // (The `Mob` draws its wait and asks anyway; the ask fails with no player in reach, so only the draws of the stream differ.)
    if n.p.players && !player_in_reach(w, e, &n) {
        edit(w, e, inst, |n| n.parked = true);
        return;
    }
    let wait = with_rng(w, e, |r| arm_wait(|k| geometric(r, k), n.p.interval));
    crate::goals::arm(w, e, inst.slot, wait);
    edit(w, e, inst, |n| n.armed = true);
}
/// The reach of a player goal: the follow range scaled, as `can_use` reads it.
fn reach_of(w: &World, e: Entity, n: &Nearest) -> f64 {
    reach_range(n.p.range, w.get::<FollowRange>(e).map(|f| f.0), n.p.follow_scale)
}
/// Some player is within reach (the watch is laid or widened to cover it, so that one coming in wakes the goal).
fn player_in_reach(w: &mut World, e: Entity, n: &Nearest) -> bool {
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return true };
    let range = reach_of(w, e, n);
    // The watch sees 64 blocks at most: a longer reach is polled.
    if watch_polled(range) {
        return true;
    }
    let any = w.resource::<Online>().any_near(pos.x, pos.z, range, |p| in_sphere(p.x - pos.x, p.y - pos.y, p.z - pos.z, range));
    if any {
        return true;
    }
    let radius = watch_radius(range);
    match w.get_mut::<PlayerWatch>(e) {
        Some(mut pw) => pw.radius = pw.radius.max(radius),
        None => {
            w.entity_mut(e).insert((PlayerWatch::new(radius), ParkWatch));
        }
    }
    false
}
/// A player came within the watch of an animal whose player goal sleeps: the goal is armed again.
fn tick(w: &mut World) {
    // A target lost by another goal (`clear_target`, the guardian's beam): the instances that held it end, so their ask is armed again.
    let lost: Vec<Entity> = w.resource::<Events>().target_lost.iter().collect();
    for e in lost {
        let slots: Vec<u8> = w.get::<NearestTargets>(e).map_or(Vec::new(), |ts| ts.0.iter().map(|n| n.slot).collect());
        for s in slots {
            end_goal(w, e, s);
        }
    }
    if w.resource::<Events>().player_entered.is_empty() {
        return;
    }
    let es: Vec<Entity> = w.resource::<Events>().player_entered.iter().map(|(e, _)| e).collect();
    for e in es {
        let Some(ts) = w.get::<NearestTargets>(e) else { continue };
        let hits: Vec<u8> = ts.0.iter().enumerate().filter(|(_, n)| n.parked && n.p.players).map(|(i, _)| i as u8).collect();
        for nth in hits {
            let inst = Inst { slot: w.get::<NearestTargets>(e).unwrap().0[nth as usize].slot, nth };
            edit(w, e, inst, |n| n.parked = false);
            arm(w, e, inst);
        }
        release(w, e);
    }
}
/// The watch laid for the parked goals (and by them alone) goes once none is parked: an awake crowd keeps no list of the players around it.
#[derive(Component)]
#[component(storage = "SparseSet")]
struct ParkWatch;
fn release(w: &mut World, e: Entity) {
    if w.get::<ParkWatch>(e).is_some() && !w.get::<NearestTargets>(e).is_some_and(|ts| ts.0.iter().any(|n| n.parked && n.p.players)) {
        w.entity_mut(e).remove::<(PlayerWatch, ParkWatch)>();
    }
}
/// A creature of species `sp` was born where none lived: the instances parked for it are armed again.
pub fn unpark(w: &mut World, sp: u8) {
    let ids: Vec<Entity> = crate::queries::entities::<With<NearestTargets>>(w);
    for e in ids {
        let hits: Vec<(u8, u8)> = w.get::<NearestTargets>(e).map_or(Vec::new(), |ts| ts.0.iter().enumerate().filter(|(_, n)| n.parked && n.mask.contains(sp)).map(|(i, n)| (n.slot, i as u8)).collect());
        for (slot, nth) in hits {
            let inst = Inst { slot, nth };
            edit(w, e, inst, |n| n.parked = false);
            arm(w, e, inst);
        }
    }
}
/// A creature wakes: the instances parked for a species that was born while it slept (`unpark` does not see a sleeper) are armed again.
pub fn on_wake(w: &mut World, e: Entity) {
    let Some(ts) = w.get::<NearestTargets>(e) else { return };
    let hits: Vec<(u8, u8)> = ts.0.iter().enumerate().filter(|(_, n)| n.parked && !n.p.players).map(|(i, n)| (n.slot, i as u8)).collect();
    for (slot, nth) in hits {
        let inst = Inst { slot, nth };
        edit(w, e, inst, |n| n.parked = false);
        arm(w, e, inst);
    }
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(hear_hurt.in_set(slot));
}
/// The hurts of this step, for the animals that hold instances (the others cost nothing).
fn hear_hurt(hurt: Res<crate::hostile::HurtBy>, mut reqs: ResMut<Requests>, q: Query<(), With<NearestTargets>>) {
    for (victim, _) in hurt.0.iter() {
        if q.contains(*victim) {
            reqs.0.push(Req::Call(*victim, angered));
        }
    }
}
/// `setAngerTarget` (`AbstractNautilus.hurtServer`): the last attacker of the hurt is remembered for 400 ticks, and the instances
/// that keep the memory ask again.
fn angered(w: &mut World, e: Entity) {
    let Some(who) = w.resource::<crate::hostile::HurtBy>().0.iter().rev().find(|(v, _)| *v == e).map(|(_, a)| *a) else { return };
    let until = now_of(w, e) + 400;
    w.entity_mut(e).insert(AngerMemory { who, until });
    let hits: Vec<(u8, u8)> = w.get::<NearestTargets>(e).map_or(Vec::new(), |ts| ts.0.iter().enumerate().filter(|(_, n)| n.p.anger_memory).map(|(i, n)| (n.slot, i as u8)).collect());
    for (slot, nth) in hits {
        arm(w, e, Inst { slot, nth });
    }
}
/// `findNearestValidAttackTarget`'s first search: the `ANGRY_AT` attacker, while it is in the water and can be attacked.
fn angry_target(w: &World, e: Entity) -> Option<Who> {
    let a = *w.get::<AngerMemory>(e)?;
    if a.until <= now_of(w, e) {
        return None;
    }
    let wet = match a.who {
        Who::Player(id) => w.resource::<crate::hostile::PlayerState>().in_water(id),
        Who::Entity(c) => w.get::<FluidState>(c).is_some_and(|f| f.in_water),
    };
    (wet && targeting::info(w, a.who).is_some_and(|i| !i.exempt)).then_some(a.who)
}
fn rearm(w: &mut World, e: Entity, inst: Inst) {
    edit(w, e, inst, |n| n.armed = false);
    arm(w, e, inst);
}
/// A flag is taken by a goal of a higher priority: no ask now (the selector asks only what can replace), the next one is
/// a fresh deadline.
fn blocked(w: &mut World, e: Entity, inst: Inst) {
    rearm(w, e, inst);
}
/// The draw came up: ask. Only the stroll depends on `noActionTime` (`RandomStrollGoal.canUse`); the target goals ask whatever
/// the distance to a player, in a chunk that ticks.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    edit(w, e, inst, |n| n.armed = false);
    true
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(n) = state(w, e, inst) else { return false };
    if (n.p.after_heal || n.p.heal) && super::heal_raiders::waiting(w, e) {
        return false;
    }
    if n.p.heal && (!crate::raids::has_active_raid(w, e) || !with_rng(w, e, |r| heal_chance(|k| r.next_int(k)))) {
        return false;
    }
    if n.p.may_search.is_some_and(|f| !f(w, e)) {
        rearm(w, e, inst);
        return false;
    }
    if n.p.anger_memory && let Some(who) = angry_target(w, e) {
        edit(w, e, inst, |n| n.pick = Some(who));
        return true;
    }
    if let Some((lo, hi)) = n.p.attack_cooldown {
        if attack_cooldown_left(w, e).is_some() {
            rearm(w, e, inst);
            return false;
        }
        let left = with_rng(w, e, |r| lo + r.next_int(hi - lo + 1));
        set_attack_cooldown_left(w, e, Some(left as u64));
        // `nextFloat() < 0.5` picks nothing (the jar's `ifge` goes on at 0.5 and above): the top 24 bits of the draw against 2^23.
        if with_rng(w, e, |r| r.next_u32() >> 8 < 1 << 23) {
            rearm(w, e, inst);
            return false;
        }
    }
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let eye = w.get::<targeting::Eye>(e).map_or(1.0, |l| l.0);
    let range = reach_of(w, e, &n);
    let mw = w.resource::<Level>().mw.0;
    let from = Vec3::new(pos.x, pos.y + eye, pos.z);
    let owner = w.get::<crate::tame::Owner>(e).and_then(|o| o.0);
    let mut found: Option<Who> = None;
    if n.p.players {
        // `TargetingConditions.forCombat` always checks the line of sight, here for the candidates in range only.
        let ps = w.resource::<crate::hostile::PlayerState>();
        // No player has an effect (the usual): the list is not asked per candidate.
        let any_effect = w.get_resource::<crate::effects::PlayerEffects>().is_some_and(|p| !p.0.is_empty());
        let looker = w.get::<SpeciesId>(e).map(|s| s.0);
        let hook = w.get_resource::<targeting::LivingVisibility>().and_then(|h| h.0).map(targeting::LentVisibility::get);
        let looking = if hook.is_some() { crate::bridge::ids::id_of(w, e).unwrap_or(0) } else { 0 };
        let best = w.resource::<Online>().nearest_where(pos.x, pos.y, pos.z, range, |p, d| {
            let vis = ps.visibility(p.id) * targeting::head_factor(ps.helmet(p.id), looker);
            let vis = hook.map_or(vis, |f| f.visibility(p.id, looking, vis).max(0.0));
            let reach = if any_effect {
                let seen = if crate::effects::player_has(w, p.id, crate::effects::Kind::Invisibility) { INVISIBLE } else { 1.0 };
                targeting::sight_range(range, vis * seen)
            } else {
                targeting::sight_range(range, vis)
            };
            d <= reach * reach
                && n.p.beyond_sqr.is_none_or(|b| d > b)
                && !(n.p.spare_owner && owner == Some(Who::Player(p.id)))
                && n.p.max_dy.is_none_or(|m| (p.y - pos.y).abs() <= m)
                && n.p.attach_reach.is_none_or(|m| {
                    let face = w.get::<crate::behaviours::attached_face::Attached>(e).map_or(0, |a| a.face);
                    let d = [p.x - pos.x, p.y - pos.y, p.z - pos.z][crate::behaviours::homing_projectile::axis_of(face) as usize];
                    d.abs() <= m
                })
                && ps.alive(p.id)
                && !ps.creative(p.id)
                && !ps.spectator(p.id)
                && mw.line_of_sight_for(Some(pos.y), from, Vec3::new(p.x, p.y + 1.62, p.z))
        });
        found = best.map(Who::Player);
    } else if !n.mask.is_empty() && w.get_resource::<crate::goals::Carriers>().is_none_or(|c| n.mask.any_alive(&c.1.0)) {
        let mut cands: Vec<(f64, Entity, Vec3, f64)> = Vec::new();
        let looker = w.get::<SpeciesId>(e).map(|s| s.0);
        w.resource::<Index>().for_radius(pos.x, pos.z, range, |c| {
            if c == e {
                return;
            }
            let (Some(cp), Some(cs)) = (w.get::<Pos>(c), w.get::<SpeciesId>(c)) else { return };
            if !n.mask.contains(cs.0) || n.baby.is_some_and(|b| b != w.get::<GrowAt>(c).is_some()) || (n.land && w.get::<crate::cow::FluidState>(c).is_some_and(|f| f.in_water)) || n.p.candidate_when.is_some_and(|f| !f(w, c)) {
                return;
            }
            let (dx, dy, dz) = ((cp.0.x - from.x).abs(), (cp.0.y - from.y).abs(), (cp.0.z - from.z).abs());
            if in_box(dx, dy, dz, range) && n.p.max_dy.is_none_or(|m| (cp.0.y - pos.y).abs() <= m) {
                let d = dist2(dx, dy, dz);
                cands.push((d, c, cp.0, w.get::<targeting::Eye>(c).map_or(1.0, |l| l.0)));
            }
        });
        cands.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (_, c, cp, ceye) in cands {
            let feet = d2(pos, cp);
            let vis = targeting::creature_visibility_to(w, c, looker);
            if feet > targeting::sight_range(range, vis).powi(2) || n.p.beyond_sqr.is_some_and(|d| feet <= d) {
                continue;
            }
            if mw.line_of_sight_for(Some(pos.y), from, Vec3::new(cp.x, cp.y + ceye, cp.z)) {
                found = Some(Who::Entity(c));
                break;
            }
        }
    }
    match found {
        Some(who) => {
            edit(w, e, inst, |n| n.pick = Some(who));
            true
        }
        None => {
            rearm(w, e, inst);
            false
        }
    }
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(n) = state(w, e, inst) else { return };
    let Some(who) = n.pick else { return };
    if n.p.heal {
        super::heal_raiders::wait(w, e);
    }
    let mut t = Target::new(who, reduced(n.p.unseen_memory) as u16, inst.slot);
    (t.must_see, t.follow_scale) = (n.p.must_see, n.p.follow_scale);
    if w.get_resource::<TargetWatched>().is_some_and(|x| x.0) {
        w.entity_mut(e).insert(PendingTarget(t));
        w.resource_mut::<crate::goals::Events>().host.target_ask(e, Some(who));
        return;
    }
    targeting::set_target(w, e, t);
}

/// A target the host is asked about while a mod hears `living_change_target`: `resolve_pending` takes it on `Set`, drops it on `Keep`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct PendingTarget(pub Target);

/// The answer to the `TargetAsk` of a pending target: `Set(who)` takes the goal's target aimed at `who` (memory and slot kept); `Set(None)`
/// clears the target, as the old path's `setTarget(null)`; `Keep` leaves the target it had.
pub fn resolve_pending(w: &mut World, e: Entity, answer: TargetAnswer) {
    let Some(p) = w.get_entity_mut(e).ok().and_then(|mut m| m.take::<PendingTarget>()) else { return };
    match answer {
        TargetAnswer::Keep => {}
        TargetAnswer::Set(None) => targeting::clear_target(w, e),
        TargetAnswer::Set(Some(who)) => {
            if who == p.0.who || targeting::info(w, who).is_some() {
                let mut t = Target::new(who, p.0.memory, p.0.slot);
                (t.must_see, t.follow_scale) = (p.0.must_see, p.0.follow_scale);
                targeting::set_target(w, e, t);
            }
        }
    }
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Target>(e).is_some_and(|t| t.slot == inst.slot) {
        targeting::clear_target(w, e);
    }
    rearm(w, e, inst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    /// `max_dy` (the slime's `|dy| <= 4`): the nearer candidate, too far in height, is skipped for the farther one in range; without the
    /// limit (the witness) the nearer one is the pick.
    #[test]
    fn max_dy_skips_a_candidate_too_far_in_height() {
        if crate::cow::skip_unless_walker("nearest max_dy") {
            return;
        }
        let name = species().name;
        let types: &'static [&'static str] = Box::leak(vec![name].into_boxed_slice());
        for (max_dy, expect_near) in [(None, true), (Some(2.0), false)] {
            let p: &'static Params = Box::leak(Box::new(Params { types, must_see: false, max_dy, range: Some(16.0), ..Params::DEFAULT }));
            let mut h = Herd::with_slab_awake(30, false, false);
            let entries: &'static [Entry] = Box::leak(vec![Entry::new(&MODULE, 1, p)].into_boxed_slice());
            let mut sp = *species();
            sp.goals = Config { entries };
            let hunter = spawn_species(&mut h.world, &sp, 0.5, 0.5, 1);
            let mut quiet = sp;
            quiet.goals = Config { entries: &[] };
            let (near, far) = (spawn_species(&mut h.world, &quiet, 3.5, 0.5, 2), spawn_species(&mut h.world, &quiet, 8.5, 0.5, 3));
            for _ in 0..3 {
                h.tick();
            }
            let y = h.world.get::<Pos>(hunter).unwrap().0.y;
            h.world.get_mut::<Pos>(near).unwrap().0.y = y + 3.0;
            h.world.get_mut::<Pos>(far).unwrap().0.y = y + 1.0;
            assert!(can_use(&mut h.world, hunter, Inst { slot: 0, nth: 0 }), "a candidate is in range");
            let pick = state(&h.world, hunter, Inst { slot: 0, nth: 0 }).unwrap().pick;
            assert_eq!(pick, Some(Who::Entity(if expect_near { near } else { far })), "max_dy {max_dy:?}");
        }
    }
}
