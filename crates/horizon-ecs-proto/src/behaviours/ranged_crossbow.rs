//! `RangedCrossbowAttackGoal` over the bow's shot: the target near enough and seen (a pillager: within 8 blocks, seen for 5 ticks), the
//! crossbow is loaded for `charge` ticks (25), waits `20 + nextInt(20)` ticks charged, then shoots when it sees the target: the arrow of
//! `ranged_bow` (a third of the target's height, a fifth of the distance up, speed 1.6, spread `14 - 4 * difficulty`) with the damage of a
//! crossbow bolt (no draw). The walk is half speed while charged. The crossbow in hand is read from the entity's equipment when it has one. Not done: the item's `charged_projectiles`,
//! the multishot and piercing enchantments, the sounds. Held in a state of the goal: the four states of the `Mob`, `see_time`, `attack_delay`,
//! `path_delay`.
use super::*;
use crate::projectile::{launch, Flight};
use crate::targeting::{self, Target, Who};
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub radius: f64,
    /// `CrossbowItem.getChargeDuration` (25, 5 fewer per level of Quick Charge).
    pub charge: i32,
    pub difficulty: i32,
}
impl Params {
    pub const PILLAGER: Self = Self { speed: 1.0, radius: 8.0, charge: 25, difficulty: 2 };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Uncharged,
    Charging,
    Charged,
    Ready,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Crossbow {
    p: &'static Params,
    slot: u8,
    phase: Phase,
    using: i32,
    see_time: i32,
    attack_delay: i32,
    path_delay: i32,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Crossbowing;

pub static MODULE: Module = Module {
    name: "ranged_crossbow",
    flags: MOVE | LOOK,
    resources,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Targets, Cap::Locomotion, Cap::Head],
    ..Module::DEFAULT
};
fn resources(w: &mut World) {
    w.resource_mut::<targeting::TargetWatchers>().0.push(|w, e| {
        if let Some(b) = w.get::<Crossbow>(e) {
            let slot = b.slot;
            w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
        }
    });
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Crossbow { p: params::<Params>(p), slot: inst.slot, phase: Phase::Uncharged, using: 0, see_time: 0, attack_delay: 0, path_delay: 0 });
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, crossbow_run);
}
/// `holding_crossbow` (`illager.rs:491`): the main hand holds a crossbow. An entity with no equipment at all is supposed to hold one (no species
/// is given its spawn gear yet: `finalize.rs:1506` gives the pillager's crossbow), see `suivi/ecs-ecarts.md`.
fn holds(eq: Option<&Equipment>) -> bool {
    eq.is_none_or(|q| q.main == crate::goals::crossbow())
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    holds(w.get::<Equipment>(e)) && w.get::<Target>(e).is_some_and(|t| targeting::info(w, t.who).is_some_and(|i| !i.exempt))
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Crossbowing);
}
/// `stop`: the view and the load are forgotten (the crossbow is unloaded).
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Crossbowing>();
    if let Some(mut c) = w.get_mut::<Crossbow>(e) {
        c.see_time = 0;
        c.using = 0;
        c.phase = Phase::Uncharged;
    }
    targeting::clear_target(w, e);
    stop_nav(w, e);
}
#[allow(clippy::type_complexity)]
fn crossbow_run(
    tg: targeting::Targets,
    mut q: Query<(Aim, &mut Crossbow, Option<&Target>, &mut Rng, Option<&Equipment>), With<Crossbowing>>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, _following, _asked), mut c, target, mut rng, eq) in &mut q {
        if !holds(eq) {
            reqs.0.push(Req::End(e, c.slot));
            continue;
        }
        let Some(t) = target else {
            reqs.0.push(Req::End(e, c.slot));
            continue;
        };
        let Some(i) = tg.info(t.who).filter(|i| !i.exempt) else {
            reqs.0.push(Req::End(e, c.slot));
            continue;
        };
        let sees = t.seen;
        if sees != (c.see_time > 0) {
            c.see_time = 0;
        }
        c.see_time += if sees { 1 } else { -1 };
        let d = d2(pos.0, i.pos);
        let needs_to_move = (d > c.p.radius * c.p.radius || c.see_time < 5) && c.attack_delay == 0;
        if needs_to_move {
            c.path_delay -= 1;
            if c.path_delay <= 0 {
                let speed = if c.phase == Phase::Uncharged { c.p.speed } else { c.p.speed * 0.5 };
                let at = [i.pos.x, i.pos.y, i.pos.z];
                cmd.queue(move |w: &mut World| go_to_at(w, e, at, speed, 0));
                c.path_delay = 20 + rng.next_int(21);
            }
        } else {
            c.path_delay = 0;
            cmd.queue(move |w: &mut World| stop_nav(w, e));
        }
        aim_clamped(&mut cmd, e, &mut look, looking, [i.pos.x, i.eye, i.pos.z], Some(30.0), Some(30.0));
        match c.phase {
            Phase::Uncharged => {
                if !needs_to_move {
                    c.phase = Phase::Charging;
                    c.using = 0;
                    // The sound's pitch draws two floats in the `Mob`.
                    let _ = (nf(&mut rng), nf(&mut rng));
                }
            }
            Phase::Charging => {
                c.using += 1;
                if c.using >= c.p.charge {
                    c.using = 0;
                    c.phase = Phase::Charged;
                    c.attack_delay = 20 + rng.next_int(20);
                    let _ = nf(&mut rng);
                }
            }
            Phase::Charged => {
                c.attack_delay -= 1;
                if c.attack_delay == 0 {
                    c.phase = Phase::Ready;
                }
            }
            Phase::Ready => {
                if sees {
                    let (who, difficulty) = (t.who, c.p.difficulty);
                    cmd.queue(move |w: &mut World| shoot(w, e, who, difficulty));
                    c.phase = Phase::Uncharged;
                    let _ = nf(&mut rng);
                }
            }
        }
    }
}
fn nf(r: &mut Rng) -> f32 {
    (r.next_u32() >> 8) as f32 / (1u32 << 24) as f32
}
/// `CrossbowAttackMob.performCrossbowAttack` at power 1.6.
fn shoot(w: &mut World, e: Entity, who: Who, difficulty: i32) {
    let Some(i) = targeting::info(w, who) else { return };
    let (Some(pos), Some(eye)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<targeting::Eye>(e).map(|x| x.0)) else { return };
    let start = Vec3::new(pos.x, pos.y + eye - f64::from(0.1_f32), pos.z);
    let height = i.bounds.max_position().y - i.bounds.min_position().y;
    let (xd, zd) = (i.pos.x - pos.x, i.pos.z - pos.z);
    let dist = (xd * xd + zd * zd).sqrt();
    let yd = i.pos.y + height / 3.0 - start.y + dist * f64::from(0.2_f32);
    let dir = Vec3::new(xd, yd, zd).normalize();
    let spread = 0.017_227_5 * f64::from((14 - difficulty * 4) as f32);
    let mut rng = w.get_mut::<Rng>(e).unwrap();
    let mut triangle = |mean: f64, spread: f64| mean + spread * (rng.next_f64() - rng.next_f64());
    let v = Vec3::new((dir.x + triangle(0.0, spread)) * 1.6, (dir.y + triangle(0.0, spread)) * 1.6, (dir.z + triangle(0.0, spread)) * 1.6);
    launch(w, Flight::ARROW, start, v, Some(Who::Entity(e)));
}
