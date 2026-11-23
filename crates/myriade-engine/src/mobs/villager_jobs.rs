//! Villager job validation, acquisition and POI classification.
use super::*;
/// The POI a block state is (`PoiTypes`): job site blocks, bed heads, bells.
pub fn poi_at(w: &MobWorld, x: i32, y: i32, z: i32) -> Option<Poi> {
    poi_of_state(w, w.world.block(x, y, z))
}
/// Cache only vanilla classification. Mod additions are process-wide and replaced
/// on reload, so every live resource set consults the current extension table.
pub(super) fn poi_of_state(
    w: &MobWorld,
    state: horizon_world::registry::BlockStateId,
) -> Option<Poi> {
    let table = w.data.pois.get_or_init(|| {
        (0..w.registry.len())
            .map(|i| vanilla_state_poi(w, horizon_world::registry::BlockStateId(i as u32)))
            .collect()
    });
    let vanilla = table
        .get(state.0 as usize)
        .copied()
        .unwrap_or_else(|| vanilla_state_poi(w, state));
    vanilla
        .or_else(|| horizon_world::poi::extension(w.registry.state(state)?))
        .map(Poi::Vanilla)
        .or_else(|| {
            w.professions
                .workstation_pois
                .get(state.0 as usize)
                .copied()
                .flatten()
        })
}
/// Vanilla's job sites, bed heads and bells, without reloadable mod additions.
pub(super) fn vanilla_state_poi(
    w: &MobWorld,
    state: horizon_world::registry::BlockStateId,
) -> Option<PoiType> {
    let name = &*w.facts.get(state).name;
    if name == "minecraft:bell" {
        return Some(PoiType::Meeting);
    }
    let state = w.registry.state(state)?;
    let part = state.properties.get("part").map(String::as_str);
    if let Some(home) = runtime_bed_poi(name, part) {
        return Some(home);
    }
    horizon_world::poi::vanilla_poi(state).filter(|p| p.is_job())
}
pub(super) fn runtime_bed_poi(name: &str, part: Option<&str>) -> Option<PoiType> {
    // PoiTypes.BEDS contains the colored Blocks.BED collection, not STRAW_BED.
    (name.ends_with("_bed") && name != "minecraft:straw_bed" && part == Some("head"))
        .then_some(PoiType::Home)
}

/// Which POI types a predicate accepts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum PoiFilter {
    /// `VillagerProfession.heldJobSite` of the current profession.
    Held,
    /// `acquirableJobSite` of the current profession.
    Acquirable,
    Home,
    Meeting,
}


fn seen_profession(
    seen: &crate::mobs::Seen,
    table: &crate::mobs::villager::ResolvedProfessions,
) -> Option<usize> {
    seen.profession.map(usize::from).or_else(|| {
        seen.custom_profession
            .as_deref()
            .and_then(|id| table.resolve(id))
    })
}





