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
/// The tick of the next head turn of a hanging bat (`Bat.tick` draws `nextInt(200)` each resting tick; the count is drawn at the perch).
#[derive(Component, Clone, Copy, Debug)]
pub struct HeadTurn(pub u64);
/// Hangs at the end of this tick (after the move, as the `Mob` does).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Perching;

pub static MODULE: Module = Module { name: "bat_flutter", equip, systems, requires: &[Cap::Locomotion], ..Module::DEFAULT };
/// `Bat(EntityType, Level)`: `BatFlags` 0, not resting; it flies from its first AI step, with no target.
fn equip(w: &mut World, e: Entity, _: Inst, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert((Flutter::default(), Fluttering, Moving));
}
/// Hangs from the block above: at rest, asleep (`Bat.tick` of a resting bat), watched for a player within 4 blocks.
pub fn perch(w: &mut World, e: Entity) {
    let (Some(mut p), Some(b)) = (w.get::<Pos>(e).copied(), w.get::<Body>(e).copied()) else { return };
    p.0.y = p.0.y.floor() + 1.0 - f64::from(b.height);
    w.entity_mut(e).insert((p, Bounds(box_at(p.0, b.width, b.height)), Vel(Vec3::ZERO), Hanging, PlayerWatch::new(4.0))).remove::<(Perching, Fluttering, Waking, Moving)>();
    head_turn_at(w, e);
}
/// The head turns of a resting bat: `nextInt(200)` per tick until 0 (`Bat.tick`), drawn here once as the count of ticks, then `after` calls `turn_head`.
fn head_turn_at(w: &mut World, e: Entity) {
    let Some(mut r) = w.get_mut::<Rng>(e) else { return };
    let mut ticks = 1u64;
    while r.next_int(200) != 0 {
        ticks += 1;
    }
    drop(r);
    let at = crate::cow::now_of(w, e) + ticks;
    w.entity_mut(e).insert(HeadTurn(at));
    crate::cow::after(w, e, ticks as i32, turn_head);
}
/// `Bat.tick`'s `yHeadRot` draw (`nextInt(360)`), then the next count; a call for a bat that flies again or whose count moved on does nothing.
fn turn_head(w: &mut World, e: Entity) {
    let now = crate::cow::now_of(w, e);
    if w.get::<Hanging>(e).is_none() || !w.get::<HeadTurn>(e).is_some_and(|t| t.0 == now) {
        return;
    }
    let yaw = w.get_mut::<Rng>(e).map_or(0, |mut r| r.next_int(360)) as f32;
    if let Some(mut h) = w.get_mut::<crate::goals::Head>(e) {
        h.yaw = yaw;
    }
    head_turn_at(w, e);
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    // The decisions are the goal's (its slot's systems run while a bat that carries it lives); the steering is the flight's.
    s.add_systems((wake.before(decide), block_wake.after(crate::cow::block_changes).before(crate::block_events::drain_block_events), decide.in_set(slot), steer.after(decide).before(crate::locomotion::fly::travel), (hang, woke).after(crate::locomotion::fly::travel)));
}
fn conductor(level: &Level, p: [i32; 3]) -> bool {
    let mw = level.mw.0;
    mw.data.state(mw.world.block(p[0], p[1], p[2])).flags & myriade_engine_lib::mobs::data::REDSTONE_CONDUCTOR != 0
}
/// A player within 4 blocks or a block under it: the `bat.takeoff` level event unless it is silent (`Bat.customServerAiStep`), then it takes off.
fn wake_by(w: &mut World, e: Entity) {
    if w.get::<crate::behaviours::elder_curse::Silent>(e).is_none() {
        let p = w.get::<Pos>(e).map_or([0, 0, 0], |p| [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32]);
        w.resource_mut::<Events>().host.level_event(1025, p, 0);
    }
    take_off(w, e);
}
/// A player within 4 blocks, or a hit: the bat takes off.
pub fn take_off(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<(Hanging, PlayerWatch)>().insert((Waking, Moving));
}
fn wake(ev: Res<Events>, hanging: Query<(), With<Hanging>>, mut cmd: Commands) {
    for (e, _) in ev.player_entered.iter() {
        if hanging.contains(e) {
            cmd.queue(move |w: &mut World| wake_by(w, e));
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
/// A block changed under a hanging bat (`Bat.customServerAiStep`: the block above no longer conducts): it takes off.
fn block_wake(changed: Option<Res<crate::block_events::BlockEvents>>, level: Res<Level>, hanging: Query<(Entity, &Pos), With<Hanging>>, mut cmd: Commands) {
    let Some(changed) = changed else { return };
    for c in &changed.changed {
        for (e, p) in &hanging {
            let above = bat_flutter::above(p.0.x, p.0.y, p.0.z);
            if above == *c && !conductor(&level, above) {
                cmd.queue(move |w: &mut World| wake_by(w, e));
            }
        }
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

