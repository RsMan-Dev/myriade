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

/// Does not drown under water.
pub const BREATHES_WATER: u32 = 2;

/// `Entity.updateFluidInteraction` (Horizon's real fluid heights and currents), only for the cows whose box overlaps
/// a column with a fluid.
pub(crate) fn fluid_update(
    cfg: Res<Config>,
    level: Res<Level>,
    mut q: Query<(&Pos, &Bounds, &mut Vel, &mut FluidState, &mut Contact, Option<&FluidParams>), With<InFluid>>,
) {
    let level = &*level;
    let one = |pos: &Pos, b: &Bounds, v: &mut Vel, fs: &mut FluidState, c: &mut Contact, fp: Option<&FluidParams>| {
        let mw = level.mw.0;
        let f = mw.fluids(b.0, pos.0.y + fp.unwrap_or(&FluidParams::DEFAULT).eye_height);
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
            v.0 = mw.pushed_by_currents(b.0, v.0);
        }
        // `BubbleColumnBlock.entityInside`: a column of bubbles in the box lifts or drags it (no cost out of a fluid).
        horizon_client_lib::motion::bubble_column_push_over(mw.world, mw.registry, mw.facts, mw.collision, b.0, &mut v.0, |_| {});
        if f.in_water {
            c.fall_distance = 0.0;
        }
    };
    if cfg.parallel {
        q.par_iter_mut().for_each(|(p, b, mut v, mut fs, mut c, i)| one(p, b, &mut v, &mut fs, &mut c, i));
    } else {
        for (p, b, mut v, mut fs, mut c, i) in &mut q {
            one(p, b, &mut v, &mut fs, &mut c, i);
        }
    }
}

/// `LivingEntity.baseTick`'s air: the cows in a fluid, or with air missing, breathe (Horizon's real `breathe` and
/// `drown`): the air runs down while the eyes are in water, refills by 4 outside, and drowning hurts 2.
#[allow(clippy::type_complexity)]
pub(crate) fn breathing(
    level: Res<Level>,
    mut q: Query<(Entity, &Pos, &FluidState, &mut Air, Has<AirLow>, Has<InFluid>, &FluidParams, &Immune, &TypeName, &crate::goals::Uid, Has<crate::behaviours::ender_teleport::WaterSensitive>, Has<crate::effects::CanBreatheUnderWater>), Or<(With<InFluid>, With<AirLow>)>>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
) {
    let mw = level.mw.0;
    for (e, pos, fs, mut air, low, in_fluid, fp, imm, tn, uid, wet, tag_breathes) in &mut q {
        // `Enderman.aiStep`: the water hurts it (1 a tick, through the invulnerability window), and the hurt sends it away.
        if wet && fs.in_water {
            queue.add(e, 1.0, Cause::Drown);
        }
        let submerged = fs.eye_in_water && {
            let (x, y, z) = (pos.0.x.ifloor(), (pos.0.y + fp.eye_height).ifloor(), pos.0.z.ifloor());
            mw.block_name(x, y, z) != "minecraft:bubble_column"
        };
        let drowns = submerged && !imm.has(BREATHES_WATER) && !tag_breathes;
        let vanilla = horizon_client_lib::breathing::Breath {
            can_breathe: !drowns,
            consume: 1,
            refill: if drowns { 0 } else { 4 },
        };
        let id = uid.0 as u32;
        let breath = horizon_client_lib::breathing::breathe(mw.loop_context, id, tn.0, vanilla);
        if let Some(d) = horizon_client_lib::breathing::drown(mw.loop_context, id, tn.0, &mut air.0, fp.max_air, breath, true)
            && let Some(damage) = d.damage
        {
            queue.add(e, damage, Cause::Drown);
        }
        if air.0 < fp.max_air && !low {
            commands.entity(e).insert(AirLow);
        } else if air.0 >= fp.max_air && low && !in_fluid {
            commands.entity(e).remove::<AirLow>();
        }
    }
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
        let inside = horizon_client_lib::mobs::ai::in_wall(
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
