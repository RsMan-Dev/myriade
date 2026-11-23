//! Fluids, breath and wall damage.
//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
#[allow(unused_imports)]
use crate::IFloor as _;
#[allow(unused_imports)]
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
#[allow(unused_imports)]
use horizon_client_lib::mobs::{
    Mob, MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use horizon_client_render::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
use horizon_core::random::{LegacyRandom, RandomSource};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

/// `EntityFluidInteraction`: the fluid heights the box has, and whether the feet and the eyes are in them.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FluidState {
    pub water: f64,
    pub lava: f64,
    pub in_water: bool,
    pub in_lava: bool,
    pub eye_in_water: bool,
    pub eye_in_lava: bool,
    /// `Entity.wasEyeInWater`: the eyes' state of the tick before (`isUnderWater` reads it; one byte, for the drowned's swim only).
    pub was_eye_in_water: bool,
}

/// The fluid module is active: the box overlaps a column with water or lava (`fluid_update` visits only these).
#[derive(Component, Clone, Copy, Debug)]
#[component(on_add = crate::behaviours::drink_potion::thirst_hook)]
pub struct InFluid;

/// `Entity.getAirSupply`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Air(pub i32);

/// The air is below its maximum: the breathing module ticks until it is full again.
#[derive(Component, Clone, Copy, Debug)]
pub struct AirLow;

/// Ask `wall_check` to look whether the cow's head is in a block (a spawn, a teleport, a growth: the events that
/// can put a box into blocks).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Recheck;

/// The head of the cow is in a suffocating block: it hurts every tick until that stops.
#[derive(Component, Clone, Copy, Debug)]
pub struct InWall;

/// What the fluid physics reads of an entity (a capability of the body: an object without it uses `DEFAULT`).
#[derive(Component, Clone, Copy, Debug)]
pub struct FluidParams {
    pub eye_height: f64,
    pub can_float: bool,
    pub water_slowdown: f32,
    pub water_efficiency: f32,
    /// `Entity.getMaxAirSupply`.
    pub max_air: i32,
}

impl FluidParams {
    pub const DEFAULT: FluidParams = FluidParams { eye_height: 0.0, can_float: false, water_slowdown: 0.8, water_efficiency: 0.0, max_air: 300 };
}

/// `Attributes.OXYGEN_BONUS` (absent is 0), written by `refresh_attr`: `LivingEntity.decreaseAirSupply` spares the air on most ticks.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct OxygenBonus(pub f64);

/// `LivingEntity.decreaseAirSupply`: the air lost on a tick, 1 or 0. With a bonus, the air runs down only when the draw is under
/// `1 / (bonus + 1)`; without one it always runs down.
pub fn air_consumed(bonus: f64, draw: f32) -> i32 {
    i32::from(bonus <= 0.0 || f64::from(draw) < 1.0 / (bonus + 1.0))
}

/// The tag `minecraft:can_float_while_ridden`: a ridden mount of it rises 0.04 a tick in water deeper than its jump threshold.
#[derive(Component, Clone, Copy, Debug)]
pub struct FloatsWhileRidden;

/// The air a creature's own class sets (`Dolphin.getMaxAirSupply` 4800, its `increaseAirSupply` the whole maximum): only the carriers have it.
#[derive(Component, Clone, Copy, Debug)]
pub struct OwnAir {
    pub max: i32,
    pub full_refill: bool,
    /// `Nautilus.handleAirSupply`: on land the air is the one before the tick less one, in water the maximum (not without AI).
    pub nautilus: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct OwnAirParams {
    pub max: i32,
    pub full_refill: bool,
    pub nautilus: bool,
}
pub static OWN_AIR: crate::rules::Rule = crate::rules::Rule { name: "own_air", equip: equip_own_air, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn equip_own_air(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    let p = crate::rules::params::<OwnAirParams>(p);
    w.entity_mut(e).insert(OwnAir { max: p.max, full_refill: p.full_refill, nautilus: p.nautilus });
}

/// Does not drown under water.
pub const BREATHES_WATER: u32 = 2;

/// `AbstractBoat.modifyPassengerFluidInteractionBox`: the box a passenger's fluids are measured in. A passenger of a boat that is not
/// under water is measured from the boat's top, and none when its top is not above that one. Other entities keep their box.
fn passenger_box(boats: &Query<(&Bounds, &crate::boat::Boat)>, riding: Option<&crate::mount::Riding>, b: Aabb) -> Option<Aabb> {
    let Some(crate::mount::Riding(v)) = riding else { return Some(b) };
    let Ok((vb, boat)) = boats.get(*v) else { return Some(b) };
    if matches!(boat.status, crate::rules::boat::Status::UnderWater | crate::rules::boat::Status::UnderFlowingWater) {
        return Some(b);
    }
    let (lo, hi) = (b.min_position(), b.max_position());
    let top = vb.0.max_position().y;
    if top >= hi.y {
        return None;
    }
    Some(Aabb::new(Vec3::new(lo.x, lo.y.max(top), lo.z), hi))
}
/// `Entity.updateFluidInteraction` (Horizon's real fluid heights and currents), only for the cows whose box overlaps
/// a column with a fluid.
pub(crate) fn fluid_update(
    cfg: Res<Config>,
    level: Res<Level>,
    mut q: Query<(Entity, &Pos, &Bounds, &mut Vel, &mut FluidState, &mut Contact, Option<&FluidParams>, Option<&crate::mount::Riding>), With<InFluid>>,
    boats: Query<(&Bounds, &crate::boat::Boat)>,
    mut commands: Commands,
) {
    let level = &*level;
    // The eyes going under or out while the body stays in a fluid: rare, queued after the pass as events (`eye_entered_water`, `eye_left_water`).
    let flips = std::sync::Mutex::new(Vec::<(Entity, bool)>::new());
    let one = |e: Entity, pos: &Pos, b: &Bounds, v: &mut Vel, fs: &mut FluidState, c: &mut Contact, fp: Option<&FluidParams>, riding: Option<&crate::mount::Riding>| {
        let mw = level.mw.0;
        // An empty box: `EntityFluidInteraction.update` resets its trackers and returns false, so no fluid is measured.
        let Some(bx) = passenger_box(&boats, riding, b.0) else {
            if fs.eye_in_water {
                flips.lock().unwrap().push((e, false));
            }
            *fs = FluidState { was_eye_in_water: fs.eye_in_water, ..Default::default() };
            horizon_client_lib::motion::bubble_column_push_over(mw.world, mw.registry, mw.facts, mw.collision, b.0, &mut v.0, |_| {});
            return;
        };
        let f = mw.fluids(bx, pos.0.y + fp.unwrap_or(&FluidParams::DEFAULT).eye_height);
        if f.eye_in_water != fs.eye_in_water {
            flips.lock().unwrap().push((e, f.eye_in_water));
        }
        *fs = FluidState {
            water: f.water,
            lava: f.lava,
            in_water: f.in_water,
            in_lava: f.in_lava,
            eye_in_water: f.eye_in_water,
            eye_in_lava: f.eye_in_lava,
            was_eye_in_water: fs.eye_in_water,
        };
        if f.in_water || f.in_lava {
            // `update` accumulates the currents in the same box as the fluids (the passenger's modified one).
            v.0 = mw.pushed_by_currents(bx, v.0);
        }
        // `BubbleColumnBlock.entityInside`: a column of bubbles in the box lifts or drags it (no cost out of a fluid).
        horizon_client_lib::motion::bubble_column_push_over(mw.world, mw.registry, mw.facts, mw.collision, b.0, &mut v.0, |_| {});
        if f.in_water {
            c.fall_distance = 0.0;
        }
    };
    if cfg.parallel {
        q.par_iter_mut().for_each(|(e, p, b, mut v, mut fs, mut c, i, r)| one(e, p, b, &mut v, &mut fs, &mut c, i, r));
    } else {
        for (e, p, b, mut v, mut fs, mut c, i, r) in &mut q {
            one(e, p, b, &mut v, &mut fs, &mut c, i, r);
        }
    }
    let flips = flips.into_inner().unwrap();
    if !flips.is_empty() {
        commands.queue(move |w: &mut bevy_ecs::world::World| {
            let mut ev = w.resource_mut::<crate::goals::Events>();
            for (e, under) in flips {
                if under {
                    ev.eye_entered_water.push(e);
                } else {
                    ev.eye_left_water.push(e);
                }
            }
        });
    }
}

/// `LivingEntity.baseTick`'s air: the cows in a fluid, or with air missing, breathe (Horizon's real `breathe` and
/// `drown`): the air runs down while the eyes are in water, refills by 4 outside, and drowning hurts 2.
#[allow(clippy::type_complexity)]
pub(crate) fn breathing(
    level: Res<Level>,
    mut q: Query<(Entity, &Pos, &FluidState, &mut Air, Has<AirLow>, Has<InFluid>, &FluidParams, &Immune, &TypeName, &crate::goals::Uid, Has<crate::behaviours::ender_teleport::WaterSensitive>, Has<crate::effects::CanBreatheUnderWater>, Has<crate::effects::HoldsAir>, Option<&OwnAir>, Has<crate::dragon::flight::NoAi>, Option<&OxygenBonus>), Or<(With<InFluid>, With<AirLow>, With<OwnAir>)>>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
    clock: Res<Clock>,
) {
    let mw = level.mw.0;
    for (e, pos, fs, mut air, low, in_fluid, fp, imm, tn, uid, wet, tag_breathes, holds, own, no_ai, bonus) in &mut q {
        let max = own.map_or(fp.max_air, |o| o.max);
        let before = air.0;
        // `Enderman.aiStep`: the water hurts it (1 a tick, through the invulnerability window), and the hurt sends it away.
        if wet && fs.in_water {
            queue.add(e, 1.0, Cause::Drown);
        }
        let submerged = fs.eye_in_water && {
            let (x, y, z) = (pos.0.x.ifloor(), (pos.0.y + fp.eye_height).ifloor(), pos.0.z.ifloor());
            mw.block_name(x, y, z) != "minecraft:bubble_column"
        };
        // Water breathing and conduit power refill the air; the nautilus alone holds it (`LivingEntity.baseTick`).
        let water_ok = imm.has(BREATHES_WATER) || tag_breathes;
        let drowns = submerged && !water_ok && !holds;
        let held = submerged && holds && !water_ok;
        let vanilla = horizon_client_lib::breathing::Breath {
            can_breathe: !drowns,
            // The draw is the entity's for this tick: its id and the clock (`LivingEntity`'s `random`, not shared with the other mobs).
            consume: air_consumed(bonus.map_or(0.0, |b| b.0), LegacyRandom::new((clock.0 as i64) ^ (uid.0 as i64)).next_float()),
            refill: if drowns || held { 0 } else if own.is_some_and(|o| o.full_refill) { max } else { 4 },
        };
        let id = uid.0 as u32;
        let breath = horizon_client_lib::breathing::breathe(mw.loop_context, id, tn.0, vanilla);
        if let Some(d) = horizon_client_lib::breathing::drown(mw.loop_context, id, tn.0, &mut air.0, max, breath, true) {
            if let Some(damage) = d.damage {
                queue.add(e, damage, Cause::Drown);
            }
            // The bubbles and the sound (entity event 67) are the host's: one event a second, only when it drowns.
            let (at, bubbles) = ([pos.0.x, pos.0.y + fp.eye_height, pos.0.z], d.bubbles);
            commands.queue(move |w: &mut bevy_ecs::world::World| {
                w.get_resource_or_insert_with(crate::colony_register::MiscOut::default).0.push(crate::host_out_misc::MiscEvent::Drowned { entity: e, at, bubbles });
            });
        }
        // `Dolphin.tick`: without AI the air is the maximum after the base tick, in or out of water.
        if no_ai && own.is_some_and(|o| !o.nautilus) {
            air.0 = max;
        }
        if own.is_some_and(|o| o.nautilus) && !no_ai {
            // `Nautilus.handleAirSupply` tests `isInWater`, not the eye.
            air.0 = if fs.in_water { max } else { before - 1 };
        }
        if air.0 < max && !low {
            commands.entity(e).insert(AirLow);
        } else if air.0 >= max && low && !in_fluid {
            commands.entity(e).remove::<AirLow>();
        }
    }
}

/// `Entity.isInWall`: the eye box (the width times 0.8, a flat 1e-6 high) reaches a block the `intersects` test says suffocates, in every
/// cell the box touches. The width has no guard (#5872).
pub fn in_wall(width: f32, eye: [f64; 3], mut intersects: impl FnMut(i32, i32, i32, &horizon_core::geometry::Aabb) -> bool) -> bool {
    let side = f64::from(width * 0.8_f32);
    let half = [side / 2.0, 1.0e-6 / 2.0, side / 2.0];
    let lo = [0, 1, 2].map(|i| eye[i] - half[i]);
    let hi = [0, 1, 2].map(|i| eye[i] + half[i]);
    let bounds = horizon_core::geometry::Aabb::new(horizon_core::geometry::Vec3::new(lo[0], lo[1], lo[2]), horizon_core::geometry::Vec3::new(hi[0], hi[1], hi[2]));
    let c0 = lo.map(|v| v.floor() as i32);
    let c1 = hi.map(|v| v.floor() as i32);
    (c0[0]..=c1[0]).any(|x| (c0[1]..=c1[1]).any(|y| (c0[2]..=c1[2]).any(|z| intersects(x, y, z, &bounds))))
}
/// `Mob.baseTick`'s `isInWall` for the cows an event asked about (a spawn, a teleport, a growth): the head in a
/// suffocating block puts the cow in the `InWall` module (1 damage a tick) or takes it out.
pub(crate) fn wall_check(
    level: Res<Level>,
    q: Query<(Entity, &Pos, &Body, Has<InWall>, &FluidParams), With<Recheck>>,
    mut commands: Commands,
) {
    use horizon_core::geometry::{ShapeOp, VoxelShape};
    let mw = level.mw.0;
    for (e, pos, body, was, sp) in &q {
        let mut scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
        let inside = in_wall(
            body.width,
            [pos.0.x, pos.0.y + sp.eye_height, pos.0.z],
            |x, y, z, eye_bounds| {
                let p = [x, y, z];
                let state = mw.world.block(x, y, z);
                if !mw.suffocation.is_suffocating(&mut scene, state, p).unwrap_or(false) {
                    return false;
                }
                mw.collision.collision_shape(&mut scene, state, p).ok().is_some_and(|shape| {
                    let Ok(eye_shape) = VoxelShape::from_box(*eye_bounds) else {
                        return false;
                    };
                    shape
                        .moved(Vec3::new(f64::from(x), f64::from(y), f64::from(z)))
                        .join_is_not_empty(&eye_shape, ShapeOp::AND)
                        .unwrap_or(false)
                })
            },
        );
        let mut cmd = commands.entity(e);
        cmd.remove::<Recheck>();
        match (inside, was) {
            (true, false) => {
                cmd.insert(InWall);
            }
            (false, true) => {
                cmd.remove::<InWall>();
            }
            _ => {}
        }
    }
}

pub(crate) fn wall_damage(q: Query<Entity, With<InWall>>, mut queue: ResMut<DamageQueue>) {
    for e in &q {
        queue.add(e, 1.0, Cause::InWall);
    }
}
