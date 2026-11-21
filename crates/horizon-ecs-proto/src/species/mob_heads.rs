//! The heads with a `mob_visibility` (`Item.Properties.loweredMobVisibility`): the item and the species it fools (half the visibility).

pub const MOB_HEADS: [(&str, &[&str]); 4] = [
    ("minecraft:skeleton_skull", &["minecraft:skeleton"]),
    ("minecraft:zombie_head", &["minecraft:zombie"]),
    ("minecraft:creeper_head", &["minecraft:creeper"]),
    ("minecraft:piglin_head", &["minecraft:piglin", "minecraft:piglin_brute"]),
];
