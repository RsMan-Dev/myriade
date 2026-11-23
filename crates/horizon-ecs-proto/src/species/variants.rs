//! The variant tables of the species that draw one at their spawn (`VariantUtils.selectVariantToSpawn` over the `spawn_conditions` of
//! the registry's entries, listed in the file order of the registry: the order of the `nextInt` pick). `id` is the host's index
//! (`variant_name`), `names` the saved identifiers. Tags of structures are given by their members.
use crate::rules::variant::{Choice, Need, Pick, Table};

const fn registry(key: &'static str, list: &'static [Choice], ids: &'static [u8], names: &'static [&'static str]) -> Table {
    Table { key, pick: Pick::Registry(list), ids, names, ..Table::EMPTY }
}
const fn uniform(n: i32, ids: &'static [u8]) -> Table {
    Table { key: "Variant", pick: Pick::Uniform(n), ids, ..Table::EMPTY }
}

const fn c(id: u8, priority: i32, need: Option<Need>) -> Choice {
    Choice { id, priority, need }
}
const fn biome(id: u8, name: &'static [&'static str]) -> Choice {
    c(id, 1, Some(Need::Biomes(name)))
}

const TEMPERATE_WARM_COLD: &[&str] = &["minecraft:temperate", "minecraft:warm", "minecraft:cold"];

/// `cold`, `temperate`, `warm` (pig, cow, chicken: the tags of the farm animals' variants).
const FARM: &[Choice] = &[c(2, 1, Some(Need::Biomes(&["#minecraft:spawns_cold_variant_farm_animals"]))), c(0, 0, None), c(1, 1, Some(Need::Biomes(&["#minecraft:spawns_warm_variant_farm_animals"])))];
const FROG_LIST: &[Choice] = &[c(2, 1, Some(Need::Biomes(&["#minecraft:spawns_cold_variant_frogs"]))), c(0, 0, None), c(1, 1, Some(Need::Biomes(&["#minecraft:spawns_warm_variant_frogs"])))];
const NAUTILUS_LIST: &[Choice] = &[c(0, 0, None), c(1, 1, Some(Need::Biomes(&["#minecraft:spawns_coral_variant_zombie_nautilus"])))];

/// Alphabetical: all_black (the swamp hut's tag `cats_spawn_as_black`, or a moon of 0.9 and more), then the others, always true.
const CAT_LIST: &[Choice] = &[
    c(10, 1, Some(Need::Structures(&["minecraft:swamp_hut"]))),
    c(10, 0, Some(Need::MoonAtLeast(0.9))),
    c(1, 0, None),
    c(4, 0, None),
    c(5, 0, None),
    c(9, 0, None),
    c(6, 0, None),
    c(7, 0, None),
    c(2, 0, None),
    c(3, 0, None),
    c(0, 0, None),
    c(8, 0, None),
];
const WOLF_LIST: &[Choice] = &[
    biome(4, &["minecraft:snowy_taiga"]),
    biome(3, &["minecraft:old_growth_pine_taiga"]),
    biome(7, &["minecraft:old_growth_spruce_taiga"]),
    c(0, 0, None),
    biome(5, &["#minecraft:is_jungle"]),
    biome(2, &["minecraft:grove"]),
    biome(1, &["#minecraft:is_savanna"]),
    biome(8, &["#minecraft:is_badlands"]),
    biome(6, &["minecraft:forest"]),
];

pub static CAT: Table = registry(
    "variant",
    CAT_LIST,
    &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    &[
        "minecraft:tabby",
        "minecraft:black",
        "minecraft:red",
        "minecraft:siamese",
        "minecraft:british_shorthair",
        "minecraft:calico",
        "minecraft:persian",
        "minecraft:ragdoll",
        "minecraft:white",
        "minecraft:jellie",
        "minecraft:all_black",
    ],
);
pub static WOLF: Table = registry(
    "variant",
    WOLF_LIST,
    &[0, 1, 2, 3, 4, 5, 6, 7, 8],
    &["minecraft:pale", "minecraft:spotted", "minecraft:snowy", "minecraft:black", "minecraft:ashen", "minecraft:rusty", "minecraft:woods", "minecraft:chestnut", "minecraft:striped"],
);
pub static FARM_ANIMAL: Table = registry("variant", FARM, &[0, 1, 2], TEMPERATE_WARM_COLD);
pub static FROG: Table = registry("variant", FROG_LIST, &[0, 1, 2], TEMPERATE_WARM_COLD);
pub static ZOMBIE_NAUTILUS: Table = registry("variant", NAUTILUS_LIST, &[0, 1], &["minecraft:temperate", "minecraft:warm"]);

/// `Parrot.finalizeSpawn`: `Util.getRandom(Variant.values())` always (the group's value does not override it).
pub static PARROT: Table = Table { redraw: true, ..uniform(5, &[0, 1, 2, 3, 4]) };
/// `Horse.finalizeSpawn`: the group's colour, else one of 7; the markings follow (`spawn_draw`).
pub static HORSE: Table = Table {
    component_names: &["white", "creamy", "chestnut", "brown", "black", "gray", "dark_brown"],
    ..uniform(7, &[0, 1, 2, 3, 4, 5, 6])
};
/// `Llama.finalizeSpawn`: the group's variant, else one of 4.
pub static LLAMA: Table = Table { component_names: &["creamy", "white", "brown", "gray"], ..uniform(4, &[0, 1, 2, 3]) };
/// `MushroomCow.Variant` (red, brown): the colour the mooshroom reports as its component; not drawn from this table.
pub static MUSHROOM: Table = Table { component_names: &["red", "brown"], ..uniform(2, &[0, 1]) };
/// `Salmon`: small 30, medium 50, large 15 (`WeightedList`), saved as the string `type`.
pub static SALMON: Table = Table { key: "type", pick: Pick::Weighted(&[(0, 30), (1, 50), (2, 15)]), ids: &[0, 1, 2], names: &["small", "medium", "large"], ..Table::EMPTY };
