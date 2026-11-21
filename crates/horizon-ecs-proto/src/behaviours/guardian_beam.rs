//! `GuardianAttackGoal`: the creature stops, looks at its target (90 degrees per tick) and charges a beam for `duration` ticks; when the
//! charge is over it hurts the target with indirect magic (`magic` plus `hard_bonus` on the hardest difficulty plus `elder_bonus`) and by a
//! blow of `damage`, then lets the target go. A target that leaves its sight ends the goal at once. The beam's visible state is the
//! component [`Beam`] (put at the start of the charge, removed when the goal ends): the clients read it, nothing else does.
//! Not done: the sounds, the entity event 21 (the attack sound loop of the clients), the beam's `getAttackAnimationScale` (a client
//! reading of `attack_time`).
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::hostile::PlayerState;
use crate::targeting::{self, Target, TargetWatchers, Who};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The charge, in ticks (`getAttackDuration`: 80, an elder's 60).
    pub duration: i32,
    /// The magic damage: base, and the additions on hard and for an elder.
    pub magic: f32,
    pub hard_bonus: f32,
    pub elder_bonus: f32,
    /// `Attributes.ATTACK_DAMAGE`, the blow that follows the magic.
    pub damage: f32,
    /// The goal goes on while the target is nearer than 3 blocks (an elder's); else it ends there.
    pub keeps_close: bool,
    /// The difficulty id (0 peaceful to 3 hard).
    pub difficulty: i32,
}
impl Params {
    pub const GUARDIAN: Self = Self { duration: 80, magic: 1.0, hard_bonus: 2.0, elder_bonus: 0.0, damage: 6.0, keeps_close: false, difficulty: 2 };
    pub const ELDER: Self = Self { duration: 60, elder_bonus: 2.0, damage: 8.0, keeps_close: true, ..Self::GUARDIAN };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct GuardianBeam {
    p: &'static Params,
    slot: u8,
    attack_time: i32,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Beaming;
/// The beam is charging (`Guardian.hasActiveAttackTarget`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Beam;

pub static MODULE: Module = Module {
    name: "guardian_beam",
    flags: MOVE | LOOK,
    resources,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Targets, Cap::Head],
    ..Module::DEFAULT
};
fn resources(w: &mut World) {
    w.resource_mut::<TargetWatchers>().0.push(|w, e| {
        if let Some(s) = w.get::<GuardianBeam>(e) {
            let slot = s.slot;
            w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
        }
    });
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(GuardianBeam { p: params::<Params>(p), slot: inst.slot, attack_time: 0 });
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, beam_run);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Target>(e).is_some_and(|t| targeting::info(w, t.who).is_some_and(|i| !i.exempt))
}
/// `start`: the walk stops, the look is on the target at once, the charge begins at -10.
fn start(w: &mut World, e: Entity, _: Inst) {
    w.get_mut::<GuardianBeam>(e).unwrap().attack_time = -10;
    stop_nav(w, e);
    w.entity_mut(e).insert(Beaming);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<(Beaming, Beam)>();
    targeting::clear_target(w, e);
}
#[allow(clippy::type_complexity)]
fn beam_run(tg: targeting::Targets, mut q: Query<(Aim, &mut GuardianBeam, Option<&Target>, Has<super::elder_curse::Silent>), With<Beaming>>, mut reqs: ResMut<Requests>, mut cmd: Commands) {
    for ((e, pos, mut look, looking, _, _), mut b, target, silent) in &mut q {
        let Some(t) = target else {
            reqs.0.push(Req::End(e, b.slot));
            continue;
        };
        let Some(i) = tg.info(t.who).filter(|i| !i.exempt) else {
            reqs.0.push(Req::End(e, b.slot));
            continue;
        };
        // `canContinueToUse`: a guardian leaves a target that comes within 3 blocks.
        if !b.p.keeps_close && d2(pos.0, i.pos) <= 9.0 {
            reqs.0.push(Req::End(e, b.slot));
            continue;
        }
        cmd.queue(move |w: &mut World| stop_nav(w, e));
        aim_clamped(&mut cmd, e, &mut look, looking, [i.pos.x, i.eye, i.pos.z], Some(90.0), Some(90.0));
        if !t.seen {
            reqs.0.push(Req::End(e, b.slot));
            continue;
        }
        b.attack_time += 1;
        if b.attack_time == 0 {
            cmd.entity(e).insert(Beam);
            // Entity event 21: the clients start the charge sound (the `Mob` sends it unless silent).
            if !silent {
                cmd.queue(move |w: &mut World| w.resource_mut::<Events>().host.entity_event(e, 21));
            }
        } else if b.attack_time >= b.p.duration {
            let p = *b.p;
            let who = t.who;
            cmd.queue(move |w: &mut World| strike(w, e, who, p));
            reqs.0.push(Req::End(e, b.slot));
        }
    }
}
/// The beam lands: indirect magic, then the guardian's blow.
fn strike(w: &mut World, e: Entity, who: Who, p: Params) {
    let Some(from) = w.get::<Pos>(e).map(|x| (x.0.x, x.0.z)) else { return };
    let magic = p.magic + if p.difficulty >= 3 { p.hard_bonus } else { 0.0 } + p.elder_bonus;
    match who {
        Who::Player(id) => {
            let tick = w.resource::<Clock>().0;
            w.resource_mut::<PlayerState>().hurt(tick, id, magic, from, Some(Who::Entity(e)));
        }
        Who::Entity(t) => w.resource_mut::<DamageQueue>().0.push(Hit { target: t, amount: magic, cause: Cause::Magic, damage: "indirect_magic", from: Some(from), by: Some(Who::Entity(e)), direct: false, power: None }),
    }
    crate::hostile::strike(w, e, who, p.damage);
}
