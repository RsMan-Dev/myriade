//! `Bat.customServerAiStep`: a bat hangs under a conductor block until a player comes within 4 blocks or it is hurt; it flutters toward
//! a random spot nearby (one in 30 ticks a new one, or when it is near), and hangs again one tick in 100 under a conductor block.
//! The flight is the Air mode's (`Control::None`, free travel, `damp_y` 0.6); here are the decisions and the hanging.
//! A hanging bat is in NO query of a system of the frame or of this module (marker `Fluttering` absent, `Moving` absent): zero cost;
//! it is woken by EVENT (`PlayerWatch` radius 4, or hurt). Not ported: the head turning of a hanging bat, the wake when the block above
//! is removed (no block event yet), a player already inside the radius when the bat hangs.
use super::*;
use crate::rules::bat_flutter;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Self = Self {};
}
/// `Bat.targetPosition`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Flutter(pub Option<[i32; 3]>);
/// Flying about (the systems of the module visit these only).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Fluttering;
/// Hanging: at rest, asleep, in no query.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Hanging;
/// Woken: it falls and flies this tick, the AI steers it from the next (`Bat.customServerAiStep` returns on the tick it wakes).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Waking;
/// Hangs at the end of this tick (after the move, as the `Mob` does).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Perching;

pub static MODULE: Module = Module { name: "bat_flutter", equip, systems, requires: &[Cap::Locomotion], ..Module::DEFAULT };
/// `Bat(EntityType, Level)`: a new bat hangs.
fn equip(w: &mut World, e: Entity, _: Inst, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Flutter::default());
    perch(w, e);
}
/// Hangs from the block above: at rest, asleep (`Bat.tick` of a resting bat), watched for a player within 4 blocks.
pub fn perch(w: &mut World, e: Entity) {
    let (Some(mut p), Some(b)) = (w.get::<Pos>(e).copied(), w.get::<Body>(e).copied()) else { return };
    p.0.y = p.0.y.floor() + 1.0 - f64::from(b.height);
    w.entity_mut(e).insert((p, Bounds(box_at(p.0, b.width, b.height)), Vel(Vec3::ZERO), Hanging, PlayerWatch::new(4.0))).remove::<(Perching, Fluttering, Waking, Moving)>();
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    // The decisions are the goal's (its slot's systems run while a bat that carries it lives); the steering is the flight's.
    s.add_systems((wake.before(decide), decide.in_set(slot), steer.after(decide).before(crate::locomotion::fly::travel), (hang, woke).after(crate::locomotion::fly::travel)));
}
fn conductor(level: &Level, p: [i32; 3]) -> bool {
    let mw = level.mw.0;
    mw.data.state(mw.world.block(p[0], p[1], p[2])).flags & horizon_client_lib::mobs::data::REDSTONE_CONDUCTOR != 0
}
/// A player within 4 blocks, or a hit: the bat takes off.
pub fn take_off(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<(Hanging, PlayerWatch)>().insert((Waking, Moving));
}
fn wake(ev: Res<Events>, hanging: Query<(), With<Hanging>>, mut cmd: Commands) {
    for (e, _) in ev.player_entered.iter() {
        if hanging.contains(e) {
            cmd.queue(move |w: &mut World| take_off(w, e));
        }
    }
    for e in ev.hurt.iter() {
        if hanging.contains(e) {
            cmd.queue(move |w: &mut World| take_off(w, e));
        }
    }
}
/// The draws: a new spot, and the wish to hang.
#[allow(clippy::type_complexity)]
fn decide(level: Res<Level>, mut q: Query<(Entity, &Pos, &mut Flutter, &mut Rng), (With<Fluttering>, With<GoalSet>)>, mut cmd: Commands) {
    let mw = level.mw.0;
    for (e, p, mut f, mut r) in &mut q {
        let pos = p.0;
        if let Some(t) = f.0 && (!mw.facts.is_air(mw.world.block(t[0], t[1], t[2])) || t[1] <= mw.world.bottom_y()) {
            f.0 = None;
        }
        let near = |t: [i32; 3]| bat_flutter::is_near(t, pos.x, pos.y, pos.z);
        if f.0.is_none() || r.next_int(30) == 0 || f.0.is_some_and(near) {
            let o = |b: i32| f64::from(r.next_int(b));
            f.0 = Some(bat_flutter::spot(pos.x, pos.y, pos.z, o));
        }
        let above = bat_flutter::above(pos.x, pos.y, pos.z);
        if r.next_int(100) == 0 && conductor(&level, above) {
            cmd.entity(e).insert(Perching);
        }
    }
}
/// The velocity toward the spot (a tenth of the gap to 0.5 / 0.7 / 0.5 of speed), the yaw along it. (The forward input 0.5 the `Mob` sets is zeroed by its move control, which has no operation: no push.)
fn steer(mut q: Query<(&Flutter, &mut Vel, &mut Yaw, &Pos), With<Fluttering>>) {
    for (f, mut v, mut yaw, p) in &mut q {
        let Some(t) = f.0 else { continue };
        let (dx, dy, dz) = (f64::from(t[0]) + 0.5 - p.0.x, f64::from(t[1]) + 0.1 - p.0.y, f64::from(t[2]) + 0.5 - p.0.z);
        // `LivingEntity.aiStep` drops the tiny motions BEFORE the step of the AI.
        crate::locomotion::fly::drop_tiny_motion(&mut v);
        v.0 = bat_flutter::steer(v.0, dx, dy, dz);
        yaw.0 += crate::cow::wrap(horizon_core::geometry::trig::atan2_degrees(v.0.z, v.0.x) - 90.0 - yaw.0);
    }
}
/// After the move: a bat that woke this tick is steered from the next.
fn woke(q: Query<Entity, With<Waking>>, mut cmd: Commands) {
    for e in &q {
        cmd.entity(e).remove::<Waking>().insert(Fluttering);
    }
}
/// After the move: the bat that decided to hang stops and hangs from the block above (`Bat.tick`).
fn hang(q: Query<Entity, With<Perching>>, mut cmd: Commands) {
    for e in &q {
        cmd.queue(move |w: &mut World| perch(w, e));
    }
}

/// `Bat.checkFallDamage` does nothing: a bat never takes fall damage.
pub static NO_FALL: crate::rules::Rule = crate::rules::Rule { name: "bat_no_fall", equip: |_, _, _| {}, attrs: |_| crate::rules::Attrs { fall_factor: None, immune: IMMUNE_FALL }, interact: crate::rules::no_interact };
