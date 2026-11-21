//! The names the framework's modules need and that are species: configuration, so they live here and not in the modules
//! (`no_species_names`). Each module asks a question; the answer is this folder's.
use crate::projectile::Flight;
use crate::projectile_more;

/// The row a non-`Mob` species takes in the tables of a test world (never read).
pub const STAND_IN: &str = "minecraft:cow";
pub const TNT: &str = "minecraft:tnt";
pub const ITEM: &str = "minecraft:item";
pub const EXPERIENCE_ORB: &str = "minecraft:experience_orb";
pub const END_CRYSTAL: &str = "minecraft:end_crystal";
pub const FALLING_BLOCK: &str = "minecraft:falling_block";
pub const TRIDENT: &str = "minecraft:trident";
pub const OMINOUS_ITEM_SPAWNER: &str = "minecraft:ominous_item_spawner";
pub const EVOKER_FANGS: &str = "minecraft:evoker_fangs";
pub const AREA_EFFECT_CLOUD: &str = "minecraft:area_effect_cloud";
pub const FIREWORK_ROCKET: &str = "minecraft:firework_rocket";
pub const SHULKER_BULLET: &str = "minecraft:shulker_bullet";
pub const EGG: &str = "minecraft:egg";
pub const ENDER_PEARL: &str = "minecraft:ender_pearl";
pub const EXPERIENCE_BOTTLE: &str = "minecraft:experience_bottle";
pub const DRAGON_FIREBALL: &str = "minecraft:dragon_fireball";
pub const WIND_CHARGE: &str = "minecraft:wind_charge";
/// The effect `wither` (the name of an effect, which is also the name of a species).
pub const WITHER_EFFECT: &str = "minecraft:wither";
pub const MARKER: &str = "minecraft:marker";
pub const TEXT_DISPLAY: &str = "minecraft:text_display";
pub const BLOCK_DISPLAY: &str = "minecraft:block_display";
pub const ITEM_DISPLAY: &str = "minecraft:item_display";
pub const DISPLAYS: [&str; 3] = [BLOCK_DISPLAY, ITEM_DISPLAY, TEXT_DISPLAY];
pub const INTERACTION: &str = "minecraft:interaction";

pub const ITEM_FRAME: &str = "minecraft:item_frame";
pub const GLOW_ITEM_FRAME: &str = "minecraft:glow_item_frame";
pub const PAINTING: &str = "minecraft:painting";

pub fn is_vex(name: &str) -> bool {
    name == "minecraft:vex"
}

/// What a lightning bolt does to a creature of this species (`thunderHit` overrides).
pub enum Struck {
    /// Becomes this species unless the game is peaceful (`Pig`, `Villager`).
    Converts(&'static str),
    /// Flips its variant once per bolt (the mooshroom).
    Flips,
    /// Dies at once (the turtle).
    Dies,
    /// Becomes charged (the creeper).
    Charges,
    Plain,
}
pub fn struck(name: &str) -> Struck {
    match name {
        "minecraft:pig" => Struck::Converts("minecraft:zombified_piglin"),
        "minecraft:villager" => Struck::Converts("minecraft:witch"),
        "minecraft:mooshroom" => Struck::Flips,
        "minecraft:turtle" => Struck::Dies,
        "minecraft:creeper" => Struck::Charges,
        _ => Struck::Plain,
    }
}

/// What an ominous item spawner shoots for an item: `(flight, power, uncertainty, level event)`.
pub fn shot(item: &str) -> Option<(Flight, f32, f32, Option<i32>)> {
    Some(match item {
        "minecraft:fire_charge" => (Flight::SMALL_FIREBALL, 1.0, 6.666_666_5, Some(1018)),
        "minecraft:wind_charge" => (projectile_more::WIND_CHARGE, 1.0, 6.666_666_5, Some(1051)),
        "minecraft:experience_bottle" => (projectile_more::EXPERIENCE_BOTTLE, 1.375, 3.0, None),
        "minecraft:splash_potion" => (Flight::POTION, 1.375, 3.0, None),
        "minecraft:lingering_potion" => (Flight::LINGERING_POTION, 1.375, 3.0, None),
        "minecraft:arrow" => (Flight::ARROW, 1.1, 6.0, None),
        "minecraft:snowball" => (Flight::SNOWBALL, 1.1, 6.0, None),
        "minecraft:egg" => (crate::projectile::Flight { hit: projectile_more::egg_hit("minecraft:chicken"), ..projectile_more::EGG }, 1.1, 6.0, None),
        "minecraft:trident" => (Flight::TRIDENT, 1.1, 6.0, None),
        _ => return None,
    })
}
