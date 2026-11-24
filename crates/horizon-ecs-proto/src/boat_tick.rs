//! The boats' own tick (`AbstractBoat.tick`): the status of the hull from the water and the ground under it, the float and the steering of
//! the rider, the paddles, the move through the blocks, the fall, the sleep of a boat at rest and the void. An awake boat carries `Moving`;
//! it sleeps when it lies still with no rider and is not visited again until it is woken. The body it moves is built here, not stored:
//! a `Body` component would hand the boat to the creatures' travel as well.
//! Not ported here: the current that pushes a boat, the fire, the bubble columns, lava, the passengers, the leash, the chest, the mods'
//! friction hook (the host keeps them), the splash (`host_out_boats`, lot 54).
use crate::boat::{Boat, HEIGHT, Status, Under, WIDTH, classify, enter_water, float, set_status};
use crate::behaviours::elder_curse::Silent;
use crate::boat_state::Hull;
use crate::no_gravity::NoGravity;
use crate::cow::*;
use crate::host_out_boats::BoatEvent;
use crate::pilot::PilotInput;
use bevy_ecs::prelude::*;
use myriade_engine_lib::blocks::Fluid;
use myriade_level::collisions::SceneCollisions;
use horizon_core::geometry::{Aabb, ShapeOp, Vec3, VoxelShape, trig};
use horizon_core::position::BlockPos;
use horizon_world::collision_context::CollisionContext;
use std::f32::consts::PI;

/// `AbstractBoat.getDefaultGravity` (javap: 0.04).
const GRAVITY: f64 = 0.04;
/// `Entity.checkBelowWorld`: a boat more than 64 blocks under the world's floor is discarded.
const BELOW: i32 = 64;
/// A boat whose speed is under this on every axis, with no rider and no fall, lies still.
const REST: f64 = 1.0e-5;

/// The rider holds the head seat: its keys steer the boat and its paddles row (`paddle_state`). Set and removed by `EcsWorld::boat_input`
/// and `boat_release`.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Steered;

/// The paddle angles of the two rows, while they row (`AbstractBoat.paddles`); absent when both are at rest.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Oars(pub [f32; 2]);

/// `AbstractBoat.TIME_TO_EJECT`: the ticks under water in a row after which the riders are ejected.
pub const TIME_TO_EJECT: u64 = 60;

/// The boat's own time when its run under water began (`AbstractBoat.outOfControlTicks` as a deadline): the counter is read only by the
/// ejection of the tick, so a wake is armed at the deadline instead of counting each tick. Sparse: only the boats under water carry it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct OutOfControl(pub u64);

/// The events of the boats since the host last drained them.
#[derive(Resource, Default)]
pub struct BoatEvents(pub Vec<BoatEvent>);

/// The events the boats made since the last call, in order.
pub fn take_boat_events(w: &mut World) -> Vec<BoatEvent> {
    w.get_resource_mut::<BoatEvents>().map(|mut e| std::mem::take(&mut e.0)).unwrap_or_default()
}

/// `Mth`'s sine table through `trig` (`controlBoat`): the push of the paddles along the yaw, widened to double.
pub fn paddle_push(yaw: f32, acceleration: f32) -> (f64, f64) {
    (
        f64::from(trig::sin(f64::from(-yaw * trig::DEG_TO_RAD)) * acceleration),
        f64::from(trig::cos(f64::from(yaw * trig::DEG_TO_RAD)) * acceleration),
    )
}

/// `AbstractBoat.controlBoat`: the keys turn the boat and push it along its yaw. The land friction is halved by a steered tick on land.
fn steer(b: &mut Boat, keys: [bool; 4], vel: &mut Vec3, yaw: &mut f32) {
    let [left, right, up, down] = keys;
    let mut acceleration = 0.0_f32;
    if left {
        b.delta_rotation -= 1.0;
    }
    if right {
        b.delta_rotation += 1.0;
    }
    if right != left && !up && !down {
        acceleration += 0.005;
    }
    *yaw += b.delta_rotation;
    if up {
        acceleration += 0.04;
    }
    if down {
        acceleration -= 0.005;
    }
    let (x, z) = paddle_push(*yaw, acceleration);
    vel.x += x;
    vel.z += z;
    b.paddling = [right && !left || up, left && !right || up];
    if b.status == Status::OnLand {
        b.land_friction /= 2.0;
    }
}

/// `AbstractBoat.isUnderwater`: `FlowingWater` at the first water over the hull's top that is no source, `Still` when every such water is
/// a source, `Dry` when the top is in the air.
fn hull_under(level: &Level, bounds: Aabb) -> Under {
    let mw = level.mw.0;
    let (lo, hi) = (bounds.min_position(), bounds.max_position());
    let top = hi.y + 0.001;
    let mut still = false;
    for x in lo.x.floor() as i32..hi.x.ceil() as i32 {
        for y in hi.y.floor() as i32..top.ceil() as i32 {
            for z in lo.z.floor() as i32..hi.z.ceil() as i32 {
                let (fluid, h) = mw.fluid_height(x, y, z);
                if fluid != Fluid::Water || top >= f64::from(y) + h {
                    continue;
                }
                let f = mw.facts.get(mw.world.block(x, y, z));
                if f.fluid_level != 8 || f.fluid_falling {
                    return Under::Flowing;
                }
                still = true;
            }
        }
    }
    if still { Under::Still } else { Under::Dry }
}

/// `AbstractBoat.getGroundFriction`: the mean of the blocks' friction over the blocks whose collision shape meets the slab just under
/// the hull's box (`joinIsNotEmpty` AND), zero when none does.
fn ground_friction(level: &Level, lo: Vec3, hi: Vec3) -> f32 {
    let mw = level.mw.0;
    let Ok(slab) = VoxelShape::from_box(Aabb::new(Vec3::new(lo.x, lo.y - 0.001, lo.z), Vec3::new(hi.x, lo.y, hi.z))) else {
        return 0.0;
    };
    let mut scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
    let context = CollisionContext::position(lo.y);
    let (mut sum, mut count) = (0.0f32, 0u32);
    for x in lo.x.floor() as i32 - 1..hi.x.ceil() as i32 + 1 {
        for y in (lo.y - 0.001).floor() as i32 - 1..lo.y.ceil() as i32 + 1 {
            for z in lo.z.floor() as i32 - 1..hi.z.ceil() as i32 + 1 {
                let state = mw.world.block(x, y, z);
                let f = mw.facts.get(state);
                if &*f.name == "minecraft:lily_pad" {
                    continue;
                }
                let Ok(shape) = mw.collision.collision_shape_with_context(&mut scene, state, BlockPos::new(x, y, z), &context) else {
                    continue;
                };
                let moved = shape.moved(Vec3::new(f64::from(x), f64::from(y), f64::from(z)));
                if moved.join_is_not_empty(&slab, ShapeOp::AND).unwrap_or(false) {
                    sum += f.friction;
                    count += 1;
                }
            }
        }
    }
    if count > 0 { sum / count as f32 } else { 0.0 }
}

/// The box `bounds` moved to the height `y`: where the hull would be in the water.
fn lifted(bounds: Aabb, y: f64) -> Aabb {
    let (lo, hi) = (bounds.min_position(), bounds.max_position());
    Aabb::new(Vec3::new(lo.x, y, lo.z), Vec3::new(hi.x, y + HEIGHT, hi.z))
}

/// One awake boat's tick: its status, the float, the steering, the paddles, the move, the fall, then sleep or the void.
fn step_one(w: &mut World, level: &Level, e: Entity, void_y: f64) {
    let (Some(boat), Some(pos), Some(vel), Some(yaw), Some(contact), Some(bounds), Some(hull)) = (
        w.get::<Boat>(e).copied(),
        w.get::<Pos>(e).copied(),
        w.get::<Vel>(e).copied(),
        w.get::<Yaw>(e).copied(),
        w.get::<Contact>(e).copied(),
        w.get::<Bounds>(e).copied(),
        w.get::<Hull>(e).copied(),
    ) else {
        return;
    };
    let oars = w.get::<Oars>(e).map(|o| o.0).unwrap_or([0.0; 2]);
    let steered = w.entity(e).contains::<Steered>();
    let keys = w.get::<PilotInput>(e).map_or([false; 4], |p| p.keys);
    let (mut boat, mut pos, mut vel, mut yaw, mut contact) = (boat, pos.0, vel.0, yaw.0, contact);
    let gravity = if w.get::<NoGravity>(e).is_some() { 0.0 } else { GRAVITY };
    let (lo, hi) = (bounds.0.min_position(), bounds.0.max_position());

    // The status of this tick: the water over and in the hull, else the ground under it (kept by `Contact` from the last move).
    let fluids = level.mw.0.fluids_from(bounds.0, lo.y, hi.y);
    let (surface, in_water) = (lo.y + fluids.water, fluids.in_water);
    let under = hull_under(level, bounds.0);
    let dry = matches!(under, Under::Dry);
    boat.water_level = if dry { surface } else { hi.y };
    let friction = if dry && !in_water { ground_friction(level, lo, hi) } else { 0.0 };
    let entered = set_status(&mut boat, classify(under, in_water, friction), friction);
    if entered {
        // The boat reaches the water from the air: it is set on the surface when the box there is clear.
        let above = surface as f32;
        let target = f64::from(above - HEIGHT as f32) + 0.101;
        let clear = level.no_collision(lifted(bounds.0, target));
        enter_water(&mut boat, above, clear, &mut pos.y, &mut vel.y);
    } else {
        float(&mut boat, &mut vel, &mut yaw, pos.y, gravity, None);
    }
    if steered {
        steer(&mut boat, keys, &mut vel, &mut yaw);
    } else {
        boat.paddling = [false; 2];
    }
    if matches!(boat.status, Status::InWater | Status::UnderWater | Status::UnderFlowingWater) {
        contact.fall_distance = 0.0;
    }

    // The move through the blocks. `AbstractBoat.checkFallDamage` keeps the fall distance and hurts nothing (it never calls
    // `causeFallDamage`), so the damage `do_move` reports is dropped and no event is made.
    let body = Body { width: WIDTH as f32, height: HEIGHT as f32, step_height: 0.0, speed: 0.0, fall_factor: 1.0, safe_fall: 3.0, fall_mult: 1.0, movement_efficiency: 0.0, lava_walker: false, object: false, orb: false, tnt: false };
    let (mut at, mut moving, mut boxed) = (Pos(pos), Vel(vel), Bounds(bounds.0));
    let old_y = pos.y;
    let mut fall_damage = 0.0_f32;
    do_move(level, &mut at, &mut moving, &mut contact, &body, &mut boxed, &mut fall_damage);

    // The paddles row while steered: a sound at each stroke's top, on the water or on land.
    let mut angles = oars;
    for side in 0..2 {
        if steered && boat.paddling[side] {
            let p = angles[side] % (2.0 * PI);
            // `isSilent` stops the sound only; the paddle still turns.
            if p <= PI / 4.0 && (angles[side] + PI / 8.0) % (2.0 * PI) >= PI / 4.0 && boat.status != Status::InAir && w.get::<Silent>(e).is_none() {
                let pitch = 0.8 + 0.4 * crate::boat_effects::draw_unit(w);
                let water = matches!(boat.status, Status::InWater | Status::UnderWater | Status::UnderFlowingWater);
                events(w).push(BoatEvent::Paddle { entity: e, side: side as u8, pitch, water });
            }
            angles[side] += PI / 8.0;
        } else {
            angles[side] = 0.0;
        }
    }

    let at_rest = !steered
        && boat.status != Status::InAir
        && moving.0.x.abs() < REST
        && moving.0.y.abs() < REST
        && moving.0.z.abs() < REST
        && f64::from(boat.delta_rotation.abs()) < REST
        && contact.on_ground;
    let crossed = old_y >= void_y && at.0.y < void_y;
    let under = matches!(boat.status, Status::UnderWater | Status::UnderFlowingWater);
    let mut em = w.entity_mut(e);
    em.insert((at, moving, Yaw(yaw), contact, Bounds(boxed.0), boat, Hull { fall: contact.fall_distance, on_ground: contact.on_ground, ..hull }));
    if !under {
        em.remove::<OutOfControl>();
    }
    if angles == [0.0; 2] {
        em.remove::<Oars>();
    } else {
        em.insert(Oars(angles));
    }
    if at_rest {
        em.remove::<Moving>();
    }
    if under && w.get::<OutOfControl>(e).is_none() {
        let since = now_of(w, e);
        w.entity_mut(e).insert(OutOfControl(since));
        after(w, e, TIME_TO_EJECT as i32, eject_out_of_control);
    }
    if crossed {
        after(w, e, 1, void_check);
    }
}

/// The wake at `TIME_TO_EJECT` of a run under water: the riders get off (`eject_all`, the seats' own path) when the run is that old; a run
/// that began later has its own wake.
fn eject_out_of_control(w: &mut World, e: Entity) {
    let Some(since) = w.get::<OutOfControl>(e).map(|o| o.0) else { return };
    if now_of(w, e).saturating_sub(since) >= TIME_TO_EJECT {
        crate::seat::eject_all(w, e);
    }
}

/// The boats' events queue, created on the first one.
fn events(w: &mut World) -> &mut Vec<BoatEvent> {
    &mut w.get_resource_or_insert_with(BoatEvents::default).into_inner().0
}

/// `AbstractBoat.tick` for the awake boats only: a sleeping boat is not in the query, so it costs nothing.
pub fn boat_step(w: &mut World) {
    if !crate::goals::sparse_any::<Moving>(w) {
        return;
    }
    let level = *w.resource::<Level>();
    let void_y = f64::from(level.mw.0.world.bottom_y() - BELOW);
    let boats: Vec<Entity> = crate::queries::entities::<(With<Boat>, With<Moving>)>(w);
    for e in boats {
        step_one(w, &level, e, void_y);
        if w.entity(e).contains::<crate::leash::Leash>() {
            crate::boat_leash::tick(w, e);
        }
        crate::vehicle_push::push_pairs(w, e);
    }
}

/// The deadline of a boat that fell under the world (`Entity.checkBelowWorld`): it is discarded if it is still there.
pub fn void_check(w: &mut World, e: Entity) {
    let void_y = f64::from(w.resource::<Level>().mw.0.world.bottom_y() - BELOW);
    if w.get::<Pos>(e).is_some_and(|p| p.0.y < void_y) {
        w.entity_mut(e).despawn();
    }
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(boat_step.after(block_changes).before(reindex));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `controlBoat`'s push with `Mth`'s sine table: the values of the host's `paddling_pushes_along_the_sine_table`, worked apart from
    /// Horizon in Python with float32 rounding.
    #[test]
    fn paddling_pushes_along_the_sine_table() {
        for (yaw, acceleration, push) in [
            (17.3, 0.04, (-0.011893643997609615, 0.03819085285067558)),
            (-123.45, 0.04, (0.03337550163269043, -0.022047126665711403)),
            (250.0, 0.04, (0.03758755698800087, -0.013681206852197647)),
            (-6.736, 0.005, (0.0005863539990969002, 0.004965443629771471)),
        ] {
            assert_eq!(paddle_push(yaw, acceleration), push, "yaw {yaw}");
        }
    }

    /// Forward at yaw 0 goes along +z by the paddle's 0.04: a steered boat accelerates, a boat with no key does not.
    #[test]
    fn a_forward_key_pushes_the_boat_ahead() {
        let mut b = Boat::default();
        let (mut vel, mut yaw) = (Vec3::ZERO, 0.0_f32);
        steer(&mut b, [false, false, true, false], &mut vel, &mut yaw);
        assert!(vel.x.abs() < 1.0e-12 && (vel.z - 0.04).abs() < 1.0e-6, "{vel:?}");
        assert_eq!(b.paddling, [true, true]);
        let mut idle = Boat::default();
        let (mut vel, mut yaw) = (Vec3::ZERO, 0.0_f32);
        steer(&mut idle, [false; 4], &mut vel, &mut yaw);
        assert_eq!(vel, Vec3::ZERO);
    }
}
