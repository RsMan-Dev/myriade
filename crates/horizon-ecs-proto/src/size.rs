//! The size rule (`AbstractCubeMob.setSize`): a size, an integer, makes the body bigger and the numbers bigger: its box is the base box times
//! the size, its health the size squared, its movement speed `0.2 + 0.1 * size`, its attack damage the size (parameters, not a species). A
//! body with `split` leaves smaller ones where it dies (`2 + nextInt(3)` of half its size). The rule is a rule of the species
//! (`SIZE`, `SizeParams`); the size of a body is the component `Size`, set at creation (`spawn_sized`) or by `set_size`.
use crate::cow::*;
use crate::rules::{no_attrs, no_interact, params, Rule};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct SizeParams {
    pub health_per_sq: f32,
    pub speed_base: f64,
    pub speed_per_size: f64,
    pub damage_per_size: f32,
    /// The wait between two bounds is multiplied by this (`hop::Params.delay_mul`).
    pub delay_mul: i32,
    /// At the death a body of more than size 1 splits in `split.0 + nextInt(split.1)` bodies of half its size.
    pub split: Option<(i32, i32)>,
}
impl SizeParams {
    pub const SLIME: Self = Self { health_per_sq: 1.0, speed_base: 0.2, speed_per_size: 0.1, damage_per_size: 1.0, delay_mul: 1, split: Some((2, 3)) };
    pub const MAGMA_CUBE: Self = Self { delay_mul: 4, ..Self::SLIME };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Size(pub i32);
/// The rule of the body and the box it had at size 1.
#[derive(Component, Clone, Copy, Debug)]
pub struct SizeRule {
    pub p: SizeParams,
    pub base: (f32, f32),
}
pub static SIZE: Rule = Rule { name: "size", equip, attrs: no_attrs, interact: no_interact };
fn equip(w: &mut World, e: Entity, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<SizeParams>(p);
    let base = w.get::<Body>(e).map_or((0.5, 0.5), |b| (b.width, b.height));
    w.entity_mut(e).insert((SizeRule { p, base }, Size(1)));
    set_size(w, e, 1);
}
/// `AbstractCubeMob.setSize(size, true)`: the box, the health (full), the speed, the damage, the wait between bounds.
pub fn set_size(w: &mut World, e: Entity, size: i32) {
    let Some(r) = w.get::<SizeRule>(e).copied() else { return };
    let size = size.clamp(1, 127);
    w.get_mut::<Size>(e).unwrap().0 = size;
    let s = size as f32;
    if let Some(mut b) = w.get_mut::<Body>(e) {
        b.width = r.base.0 * s;
        b.height = r.base.1 * s;
        b.speed = f64::from(r.p.speed_base as f32 + r.p.speed_per_size as f32 * s);
    }
    if let (Some(at), Some(b)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Body>(e).copied()) {
        w.get_mut::<Bounds>(e).unwrap().0 = box_at(at, b.width, b.height);
        w.entity_mut(e).insert(Recheck);
    }
    if let Some(mut h) = w.get_mut::<Health>(e) {
        h.0 = r.p.health_per_sq * s * s;
    }
    if let Some(mut d) = w.get_mut::<AttackDamage>(e) {
        d.0 = r.p.damage_per_size * s;
    }
    if let Some(mut h) = w.get_mut::<crate::locomotion::hop::Hop>(e) {
        h.p.delay_mul = r.p.delay_mul;
    }
}
/// A body of the species `sp` at the given size.
pub fn spawn_sized(w: &mut World, sp: &Species, x: f64, z: f64, seed: u64, size: i32) -> Entity {
    let e = spawn_species(w, sp, x, z, seed);
    set_size(w, e, size);
    e
}
/// `AbstractCubeMob.remove` for a death: the smaller ones, around where it died (`setUpSplitCube`: a quarter of its width aside on each
/// axis, half a block up).
pub fn split_on_death(w: &mut World, e: Entity) {
    let (Some(r), Some(size), Some(at), Some(body), Some(sid)) = (w.get::<SizeRule>(e).copied(), w.get::<Size>(e).map(|s| s.0), w.get::<Pos>(e).map(|p| p.0), w.get::<Body>(e).copied(), w.get::<SpeciesId>(e).map(|s| s.0)) else { return };
    let Some((min, extra)) = r.p.split.filter(|_| size > 1) else { return };
    let count = min + crate::goals::with_rng(w, e, |r| r.next_int(extra));
    let sp = w.resource::<SpeciesTable>().0[sid as usize];
    let offset = body.width / 2.0;
    for i in 0..count as u32 {
        let (xd, zd) = ((i % 2) as f32 - 0.5, (i / 2) as f32 - 0.5);
        let seed = crate::goals::with_rng(w, e, |r| r.next_u32()) as u64 | 1;
        let child = spawn_sized(w, &sp, at.x, at.z, seed, size / 2);
        teleport(w, child, Vec3::new(at.x + f64::from(xd * offset), at.y + 0.5, at.z + f64::from(zd * offset)));
        let yaw = crate::goals::with_rng(w, e, |r| r.next_f64()) as f32 * 360.0;
        if let Some(mut y) = w.get_mut::<Yaw>(child) {
            y.0 = yaw;
        }
        w.resource_mut::<crate::goals::Entered>().0.push(child);
    }
}
