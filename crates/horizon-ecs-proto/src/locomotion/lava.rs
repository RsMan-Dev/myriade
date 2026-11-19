//! The lava medium: an animal that walks ON lava (`Strider`): the path map has lava as ground (`StandsOnLava`: `PathMob.stands_on_lava`), the
//! travel there is the walk's travel in the air (`shouldTravelInFluid` is false where the lava holds it up), no fall counts, and
//! `Strider.floatStrider` keeps it on the surface. The walk's `pathfind`, `follow` and `move_control` serve the mode (a strider in lava
//! walks the same way); its `travel` does not (it would swim). The mode becomes the active one by the event of entering lava
//! (`fluid_changed`), and is left by the event of leaving it.
use crate::{
    behaviours::{Cap, Inst, Module},
    cow::*,
};
use bevy_ecs::prelude::*;
use horizon_client_lib::blocks::Fluid;

#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Params = Params {};
}
/// The animal's path map has lava as ground (every mode of an animal that declares the lava mode: it walks to the lava on the ground too).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct StandsOnLava;

pub static MODULE: Module = Module { name: "lava", requires: &[Cap::Locomotion, Cap::Body], equip, systems, ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, _: Inst, _: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(StandsOnLava);
}
fn systems(s: &mut bevy_ecs::schedule::Schedule, _: crate::goals::GoalSlot) {
    use bevy_ecs::schedule::IntoScheduleConfigs;
    s.add_systems((travel.after(crate::cow::travel).before(crate::cow::ordered_pass), float.after(travel).before(crate::cow::ordered_pass)));
}
fn standing(level: &Level, p: &Pos) -> bool {
    let (x, y, z) = (p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32);
    level.mw.0.fluid(x, y, z) == Fluid::Lava
}
/// `LivingEntity.travel` where the lava holds the animal up: the travel in the air (the fluid state is told it is not in the fluid), and
/// `Strider.checkFallDamage`: lava breaks any fall.
#[allow(clippy::type_complexity)]
fn travel(
    level: Res<Level>,
    par: ParallelCommands,
    mut q: Query<
        (Entity, &mut Pos, &mut Vel, &Yaw, &mut Contact, &mut Drive, &Body, &mut Bounds, &FluidState, &FluidParams, &Immune, Has<Following>),
        (With<Moving>, Without<Overlapping>, With<crate::locomotion::LavaMode>),
    >,
) {
    for (e, mut p, mut v, y, mut c, mut d, b, mut bb, fs, fp, imm, following) in &mut q {
        let mut f = *fs;
        let held = standing(&level, &p);
        if held {
            f.in_lava = false;
            f.in_water = false;
        }
        let (was_on_ground, was_in_air) = (c.on_ground, !c.on_ground);
        let mut fall = 0.0_f32;
        let rest = travel_one(&level, fp, &mut p, &mut v, y, &mut c, &mut d, b, &mut bb, &f, false, &mut fall);
        if fs.in_lava {
            c.fall_distance = 0.0;
            fall = 0.0;
        }
        if was_on_ground && !c.on_ground {
            par.command_scope(|mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<crate::goals::Events>().left_ground.push(e));
            });
        }
        if was_in_air && c.on_ground {
            par.command_scope(|mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<crate::goals::Events>().landed.push(e));
            });
        }
        if fall > 0.0 && !imm.has(IMMUNE_FALL) {
            par.command_scope(|mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<DamageQueue>().add(e, fall, Cause::Fall));
            });
        }
        if rest && !following && !fs.in_lava {
            par.command_scope(|mut cmd| {
                cmd.entity(e).remove::<Moving>();
            });
        }
    }
}
/// `Strider.floatStrider` (after the tick): in lava, on the surface it stands (on the ground), deeper it is lifted and slowed.
fn float(level: Res<Level>, mut q: Query<(&Pos, &mut Vel, &mut Contact, &FluidState), (With<crate::locomotion::LavaMode>, With<InFluid>)>) {
    for (p, mut v, mut c, fs) in &mut q {
        if !fs.in_lava {
            continue;
        }
        let (x, y, z) = (p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32);
        let above = p.0.y > f64::from(y) + 0.5 - f64::from(1.0e-5_f32);
        if above && level.mw.0.fluid(x, y + 1, z) != Fluid::Lava {
            c.on_ground = true;
        } else {
            let u = v.0;
            v.0 = horizon_core::geometry::Vec3::new(u.x * 0.5, u.y * 0.5 + 0.05, u.z * 0.5);
        }
    }
}
