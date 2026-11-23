//! Primed TNT (`PrimedTnt`): a body that falls and slides as `Entity.move` does (gravity 0.04, the air drag 0.98 on every axis, on the
//! ground x and z times 0.7 and y times -0.5), and a fuse that is a deadline of its own time (`after`): no tick while it burns. Asleep on
//! the ground it has no `Moving`. At its deadline it is removed and the host is asked the blast (`tnt_tick::explode`), credited to its owner.
//! The fluids push it by the currents through the generic `fluid_update`. Not ported: the soul sand's and honey's speed factor (the mob
//! world gives none to P), `tnt_explodes` (the host checks the rule before it applies the blast), the portal's flag.
//! NBT keys (`PrimedTnt`): `fuse`, `block_state`, `explosion_power` (kept when it is not 4.0), `owner` (an `EntityReference`).
use crate::cow::*;
use crate::rules::tnt::drag;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// `PrimedTnt`'s fuse at its start, its power and its box (`EntityType.TNT`: 0.98 wide and high).
pub const FUSE: i32 = 80;
pub const POWER: f32 = 4.0;
pub const SIZE: f32 = 0.98;
/// The NBT keys of `PrimedTnt` (`TAG_FUSE`, `TAG_BLOCK_STATE`, `TAG_EXPLOSION_POWER`, `owner`).
pub const TAG_FUSE: &str = "fuse";
pub const TAG_BLOCK_STATE: &str = "block_state";
pub const TAG_EXPLOSION_POWER: &str = "explosion_power";
pub const TAG_OWNER: &str = "owner";

/// A primed TNT: its own time at which it explodes, the power of the blast, who lit it (a player or an entity, credited by the blast), and the
/// fall it made in the air as the last tick there left it (the landing reads it: the move that lands resets `fall_distance`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Primed {
    pub fuse_at: u64,
    pub power: f32,
    pub owner: Option<Who>,
    pub fell: f64,
    /// Whether it went through a nether portal (`PrimedTnt.usedPortal`): its blast picks the portal-proof calculator.
    pub used_portal: bool,
}

/// Primes a TNT at `at` with velocity `vel`, its fuse `fuse` ticks long (`PrimedTnt`'s constructor; the random short fuse is the caller's).
pub fn spawn_primed(w: &mut World, at: Vec3, vel: Vec3, fuse: i32, power: f32) -> Entity {
    let body = Body { width: SIZE, height: SIZE, step_height: 0.0, speed: 0.0, fall_factor: 1.0, safe_fall: 3.0, fall_mult: 1.0, movement_efficiency: 0.0, lava_walker: false, object: true, orb: false, tnt: true };
    let e = w
        .spawn((Pos(at), Vel(vel), OwnTime, body, Gravity(0.04), Bounds(box_at(at, SIZE, SIZE)), Contact { on_ground: false, horizontal: false, fall_distance: 0.0, stuck: 0 }, FluidState::default(), Moving))
        .id();
    crate::nbt::mark_saved(w, e, crate::species::lists::TNT);
    w.entity_mut(e).insert(crate::cow::TypeName(crate::species::lists::TNT));
    let fuse_at = now_of(w, e) + fuse.max(0) as u64;
    w.entity_mut(e).insert(Primed { fuse_at, power, owner: None, fell: 0.0, used_portal: false });
    after(w, e, fuse, primed_due);
    crate::zone::settle(w, e);
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
    spawn_from_blast_by(w, block, rng, None)
}

/// `spawn_from_blast` for a blast with an owner: the TNT it primes keeps that owner, so the chained blast is credited to it too.
pub fn spawn_from_blast_by(w: &mut World, block: [i32; 3], rng: &mut Rng, owner: Option<Who>) -> Entity {
    let at = Vec3::new(f64::from(block[0]) + 0.5, f64::from(block[1]), f64::from(block[2]) + 0.5);
    let vel = push_of(rng);
    let fuse = rng.next_int(FUSE / 4) + FUSE / 8;
    let e = spawn_primed(w, at, vel, fuse, POWER);
    if let Some(mut p) = w.get_mut::<Primed>(e) {
        p.owner = owner;
    }
    e
}

/// The deadline of the fuse (on the wheel): a TNT still there is removed and its blast is asked (`tnt_tick::explode`).
pub(crate) fn primed_due(w: &mut World, e: Entity) {
    if w.get::<Primed>(e).is_some() {
        crate::tnt_tick::explode(w, e);
    }
}

/// The motion of a primed TNT (`travel_one` for `Body.tnt`): the gravity, the move, then the drag and the ground's friction. True when it
/// lies still on the ground with no speed: nothing moves it until a blast or a push wakes it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn travel_tnt(level: &Level, pos: &mut Pos, vel: &mut Vel, c: &mut Contact, body: &Body, gravity: Option<f64>, bounds: &mut Bounds, fall_damage: &mut f32) -> bool {
    let v = vel.0;
    if c.on_ground && v.x == 0.0 && v.y == 0.0 && v.z == 0.0 {
        return true;
    }
    if let Some(g) = gravity {
        vel.0.y -= g;
    }
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    vel.0 = drag(vel.0, c.on_ground);
    false
}
