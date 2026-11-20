//! `Raider.HoldGroundAttackGoal`: a patrolling raider (not in a raid) that has a target without having been hurt by a player stands still
//! and shouts: the raiders of its patrol within 8 blocks take the target (not aggressive). While the target is farther than the radius it only looks at
//! it (and a 1-in-50 draw is its voice: the sound is not done); closer, it turns aggressive. On the end its neighbours are shouted at once more, aggressive.
//! Move and look flags. Woken when the raider gets a target.
use super::*;
use crate::raids::{Aggressive, Patrol, Raider};
use crate::targeting::{self, Target, Who};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub radius: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { radius: 10.0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct HoldGround {
    p: Params,
    slot: u8,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Holding;

pub static MODULE: Module = Module { name: "hold_ground_attack", flags: MOVE | LOOK, resources, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Targets, Cap::Locomotion, Cap::Head], ..Module::DEFAULT };
fn resources(w: &mut World) {
    w.resource_mut::<targeting::TargetWatchers>().0.push(|w, e| {
        if let Some(h) = w.get::<HoldGround>(e) {
            let slot = h.slot;
            w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
        }
    });
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(HoldGround { p: *params::<Params>(p), slot: inst.slot });
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Raider>(e).is_some_and(|r| r.raid.is_none())
        && w.get::<Patrol>(e).is_some_and(|p| p.patrolling)
        && w.get::<Target>(e).is_some()
        && !w.get::<Aggressive>(e).is_some()
        && !w.get::<LastHurtBy>(e).is_some_and(|h| matches!(h.who, Who::Player(_)))
}
/// The raiders within 8 blocks (`shoutTargeting`), nearest first.
fn shout_range(w: &World, e: Entity) -> Vec<Entity> {
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return Vec::new() };
    let mut v = Vec::new();
    w.resource::<Index>().for_radius(p.x, p.z, 8.0, |o| v.push(o));
    let mut near: Vec<(f64, Entity)> = v
        .into_iter()
        .filter(|&o| o != e && w.get::<Raider>(o).is_some() && w.get::<Health>(o).is_some_and(|h| h.0 > 0.0))
        .filter_map(|o| {
            let q = w.get::<Pos>(o)?.0;
            let d = (q.x - p.x).powi(2) + (q.y - p.y).powi(2) + (q.z - p.z).powi(2);
            (d <= 64.0).then_some((d, o))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.into_iter().map(|(_, o)| o).collect()
}
fn shout(w: &mut World, e: Entity, aggressive: bool) {
    let Some(t) = w.get::<Target>(e).copied() else { return };
    for o in shout_range(w, e) {
        targeting::set_target(w, o, Target::new(t.who, t.memory, u8::MAX));
        if aggressive {
            w.entity_mut(o).insert(Aggressive);
        }
    }
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Holding);
    stop_nav(w, e);
    shout(w, e, false);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Holding>();
    if w.get::<Target>(e).is_some() {
        shout(w, e, true);
        w.entity_mut(e).insert(Aggressive);
    }
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, run);
}
fn run(tg: targeting::Targets, mut q: Query<(Aim, &HoldGround, Option<&Target>, &mut Rng), With<Holding>>, mut cmd: Commands) {
    for ((e, pos, mut look, looking, ..), h, t, mut rng) in &mut q {
        let Some(i) = t.and_then(|t| tg.info(t.who)) else { continue };
        let d = (i.pos.x - pos.0.x).powi(2) + (i.pos.y - pos.0.y).powi(2) + (i.pos.z - pos.0.z).powi(2);
        if d > h.p.radius * h.p.radius {
            aim(&mut cmd, e, &mut look, looking, [i.pos.x, i.eye, i.pos.z], None);
            let _voice = rng.next_int(50) == 0;
        } else {
            cmd.entity(e).insert(Aggressive);
        }
    }
}
