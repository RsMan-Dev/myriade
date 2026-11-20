//! A shooting goal that charges (`Ghast.GhastShootFireballGoal`): while it sees its target within `range` blocks the charge grows by
//! one a tick, and falls by one otherwise; at `at` the creature shoots (`power` is the explosion power of a large fireball) and the
//! charge goes to `after` (negative: the wait). The shot starts `forward` blocks along the creature's facing, at the middle of its body
//! raised 0.5, and goes along the line to the middle of the target seen from there; the speed is that of an `AbstractHurtingProjectile`
//! (0.1, its acceleration does the rest). No random draw. Not done: the warning cry, the charging flag, the targets (the predicate
//! "within 4 blocks in height" is not a module yet).
use super::*;
use crate::projectile::{launch, Flight};
use crate::targeting::{self, Target, TargetWatchers, Who};
use horizon_core::geometry::Vec3;
use crate::{cow::*, goals::*};
use horizon_core::geometry::trig::Mth;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub power: i32,
    pub at: i32,
    pub after: i32,
    pub range: f64,
    pub forward: f64,
}
impl Params {
    /// The ghast (`nether.rs:339-420`): explosion power 1, 20 ticks of charge, then 40 of wait, a target within 64 blocks.
    pub const GHAST: Self = Self { power: 1, at: 20, after: -40, range: 64.0, forward: 4.0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Charge {
    p: Params,
    slot: u8,
    charge: i32,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Charging;

pub static MODULE: Module = Module { name: "charged_shot", flags: 0, resources, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Body, Cap::Goals, Cap::Targets], ..Module::DEFAULT };
fn resources(w: &mut World) {
    w.resource_mut::<TargetWatchers>().0.push(on_target);
}
fn on_target(w: &mut World, e: Entity) {
    if let Some(s) = w.get::<Charge>(e) {
        let slot = s.slot;
        w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
    }
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Charge { p: *params::<Params>(p), slot: inst.slot, charge: 0 });
}
/// The tick of the module: its system, run by `arbitrate` while an animal carries it (no system in the schedule).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, charge_run);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Target>(e).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.get_mut::<Charge>(e).unwrap().charge = 0;
    w.entity_mut(e).insert(Charging);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Charging>();
}
fn charge_run(tg: targeting::Targets, mut cmd: Commands, mut q: Query<(Entity, &Pos, &mut Charge, Option<&Target>), With<Charging>>) {
    for (e, pos, mut c, target) in &mut q {
        let Some(t) = target else { continue };
        let Some(i) = tg.info(t.who) else { continue };
        if d2(pos.0, i.pos) < c.p.range * c.p.range && t.seen {
            c.charge += 1;
            if c.charge == c.p.at {
                let (who, p) = (t.who, c.p);
                cmd.queue(move |w: &mut World| shoot(w, e, who, p));
                c.charge = p.after;
            }
        } else if c.charge > 0 {
            c.charge -= 1;
        }
    }
}
fn shoot(w: &mut World, e: Entity, who: Who, p: Params) {
    let Some(i) = targeting::info(w, who) else { return };
    let (Some(pos), Some(h), Some(yaw)) = (w.get::<Pos>(e).map(|x| x.0), w.get::<Body>(e).map(|b| f64::from(b.height)), w.get::<Yaw>(e).map(|y| y.0)) else { return };
    let yaw = yaw.to_radians();
    let view = [-f64::from(yaw.msin()), f64::from(yaw.mcos())];
    let half = h * 0.5;
    let ty = (i.bounds.min_position().y + i.bounds.max_position().y) * 0.5;
    let dir = Vec3::new(i.pos.x - (pos.x + view[0] * p.forward), ty - (0.5 + pos.y + half), i.pos.z - (pos.z + view[1] * p.forward));
    let len = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt().max(1.0e-9);
    let v = Vec3::new(dir.x / len * 0.1, dir.y / len * 0.1, dir.z / len * 0.1);
    launch(w, Flight::large_fireball(p.power), Vec3::new(pos.x + view[0] * p.forward, pos.y + half + 0.5, pos.z + view[1] * p.forward), v, Some(Who::Entity(e)));
}
