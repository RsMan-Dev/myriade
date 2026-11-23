//! A birth by the level itself (`NaturalSpawner`, the spawner blocks, the reinforcements): the creature is made, finalized as the jar's
//! `Mob.finalizeSpawn` does for its reason, then told to the host (`addFreshEntityWithPassengers`). The group is the caller's: one per
//! group of a pack (`SpawnGroupData`, null again at each of `spawnCategoryForPosition`'s three groups).
use crate::cow::{Pos, Species};
use horizon_client_lib::mobs::MobWorld;
use crate::spawn_finalize::{Group, Outcome};
use crate::spawn_mod_host::Verdict;
use crate::spawn_reason::SpawnReason;
use bevy_ecs::prelude::*;
use horizon_core::random::LegacyRandom;

/// `finalizeSpawn` of `e` for `reason` with the pack's `group`: the species' draws before and after the class's own, as the jar orders
/// them, on the biome of its place. `special` is the difficulty's special multiplier there.
pub fn finalize_in(w: &mut World, mw: &MobWorld<'_>, e: Entity, reason: SpawnReason, special: f32, group: Option<Group>, random: &mut impl horizon_core::random::RandomSource) -> Outcome {
    use crate::behaviours::spawn_draw::{When, draws_of, run};
    w.insert_resource(crate::spawn_finalize::Special(special));
    let class = crate::spawn_finalize::kind_of(w, e);
    let draws = draws_of(w, e);
    let at = w.get::<Pos>(e).map_or([0; 3], |p| [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32]);
    let zoom = w.get_resource::<crate::spawning::Spawner>().map_or_else(|| crate::spawn_biome::zoom_of(mw), |s| s.zoom);
    let biome = &mw.biomes.get(crate::spawn_biome::biome_at(mw, zoom, at[0], at[1], at[2])).name;
    let has = |tag: &str| mw.data.biome_has(tag, biome);
    let ctx = |w: &World| crate::rules::spawn_draw::Ctx { biome, biome_has: &has, special, baby: w.get::<crate::cow::GrowAt>(e).is_some() };
    let mut group = group.or(draws.and_then(|d| d.group));
    if let Some(d) = draws {
        let c = ctx(w);
        run(w, e, d, When::Before, &mut group, &c, random);
    }
    let mut out = crate::spawn_finalize::finalize(w, e, class, group, reason, special, random);
    if let Some(d) = draws {
        let c = ctx(w);
        run(w, e, d, When::After, &mut out.group, &c, random);
    }
    out
}

/// `born`, then `finalizeSpawn` with the draws seeded by `seed`, then the `Spawned` event: the order of the jar, where the mob is
/// finalized before it joins the level. `group` is read and left for the next member of the same group.
pub fn born_finalized(w: &mut World, mw: &MobWorld<'_>, sp: &Species, at: [f64; 3], seed: u64, nearest_sqr: f64, reason: SpawnReason, group: &mut Option<Group>) -> Option<Entity> {
    born_finalized_with(w, mw, sp, at, seed, nearest_sqr, reason, group, &mut LegacyRandom::new(seed as i64))
}

/// `born_finalized` with the draws of `finalizeSpawn` taken from `random` (`ServerLevelAccessor.getRandom()`: the region's own at the
/// generation of a chunk).
pub fn born_finalized_with(w: &mut World, mw: &MobWorld<'_>, sp: &Species, at: [f64; 3], seed: u64, nearest_sqr: f64, reason: SpawnReason, group: &mut Option<Group>, random: &mut impl horizon_core::random::RandomSource) -> Option<Entity> {
    let e = crate::spawning::born(w, sp, at, seed, nearest_sqr);
    let block = [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32];
    if let Some(cost) = crate::spawn_cost::cost_at(w, mw, sp.name, block) {
        w.entity_mut(e).insert(crate::spawn_cost::ChargeBearer { charge: cost.charge });
    }
    // `MobSpawnEvent.FinalizeSpawn`, heard before the finalization reads the group and the difficulty (none when no mod listens).
    let verdict = crate::spawn_mod_host::lent(w).map_or(Verdict::Done(None), |h| h.finalize_spawn(sp.name, at, reason, group));
    match verdict {
        Verdict::Cancelled => {
            w.despawn(e);
            return None;
        }
        Verdict::Skipped => {}
        Verdict::Done(special) => {
            let special = special.unwrap_or_else(|| horizon_client_lib::mobs::spawning::LocalDifficulty::at(mw.world, at).special());
            let out = finalize_in(w, mw, e, reason, special, group.take(), random);
            *group = out.group;
        }
    }
    crate::bridge::host_events_more::said_spawned(w, e, reason == SpawnReason::Natural, reason);
    Some(e)
}
