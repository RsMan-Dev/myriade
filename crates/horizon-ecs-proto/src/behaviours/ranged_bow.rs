//! `RangedBowAttackGoal` (the skeleton, with the bow in hand supposed): chase the target until it is within `radius` and has
//! been seen for 20 ticks, then stand and strafe, drawing the bow for 20 ticks and loosing an arrow, every `interval` ticks
//! at least. It reads the target of the targeting service and shoots through the projectile capability: the arrow is an
//! entity of its own (`projectile.rs`) that hurts through the `DamageQueue` with the shooter as the source.
//! What wakes it: a target acquired (the watcher of the service); what ticks it: its marker, while it runs.
use super::*;
use crate::projectile::{launch, Damage, Flight};
use crate::targeting::{self, Target, Who};
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed modifier of the walk to the target, the least ticks between two shots, the shooting distance.
    pub speed: f64,
    pub interval: i32,
    pub radius: f32,
    /// The difficulty id (0 peaceful to 3 hard): the arrow's spread (`14 - 4 * id`) and its damage's draw.
    pub difficulty: i32,
    /// The effect its arrows put on the creature they hit (a stray's slowness, a bogged's poison, a parched's weakness): (kind, level, ticks).
    pub effect: Option<(crate::effects::Kind, i32, i32)>,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, interval: 20, radius: 15.0, difficulty: 2, effect: None };
}
/// The goal's state (`RangedBowAttackGoal`'s fields) and the bow in use (`using`: ticks the bow has been drawn).
#[derive(Component, Clone, Copy, Debug)]
pub struct Bow {
    p: Params,
    pub slot: u8,
    attack_time: i32,
    see_time: i32,
    strafe_clockwise: bool,
    strafe_back: bool,
    strafe_time: i32,
    using: i32,
    aim: (i32, i32),
    walkable: Option<((i32, i32, i32), bool, u64)>,
}
/// The goal runs.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Bowing;

pub static MODULE: Module = Module {
    name: "ranged_bow",
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
        if let Some(b) = w.get::<Bow>(e) {
            let slot = b.slot;
            w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
        }
    });
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Bow { p: *params::<Params>(p), slot: inst.slot, attack_time: -1, see_time: 0, strafe_clockwise: false, strafe_back: false, strafe_time: -1, using: 0, aim: (i32::MIN, 0), walkable: None });
}
/// The tick of the module: its system, run by `arbitrate` while an animal carries it (no system in the schedule).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, bow_run);
}
/// `canUse`: a target (and the bow in hand, supposed).
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Target>(e).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Bowing);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Bowing>();
    if let Some(mut b) = w.get_mut::<Bow>(e) {
        b.see_time = 0;
        b.attack_time = -1;
        b.using = 0;
    }
    stop_nav(w, e);
}
fn next_float(r: &mut Rng) -> f32 {
    (r.next_u32() >> 8) as f32 / (1u32 << 24) as f32
}
/// `BowItem.getPowerForTime`.
fn bow_power(ticks: i32) -> f32 {
    let t = ticks as f32 / 20.0;
    ((t * t + t * 2.0) / 3.0).min(1.0)
}
/// `RangedBowAttackGoal.tick`, every tick of the goal.
#[allow(clippy::type_complexity)]
fn bow_run(
    tg: targeting::Targets,
    level: Res<Level>,
    clock: Res<Clock>,
    mut q: Query<(Aim, &mut Bow, &mut Drive, &mut Yaw, &Body, Option<&Target>, &mut Rng), With<Bowing>>,
    mut reqs: ResMut<Requests>,
    mut forced: ResMut<Forced>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, following, asked), mut b, mut drive, mut yaw, body, target, mut rng) in &mut q {
        // `canContinueToUse`: a target, or a walk still going.
        let Some(t) = target else {
            if !following && !asked {
                reqs.0.push(Req::End(e, b.slot));
            }
            continue;
        };
        let Some(i) = tg.info(t.who) else { continue };
        let d = d2(pos.0, i.pos);
        let sees = t.seen;
        if sees != (b.see_time > 0) {
            b.see_time = 0;
        }
        b.see_time += if sees { 1 } else { -1 };
        let radius_sqr = b.p.radius * b.p.radius;
        if d <= f64::from(radius_sqr) && b.see_time >= 20 {
            if following || asked || b.aim.0 != i32::MIN {
                b.aim = (i32::MIN, 0);
                cmd.queue(move |w: &mut World| stop_nav(w, e));
            }
            b.strafe_time += 1;
        } else {
            let a = (i.pos.x.floor() as i32, i.pos.z.floor() as i32);
            if a != b.aim || (!following && !asked) {
                b.aim = a;
                go_to(&mut cmd, e, i.pos.x, i.pos.z, b.p.speed, 5);
            }
            b.strafe_time = -1;
        }
        if b.strafe_time >= 20 {
            // The state a `Mob` has (the tests that compare with its draws), else the two draws of 0.3.
            match forced.strafes.get(&e) {
                Some(&(cw, back)) => (b.strafe_clockwise, b.strafe_back) = (cw, back),
                None => {
                    b.strafe_clockwise ^= next_float(&mut rng) < 0.3;
                    b.strafe_back ^= next_float(&mut rng) < 0.3;
                }
            }
            b.strafe_time = 0;
        }
        let at = [i.pos.x, i.eye, i.pos.z];
        if b.strafe_time > -1 {
            if d > f64::from(radius_sqr * 0.75) {
                b.strafe_back = false;
            } else if d < f64::from(radius_sqr * 0.25) {
                b.strafe_back = true;
            }
            let (mut walkable, now) = (b.walkable, clock.of(e));
            strafe_drive(&level, &Pos(pos.0), &yaw, body, &mut drive, if b.strafe_back { -0.5 } else { 0.5 }, if b.strafe_clockwise { 0.5 } else { -0.5 }, &mut walkable, now);
            b.walkable = walkable;
            // `Mob.turn_to`: the body turns to the target at 30 degrees per tick.
            let y_rot = horizon_core::geometry::trig::atan2_degrees(at[2] - pos.0.z, at[0] - pos.0.x) - 90.0;
            yaw.0 += wrap_degrees(y_rot - yaw.0).clamp(-30.0, 30.0);
        } else {
            aim(&mut cmd, e, &mut look, looking, at, Some(30.0));
        }
        if b.using > 0 {
            if !sees && b.see_time < -60 {
                b.using = 0;
            } else if sees {
                let pull = b.using;
                if pull >= 20 {
                    b.using = 0;
                    let power = bow_power(pull);
                    let who = t.who;
                    cmd.queue(move |w: &mut World| shoot(w, e, who, power));
                    b.attack_time = b.p.interval;
                }
            }
            if b.using > 0 {
                b.using = b.using.saturating_add(1);
            }
        } else {
            b.attack_time -= 1;
            if b.attack_time <= 0 && b.see_time >= -60 {
                b.using = 1;
            }
        }
    }
}
fn wrap_degrees(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}
/// `AbstractSkeleton.performRangedAttack`: the arrow starts at the eyes less 0.1, aimed at a third of the target's height
/// and a fifth of the distance higher, at speed 1.6 with a spread of `14 - 4 * difficulty`; its base damage is
/// `power * 2 + triangle(difficulty * 0.11, 0.57425)`.
fn shoot(w: &mut World, e: Entity, who: Who, power: f32) {
    let Some(i) = targeting::info(w, who) else { return };
    let (Some(pos), Some(eye), Some(b)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<targeting::Eye>(e).map(|x| x.0), w.get::<Bow>(e).map(|b| b.p)) else { return };
    let start = Vec3::new(pos.x, pos.y + eye - f64::from(0.1_f32), pos.z);
    let height = i.bounds.max_position().y - i.bounds.min_position().y;
    let (xd, yd, zd) = (i.pos.x - pos.x, i.pos.y + height / 3.0 - start.y, i.pos.z - pos.z);
    let dist = (xd * xd + zd * zd).sqrt();
    let id = b.difficulty;
    let mut rng = w.get_mut::<Rng>(e).unwrap();
    let mut triangle = |mean: f64, spread: f64| mean + spread * (rng.next_f64() - rng.next_f64());
    let damage = f64::from(power) * 2.0 + triangle(f64::from(id) * 0.11, 0.574_25);
    let dir = Vec3::new(xd, yd + dist * f64::from(0.2_f32), zd).normalize();
    let spread = 0.017_227_5 * f64::from((14 - id * 4) as f32);
    let v = Vec3::new((dir.x + triangle(0.0, spread)) * 1.6, (dir.y + triangle(0.0, spread)) * 1.6, (dir.z + triangle(0.0, spread)) * 1.6);
    launch(w, Flight { damage: Damage::BySpeed(damage), hit: b.effect.map_or(crate::projectile::OnHit::Nothing, |(k, a, t)| crate::projectile::OnHit::Effect(k, a, t)), ..Flight::ARROW }, start, v, Some(Who::Entity(e)));
}
