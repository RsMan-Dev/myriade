//! A shooting goal with a rate of fire in bursts (`Blaze.BlazeAttackGoal`): the target seen and within the follow range, the creature
//! looks at it, charges `charge` ticks, shoots `shots` times `gap` ticks apart, then rests `rest` ticks. The projectile is `flight`
//! (parameters, no species name): it leaves the middle of the body raised `up`, along the line to the middle of the target with a
//! triangular spread that grows with the distance, at the speed 0.1 of an `AbstractHurtingProjectile` (its own acceleration does the rest).
//! Not done: the melee under 2 blocks (`doHurtTarget` every 20 ticks), the walk toward a target it cannot see, the sounds and the flag.
use super::*;
use crate::projectile::{launch, Flight};
use crate::targeting::{self, Target, TargetWatchers, Who};
use horizon_core::geometry::Vec3;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub flight: Flight,
    pub charge: i32,
    pub shots: i32,
    pub gap: i32,
    pub rest: i32,
    /// The spread of the aim: `spread * sqrt(sqrt(d2)) * 0.5` on each horizontal axis.
    pub spread: f64,
    pub up: f64,
}
impl Params {
    /// The blaze (`nether.rs:44-120`): 3 small fireballs, charge 60, gap 6, rest 100.
    pub const BLAZE: Self = Self { flight: Flight::SMALL_FIREBALL, charge: 60, shots: 3, gap: 6, rest: 100, spread: 2.297, up: 0.5 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Burst {
    p: Params,
    slot: u8,
    step: i32,
    time: i32,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Bursting;

pub static MODULE: Module = Module {
    name: "burst_shot",
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
    w.resource_mut::<TargetWatchers>().0.push(on_target);
}
fn on_target(w: &mut World, e: Entity) {
    if let Some(s) = w.get::<Burst>(e) {
        let slot = s.slot;
        w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
    }
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Burst { p: *params::<Params>(p), slot: inst.slot, step: 0, time: 0 });
}
/// The tick of the module: its system, run by `arbitrate` while an animal carries it (no system in the schedule).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, burst_run);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Target>(e).is_some_and(|t| targeting::info(w, t.who).is_some_and(|i| !i.exempt))
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.get_mut::<Burst>(e).unwrap().step = 0;
    w.entity_mut(e).insert(Bursting);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Bursting>();
}
#[allow(clippy::type_complexity)]
fn burst_run(tg: targeting::Targets, mut cmd: Commands, mut reqs: ResMut<Requests>, mut q: Query<(crate::goals::Aim, &mut Burst, Option<&Target>, &NavParams), With<Bursting>>) {
    for ((e, pos, mut look, looking, _, _), mut b, target, nav) in &mut q {
        b.time -= 1;
        let Some(t) = target else {
            reqs.0.push(Req::End(e, b.slot));
            continue;
        };
        let Some(i) = tg.info(t.who).filter(|i| !i.exempt) else {
            reqs.0.push(Req::End(e, b.slot));
            continue;
        };
        let d = d2(pos.0, i.pos);
        let follow = f64::from(nav.follow_range);
        if d >= 4.0 && d < follow * follow && t.seen {
            if b.time <= 0 {
                b.step += 1;
                if b.step == 1 {
                    b.time = b.p.charge;
                } else if b.step <= b.p.shots + 1 {
                    b.time = b.p.gap;
                } else {
                    b.time = b.p.rest;
                    b.step = 0;
                }
                if b.step > 1 {
                    let p = b.p;
                    let who = t.who;
                    cmd.queue(move |w: &mut World| shoot(w, e, who, p));
                }
            }
            aim_clamped(&mut cmd, e, &mut look, looking, [i.pos.x, i.eye, i.pos.z], Some(10.0), Some(10.0));
        }
    }
}
/// `BlazeAttackGoal`'s shot: from the middle of the body raised by `up`, toward the middle of the target.
fn shoot(w: &mut World, e: Entity, who: Who, p: Params) {
    let Some(i) = targeting::info(w, who) else { return };
    let (Some(pos), Some(h)) = (w.get::<Pos>(e).map(|x| x.0), w.get::<Body>(e).map(|b| f64::from(b.height))) else { return };
    let half = h * 0.5;
    let (xd, zd) = (i.pos.x - pos.x, i.pos.z - pos.z);
    let yd = (i.bounds.min_position().y + i.bounds.max_position().y) * 0.5 - (pos.y + half);
    let sqd = d2(pos, i.pos).sqrt().sqrt() * 0.5;
    let dev = p.spread * sqd;
    let mut rng = w.get_mut::<Rng>(e).unwrap();
    let mut triangle = |mode: f64| mode + dev * (rng.next_f64() - rng.next_f64());
    let dir = Vec3::new(triangle(xd), yd, triangle(zd));
    let len = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt().max(1.0e-9);
    let v = Vec3::new(dir.x / len * 0.1, dir.y / len * 0.1, dir.z / len * 0.1);
    launch(w, p.flight, Vec3::new(pos.x, pos.y + half + p.up, pos.z), v, Some(Who::Entity(e)));
}
