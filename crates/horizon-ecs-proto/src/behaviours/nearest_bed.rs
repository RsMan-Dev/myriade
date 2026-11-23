//! Recherche de lit pour les bébés (`sense_bed`, `villager_brain.rs:690`) : lits dans 48 blocs, au plus 5 nouveaux essais
//! par passage, cache de 40 ticks par lit essayé. Le lit visé va dans `BED_CANDIDATE` (un `reach_block` le rejoint) ; il
//! n'entre dans `NEAREST_BED` qu'une fois atteint, comme le chemin `reached` du `Mob`.
use super::poi_claim::block_of;
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::{now_of, GrowAt, Pos, Rng};
use crate::poi::PoiRegistry;
use horizon_world::poi::PoiType;

pub struct Params {
    pub nearest: &'static MemKey,
    pub candidate: &'static MemKey,
    /// The entity's follow range, the length of the paths it searches (the villager's attribute is 48; the species table holds the type's default).
    pub follow_range: f32,
}

/// Lits essayés récemment : (position, jusqu'à quel tick). Seulement chez les bébés.
#[derive(Component, Clone, Default)]
#[component(storage = "SparseSet")]
pub struct BedCache(pub Vec<([i32; 3], i64)>);

fn is_home(k: PoiType) -> bool {
    k == PoiType::Home
}

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    if w.get::<GrowAt>(e).is_none() {
        return;
    }
    let now = now_of(w, e) as i64;
    let last_update = now + w.get_mut::<Rng>(e).map_or(0, |mut r| i64::from(r.next_u32() % 20));
    let here = block_of(w, e);
    let homes: Vec<[i32; 3]> = w.resource::<PoiRegistry>().in_range(here, 48, false, is_home).into_iter().map(|h| h.0).collect();
    let mut cache = w.get::<BedCache>(e).cloned().unwrap_or_default();
    let mut tried = 0;
    let mut targets = Vec::new();
    for h in homes {
        if cache.0.iter().any(|(q, _)| *q == h) {
            continue;
        }
        tried += 1;
        if tried >= 5 {
            continue;
        }
        cache.0.push((h, last_update + 40));
        targets.push(h);
    }
    // `createPath(targets, 1)`: the bed enters `NEAREST_BED` when a path reaches one of the tried beds, wherever the baby stands.
    let reached = reached_bed(w, e, &targets, p.follow_range);
    if let Some(t) = targets.first() {
        set(w, e, p.candidate, MemValue::Block(*t), None);
    }
    match reached {
        Some(bed) => set(w, e, p.nearest, MemValue::Block(bed), None),
        None if tried < 5 => cache.0.retain(|(_, until)| *until >= last_update),
        None => {}
    }
    w.entity_mut(e).insert(BedCache(cache.0));
}

/// The path mob of an entity (the `Mob` fields the path search reads), with its follow range.
pub(crate) fn path_mob(w: &World, e: Entity) -> Option<(myriade_engine_lib::mobs::path::PathMob, f32)> {
    use crate::cow::{Body, Contact, FluidState, NavParams};
    let (pos, body, c, sp) = (w.get::<Pos>(e)?, w.get::<Body>(e)?, w.get::<Contact>(e)?, w.get::<NavParams>(e)?);
    let pm = myriade_engine_lib::mobs::path::PathMob {
        position: pos.0,
        width: body.width,
        height: body.height,
        on_ground: c.on_ground,
        in_water: w.get::<FluidState>(e).is_some_and(|f| f.in_water),
        max_up_step: body.step_height,
        max_fall_distance: crate::cow::comfortable_fall_distance(0.0),
        malus: sp.malus,
        can_float: sp.can_float,
        can_open_doors: sp.can_open_doors,
        can_pass_doors: sp.can_pass_doors,
        can_walk_over_fences: sp.can_walk_over_fences,
        stands_on_lava: false,
    };
    Some((pm, sp.follow_range))
}

/// The target of the path that reaches one of `targets` within 1 (`Mob.create_path_to_any(targets, 1).reached`), the search `pathfind` makes.
fn reached_bed(w: &World, e: Entity, targets: &[[i32; 3]], follow_range: f32) -> Option<[i32; 3]> {
    use myriade_engine_lib::mobs::path::{Evaluator, Finder};
    if targets.is_empty() {
        return None;
    }
    let (pm, own) = path_mob(w, e)?;
    let max = follow_range.max(own);
    let path = Finder::new(w.resource::<crate::cow::Level>().mw.0, &pm, Evaluator::Walk).find(targets, max, 1, (max * 16.0).floor() as i32)?;
    (path.reached && targets.contains(&path.target)).then_some(path.target)
}

/// The bed the baby tried last (the first of the scan).
pub static BED_CANDIDATE: MemKey = MemKey::new("horizon:bed_candidate", Shape::Block);
pub static SENSOR: Sensor = Sensor { name: "minecraft:nearest_bed", mems: &[&crate::behaviours::bed::NEAREST_BED, &BED_CANDIDATE], scan, ..Sensor::DEFAULT };

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{spawn_species, Herd, SpeciesTable};

    static NEAREST: MemKey = MemKey::new("test:nearest_bed", Shape::Block);
    static CANDIDATE: MemKey = MemKey::new("test:bed_candidate", Shape::Block);
    static PARAMS: Params = Params { nearest: &NEAREST, candidate: &CANDIDATE, follow_range: 48.0 };

    /// The latency of the `Mob` (`sense_bed`): a bed tried at `now + draw(0..20)` is not tried again before 40 ticks more, and the entry
    /// leaves the cache at the first scan after that. The draw is imposed: the entity's `Rng` is set, and the same generator gives the
    /// expected value.
    #[test]
    fn a_tried_bed_is_cached_for_40_ticks_after_the_drawn_start() {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        }
        let mut h = Herd::with_slab_awake(200, false, false);
        h.set_block_state(6, 1, 0, "minecraft:red_bed", &[("part", "head")]);
        h.tick();
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 5);
        let now0 = now_of(&h.world, e);
        h.world.entity_mut(e).insert(GrowAt(now0 + 24000));
        let ctx = Ctx { id: 0, nth: 0, params: &PARAMS };
        let seed = 0x9E37_79B9_7F4A_7C15u64;
        let draw = |s: u64| i64::from(Rng(s).next_u32() % 20);
        h.world.entity_mut(e).insert(Rng(seed));
        let now = now_of(&h.world, e) as i64;
        scan(&mut h.world, e, &ctx);
        let cache = h.world.get::<BedCache>(e).unwrap().0.clone();
        assert_eq!(cache, vec![([6, 1, 0], now + draw(seed) + 40)], "the bed is cached until now + draw + 40");
        let until = cache[0].1;
        // Within the window the bed is skipped: no new entry, the cache is untouched.
        scan(&mut h.world, e, &ctx);
        assert_eq!(h.world.get::<BedCache>(e).unwrap().0, cache, "a cached bed is not tried again");
        // After the window (with the next imposed draw) the entry is dropped.
        while (now_of(&h.world, e) as i64) < until + 20 {
            h.tick();
        }
        // The villager's own sensor (period 20) scans during these ticks: the expired entry is gone at its first scan after the expiry.
        assert!(h.world.get::<BedCache>(e).unwrap().0.iter().all(|c| c.1 > until), "an expired entry leaves the cache at the first scan after it");
    }
}
