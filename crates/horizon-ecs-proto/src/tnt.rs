//! Primed TNT (`PrimedTnt`): a body that falls and slides as `Entity.move` does (gravity 0.04, the air drag 0.98 on every axis, on the
//! ground x and z times 0.7 and y times -0.5), and a fuse that is a deadline of its own time (`after`): no tick while it burns. Asleep on
//! the ground it has no `Moving`. At its deadline it is removed and explodes with its power from 0.0625 of its height.
//! Not ported: the fluid push and the fall in water, the soul sand's slowdown, the owner's credit, `tnt_explodes`, the portal's flag.
use crate::cow::*;
use crate::explosion::explode_tnt;
use crate::projectile::Blast;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// `PrimedTnt`'s fuse at its start, its power and its box (`EntityType.TNT`: 0.98 wide and high).
pub const FUSE: i32 = 80;
pub const POWER: f32 = 4.0;
pub const SIZE: f32 = 0.98;

/// A primed TNT: its own time at which it explodes, and the power of the blast.
#[derive(Component, Clone, Copy, Debug)]
pub struct Primed {
    pub fuse_at: u64,
    pub power: f32,
}

/// Primes a TNT at `at` with velocity `vel`, its fuse `fuse` ticks long (`PrimedTnt`'s constructor; the random short fuse is the caller's).
pub fn spawn_primed(w: &mut World, at: Vec3, vel: Vec3, fuse: i32, power: f32) -> Entity {
    let body = Body { width: SIZE, height: SIZE, step_height: 0.0, gravity: 0.04, speed: 0.0, fall_factor: 1.0, lava_walker: false, object: true, orb: false, tnt: true };
    let e = w
        .spawn((Pos(at), Vel(vel), OwnTime, body, Bounds(box_at(at, SIZE, SIZE)), Contact { on_ground: false, horizontal: false, fall_distance: 0.0 }, FluidState::default(), Moving))
        .id();
    crate::nbt::mark_saved(w, e, crate::species::lists::TNT);
    w.entity_mut(e).insert(crate::cow::TypeName(crate::species::lists::TNT));
    let fuse_at = now_of(w, e) + fuse.max(0) as u64;
    w.entity_mut(e).insert(Primed { fuse_at, power });
    after(w, e, fuse, primed_due);
    e
}

/// The push of a TNT primed by a block (`PrimedTnt`'s constructor): a random direction `d` (`nextDouble * 2π`), 0.02 along it, 0.2 up.
pub fn push_of(rng: &mut Rng) -> Vec3 {
    let d = rng.next_f64() * f64::from(std::f32::consts::TAU);
    Vec3::new(-d.sin() * 0.02, f64::from(0.2_f32), -d.cos() * 0.02)
}

/// A block of TNT an explosion destroys (`TntBlock.wasExploded`): a TNT at its centre, pushed as its constructor pushes, with the fuse
/// `nextInt(fuse / 4) + fuse / 8` (10 to 29 ticks for 80).
pub fn spawn_from_blast(w: &mut World, block: [i32; 3], rng: &mut Rng) -> Entity {
    let at = Vec3::new(f64::from(block[0]) + 0.5, f64::from(block[1]), f64::from(block[2]) + 0.5);
    let vel = push_of(rng);
    let fuse = rng.next_int(FUSE / 4) + FUSE / 8;
    spawn_primed(w, at, vel, fuse, POWER)
}

/// The deadline: the TNT is removed and explodes at 0.0625 of its height (`PrimedTnt.explode`, `getY(0.0625)`).
fn primed_due(w: &mut World, e: Entity) {
    let (Some(p), Some(at)) = (w.get::<Primed>(e).copied(), w.get::<Pos>(e).map(|x| x.0)) else { return };
    let blast = Blast { at: Vec3::new(at.x, at.y + f64::from(SIZE) * 0.0625, at.z), radius: p.power, fire: false, owner: None, source: Some(e) };
    w.entity_mut(e).despawn();
    w.resource_mut::<crate::goals::Events>().exploded.push(blast);
    explode_tnt(w, blast);
}

/// The motion of a primed TNT (`travel_one` for `Body.tnt`): the gravity, the move, then the drag and the ground's friction. True when it
/// lies still on the ground with no speed: nothing moves it until a blast or a push wakes it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn travel_tnt(level: &Level, pos: &mut Pos, vel: &mut Vel, c: &mut Contact, body: &Body, bounds: &mut Bounds, fall_damage: &mut f32) -> bool {
    let v = vel.0;
    if c.on_ground && v.x == 0.0 && v.y == 0.0 && v.z == 0.0 {
        return true;
    }
    vel.0.y -= body.gravity;
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    let drag = f64::from(0.98_f32);
    let v = vel.0;
    vel.0 = Vec3::new(v.x * drag, v.y * drag, v.z * drag);
    if c.on_ground {
        vel.0 = Vec3::new(vel.0.x * 0.7, vel.0.y * -0.5, vel.0.z * 0.7);
    }
    false
}
