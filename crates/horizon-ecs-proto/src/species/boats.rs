//! The boat species: vanilla's boats and rafts, each wood with or without a chest. Configuration only: the state of a boat is the
//! components of `boat_state`, and a species is its entity id, its wood's index in `WOODS` and its chest flag.

/// `(NBT type, entity id, chest entity id, raft)`. Each row is spelled once by `boat_row`, the ids are its `concat!`.
macro_rules! boat_row {
    ($wood:literal, $kind:literal, $raft:literal) => {
        ($wood, concat!("minecraft:", $wood, "_", $kind), concat!("minecraft:", $wood, "_chest_", $kind), $raft)
    };
}

/// The woods of the boats and the bamboo raft (`EntityTypeIds`, 22 ids: 11 woods, each with a chest variant).
pub const WOODS: &[(&str, &str, &str, bool)] = &[
    boat_row!("oak", "boat", false),
    boat_row!("spruce", "boat", false),
    boat_row!("birch", "boat", false),
    boat_row!("jungle", "boat", false),
    boat_row!("acacia", "boat", false),
    boat_row!("cherry", "boat", false),
    boat_row!("dark_oak", "boat", false),
    boat_row!("pale_oak", "boat", false),
    boat_row!("mangrove", "boat", false),
    boat_row!("poplar", "boat", false),
    boat_row!("bamboo", "raft", true),
];

/// The index of `wood` (`oak`, `bamboo`...) in `WOODS`, None for another wood.
pub fn index(wood: &str) -> Option<u8> {
    WOODS.iter().position(|row| row.0 == wood).map(|i| i as u8)
}

/// The entity id of the species of `WOODS[wood]`, with or without a chest.
pub fn entity_id(wood: u8, chest: bool) -> &'static str {
    let row = WOODS[usize::from(wood)];
    if chest { row.2 } else { row.1 }
}

/// Every species: its entity id, its wood's index and its chest flag.
pub fn species() -> impl Iterator<Item = (&'static str, u8, bool)> {
    (0..WOODS.len() as u8).flat_map(|wood| [false, true].map(move |chest| (entity_id(wood, chest), wood, chest)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_two_species_with_no_duplicate_id() {
        let mut ids: Vec<&str> = species().map(|s| s.0).collect();
        assert_eq!(ids.len(), 2 * WOODS.len());
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 2 * WOODS.len());
    }

    #[test]
    fn a_raft_is_bamboo_and_its_ids_are_vanilla() {
        let bamboo = index("bamboo").expect("bamboo is a wood");
        assert_eq!(entity_id(bamboo, false), "minecraft:bamboo_raft");
        assert_eq!(entity_id(bamboo, true), "minecraft:bamboo_chest_raft");
        assert_eq!(entity_id(index("oak").unwrap(), true), "minecraft:oak_chest_boat");
        assert_eq!(index("stone"), None);
    }
}
