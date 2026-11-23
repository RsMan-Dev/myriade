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
    let mut mods = w.get::<crate::attr_mod::AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == "max_health")).map_or(Vec::new(), |x| x.2.clone());
    mods.extend(crate::effects::attribute_modifiers(w, e, "max_health"));
    match w.get::<SpeciesId>(e) {
        Some(_) => crate::attr_mod::sanitize(crate::attr_mod::value(f64::from(base), &mods), 1.0, 1024.0) as f32,
        None => base,
    }
}

/// The entities of the types `types` (by `TypeName`) whose box meets `area`, other than `e`.
pub fn count_others_of(w: &World, e: Entity, types: &[&str], area: Aabb) -> usize {
    creatures_in(w, area, e).into_iter().filter(|&c| w.get::<TypeName>(c).is_some_and(|n| types.contains(&n.0))).count()
}

/// `LivingEntity.getScale`: the `minecraft:scale` attribute (base 1, its modifiers, then the range 0.0625 to 16 of `RangedAttribute`), capped by
/// the species (`sanitizeScale`). Without a written scale the body is 1.
pub fn model_scale(w: &World, e: Entity) -> f32 {
    let Some((_, base, mods)) = w.get::<crate::attr_mod::AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == "scale")) else { return 1.0 };
    let base = if base.is_nan() { 1.0 } else { *base };
    let v = crate::attr_mod::value(base, mods).clamp(0.0625, 16.0) as f32;
    let cap = w.get::<SpeciesId>(e).and_then(|s| w.get_resource::<SpeciesTable>().map(|t| t.0[s.0 as usize].max_scale)).unwrap_or(f32::MAX);
    v.min(cap)
}

/// `Entity.refreshDimensions`, run by the events that move a body's box: the `scale` attribute (`attr_mod::touched`), a growth up
/// (`wheel`), a bed (`Sleeping`'s hook). The box is the species' (a sized body's, or a baby's `Sizes.baby`, which holds the age scale)
/// times `model_scale`, and the eyes move with it. Asleep: the fixed 0.2 box and eyes, unscaled (`SLEEPING_DIMENSIONS`).
pub fn refresh_box(w: &mut World, e: Entity) {
    let Some(sizes) = w.get::<crate::cow::Sizes>(e).copied() else { return };
    let (width, height, eye) = if w.get::<crate::behaviours::bed::Sleeping>(e).is_some() {
        (0.2, 0.2, 0.2)
    } else {
        let scale = model_scale(w, e);
        let mut age = 1.0;
        let (width, height) = match w.get::<crate::size::SizeRule>(e).copied() {
            Some(r) => crate::rules::size::box_of(r.base, w.get::<crate::size::Size>(e).map_or(1, |s| s.0) as f32),
            None if w.get::<crate::cow::GrowAt>(e).is_some() => {
                // `Entity.getDefaultDimensions`: the baby's type dimensions scaled by `getAgeScale` (0.5), eyes included.
                age = 0.5;
                sizes.baby
            }
            None => sizes.adult,
        };
        let eye = w.get::<SpeciesId>(e).and_then(|s| w.get_resource::<SpeciesTable>().map(|t| t.0[s.0 as usize].eye_height)).unwrap_or(0.0);
        (width * scale, height * scale, eye * age * f64::from(scale))
    };
    if let Some(mut b) = w.get_mut::<crate::cow::Body>(e) {
        (b.width, b.height) = (width, height);
    }
    if let Some(at) = w.get::<Pos>(e).map(|p| p.0)
        && let Some(mut bounds) = w.get_mut::<crate::cow::Bounds>(e)
    {
        bounds.0 = crate::cow::box_at(at, width, height);
    }
    if let Some(mut f) = w.get_mut::<crate::cow::FluidParams>(e) {
        f.eye_height = eye;
    }
    if let Some(mut t) = w.get_mut::<crate::targeting::Eye>(e) {
        t.0 = eye;
    }
    w.entity_mut(e).insert(crate::cow::Recheck);
    crate::behaviours::contact_damage::widen(w, e);
}

/// One attribute's value into the component the mechanism reads (`touched`, never per tick): the written base (else the species' one),
/// the written modifiers and the effects' ones, in `attr_mod::value`, in the attribute's range (`RangedAttribute`).
/// `LivingEntity.maxUpStep` (step_height), `getSpeed` (movement_speed), `getFollowRange`, `getGravity`, the jump strength, the landing's
/// safe distance and multiplier, the block speed's movement efficiency. `Body.fall_factor` (the hen's glide) is not the attribute.
pub fn refresh_attr(w: &mut World, e: Entity, name: &str) {
    let Some(sp) = w.get::<SpeciesId>(e).and_then(|s| w.get_resource::<SpeciesTable>().map(|t| t.0[s.0 as usize])) else { return };
    let v = |species: f64, lo: f64, hi: f64| crate::attr_mod::read(w, e, name, species).clamp(lo, hi);
    match name {
        "step_height" => {
            let v = v(f64::from(sp.step_height), 0.0, 10.0) as f32;
            if let Some(mut b) = w.get_mut::<crate::cow::Body>(e) {
                b.step_height = v;
            }
        }
        "movement_speed" => {
            let v = v(sp.speed, 0.0, 1024.0);
            if let Some(mut b) = w.get_mut::<crate::cow::Body>(e) {
                b.speed = v;
            }
        }
        "follow_range" => {
            let v = v(f64::from(sp.follow_range), 0.0, 2048.0);
            if let Some(mut f) = w.get_mut::<crate::targeting::FollowRange>(e) {
                f.0 = v;
            }
        }
        "gravity" => {
            let v = v(0.08, -1.0, 1.0);
            if let Some(mut g) = w.get_mut::<crate::cow::Gravity>(e) {
                g.0 = v;
            }
        }
        "tempt_range" => {
            // `TemptGoal` reads the live `TEMPT_RANGE` (jar default 10) at each search; the goal's ranges take it here.
            let v = v(10.0, 0.0, 2048.0);
            if let Some(mut ts) = w.get_mut::<crate::behaviours::tempt::Tempts>(e) {
                ts.0.iter_mut().for_each(|t| t.range = v);
            }
        }
        "water_movement_efficiency" => {
            let v = v(f64::from(sp.water_efficiency), 0.0, 1.0) as f32;
            if let Some(mut f) = w.get_mut::<crate::cow::FluidParams>(e) {
                f.water_efficiency = v;
            }
        }
        "flying_speed" => {
            let v = v(sp.flying_speed, 0.0, 1024.0);
            if let Some(mut f) = w.get_mut::<crate::locomotion::fly::FlyParams>(e) {
                f.flying_speed = v;
            }
        }
        "oxygen_bonus" => {
            let v = v(0.0, 0.0, 1024.0);
            w.entity_mut(e).insert(crate::cow::fluids::OxygenBonus(v));
        }
        "safe_fall_distance" => {
            let v = v(3.0, -1024.0, 1024.0);
            if let Some(mut b) = w.get_mut::<crate::cow::Body>(e) {
                b.safe_fall = v;
            }
        }
        "fall_damage_multiplier" => {
            let v = v(1.0, 0.0, 100.0);
            if let Some(mut b) = w.get_mut::<crate::cow::Body>(e) {
                b.fall_mult = v;
            }
        }
        "movement_efficiency" => {
            let v = v(0.0, 0.0, 1.0) as f32;
            if let Some(mut b) = w.get_mut::<crate::cow::Body>(e) {
                b.movement_efficiency = v;
            }
        }
        "jump_strength" => {
            let v = v(0.42, 0.0, 32.0) as f32;
            if let Some(mut m) = w.get_mut::<crate::mount::Mount>(e) {
                m.jump_strength = v;
            }
        }
        _ => {}
    }
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
