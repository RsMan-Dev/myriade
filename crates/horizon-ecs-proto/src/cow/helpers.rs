//! The cow module's helpers that the behaviours call as `crate::cow::…`: the body's numbers, the copy, the lid's push. The reads of the level
//! are in `level` and re-exported here.
use crate::area::{creatures_in, players_in, push_creature, push_player};
use crate::behaviours::attached_face::{Attached, physical_peek};
use crate::cow::{Health, Level, Pos, SpeciesId, SpeciesTable, TypeName, spawn_like, teleport};
use crate::goals::next_uid;
use crate::loot::LevelRandom;
use crate::projectile::STEPS;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::LegacyRandom;

pub use crate::level::{block_empty, block_is_air, block_is_moving_piston, box_free_of_blocks, box_free_of_crowd, full_face, has_block_tag, within_border};
pub use horizon_core::random::RandomSource;

/// The base of the maximum health: the base the entity's `AttrMods` write over the species', else the species' own; an entity without
/// a species keeps its current health.
pub fn base_max_health(w: &World, e: Entity) -> f32 {
    let written = w.get::<crate::attr_mod::AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == "max_health")).map(|x| x.1).filter(|b| !b.is_nan());
    match (written, w.get::<SpeciesId>(e), w.get_resource::<SpeciesTable>()) {
        (Some(b), ..) => b as f32,
        (None, Some(id), Some(table)) => table.0[id.0 as usize].max_health,
        _ => w.get::<Health>(e).map_or(0.0, |h| h.0),
    }
}
/// `getMaxHealth`: the base with the modifiers of `AttrMods` (a zombie leader's bonus), in the attribute's range 1 to 1024. An entity
/// without a species keeps its current health.
pub fn max_health(w: &World, e: Entity) -> f32 {
    let base = base_max_health(w, e);
    match w.get::<crate::attr_mod::AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == "max_health")) {
        Some(x) if w.get::<SpeciesId>(e).is_some() => (crate::attr_mod::value(f64::from(base), &x.2) as f32).clamp(1.0, 1024.0),
        _ => base,
    }
}

/// The entities of the types `types` (by `TypeName`) whose box meets `area`, other than `e`.
pub fn count_others_of(w: &World, e: Entity, types: &[&str], area: Aabb) -> usize {
    creatures_in(w, area, e).into_iter().filter(|&c| w.get::<TypeName>(c).is_some_and(|n| types.contains(&n.0))).count()
}

/// The scale of the body. The bench keeps no scale per entity (the server's `minecraft:scale` is not read), so every body is 1.
pub fn model_scale(_: &World, _: Entity) -> f32 {
    1.0
}

/// The lowest block y of the world.
pub fn bottom_y(w: &World) -> i32 {
    w.resource::<Level>().mw.0.world.bottom_y()
}

/// A new entity of the species of `e`, at `at` with the colour `colour`.
pub fn spawn_copy(w: &mut World, e: Entity, at: Vec3, colour: u8) -> Entity {
    let copy = spawn_like(w, e, at.x, at.z, next_uid());
    teleport(w, copy, at);
    if let Some(mut a) = w.get_mut::<Attached>(copy) {
        a.color = colour;
    }
    copy
}

/// The level's random (the loot's `LevelRandom` resource, made when absent).
pub fn with_level_random<T>(w: &mut World, f: impl FnOnce(&mut LegacyRandom) -> T) -> T {
    if w.get_resource::<LevelRandom>().is_none() {
        w.insert_resource(LevelRandom(LegacyRandom::new(0x5EED)));
    }
    f(&mut w.resource_mut::<LevelRandom>().0)
}

/// The lid of `e` eases from `old` to `new` (eased peek amounts): what its sweep opens into is pushed out of the way, by the lift it gained.
pub fn push_lid(w: &mut World, e: Entity, old: f32, new: f32) {
    let (Some(face), Some(at)) = (w.get::<Attached>(e).map(|a| a.face), w.get::<Pos>(e).map(|p| p.0)) else { return };
    let dir = face.min(5) ^ 1;
    let size = model_scale(w, e);
    let (old_p, new_p) = (physical_peek(old), physical_peek(new));
    let push = (new_p - old_p) * size;
    if push <= 0.0 {
        return;
    }
    let delta = STEPS[dir as usize].map(|s| f64::from(push * s as f32));
    let area = sweep_box(dir, f64::from(old_p), f64::from(new_p), at, size);
    for c in creatures_in(w, area, e) {
        push_creature(w, c, delta);
    }
    for id in players_in(w, area) {
        push_player(w, id, delta);
    }
}

/// The box the lid sweeps from `from` to `to` (lift amounts), on the side `dir` of the body at `at`.
fn sweep_box(dir: u8, from: f64, to: f64, at: Vec3, size: f32) -> Aabb {
    let s = STEPS[dir as usize].map(f64::from);
    let size = f64::from(size);
    let (max, min) = (from.max(to), from.min(to));
    let half = size * 0.5;
    Aabb::new(Vec3::new(-half, 0.0, -half), Vec3::new(half, size, half))
        .expand_towards(Vec3::new(s[0] * max * size, s[1] * max * size, s[2] * max * size))
        .contract(Vec3::new(-s[0] * (1.0 + min) * size, -s[1] * (1.0 + min) * size, -s[2] * (1.0 + min) * size))
        .move_by(at)
}
