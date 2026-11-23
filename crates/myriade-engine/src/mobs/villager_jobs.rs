//! Villager job validation, acquisition and POI classification.
use super::*;
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





