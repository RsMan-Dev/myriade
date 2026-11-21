//! The horse inventory's rules (`AbstractHorse`, `AbstractChestedHorse`): the saddle and
//! body armour slots, and the columns of a mount's chest.
use crate::game::Game;
use horizon_client::mobs::{Mob, Species, llama, nautilus};

/// Menu slot ids: the saddle and the body armour.
pub const SADDLE: usize = 700;
pub const ARMOR: usize = 701;
/// The first chest slot (`AbstractChestedHorse` INV_CHEST_COUNT slots from here).
pub const CHEST: usize = 710;
/// `AbstractChestedHorse.INV_CHEST_COUNT` and `getInventoryColumns`.
const COLUMNS: usize = 5;

/// `AbstractHorse.getInventoryColumns`: five for a donkey's or mule's chest,
/// a llama's strength for its chest, else none.
pub fn mount_columns(m: &Mob) -> usize {
    let species = m.species();
    if llama::is_llama(species) {
        llama::inventory_columns(m).clamp(0, 5) as usize
    } else if m.extra.chested && matches!(species, Species::Donkey | Species::Mule) {
        COLUMNS
    } else {
        0
    }
}

fn can_wear_horse_armor(species: Species) -> bool {
    matches!(species, Species::Horse | Species::ZombieHorse)
}

/// Whether `item` fits the body slot of the species (`isEquippableInSlot`).
fn body_armor_fits(species: Species, item: &str) -> bool {
    if llama::is_llama(species) {
        return item.ends_with("_carpet");
    }
    // EntityTypeTags.CAN_WEAR_NAUTILUS_ARMOR: the nautilus armours.
    if nautilus::is_nautilus(species) {
        return item.ends_with("_nautilus_armor");
    }
    can_wear_horse_armor(species) && item.ends_with("_horse_armor")
}

/// `ArmorSlot.mayPlace` and the stack limits of a mount's slot `slot` for a
/// stack of item `item`: whether it may be placed, and the most it may hold.
pub fn slot_rules(species: Species, slot: usize, item: &str) -> (bool, u32) {
    match slot {
        SADDLE => (item == "minecraft:saddle", 1),
        ARMOR => (body_armor_fits(species, item), 1),
        _ => (true, 64),
    }
}

impl Game<'_> {
    /// The stock hopper/dispenser slot layout, unless a mod screen is shown for
    /// that menu: a mod screen places its own container slots
    /// (`native_screen_slot`) and the player's inventory (`native_screen_inventory_y`),
    /// and this layout answers first in `slot_position`, which kept a mod's screen
    /// on a hopper or a dispenser on the stock grid (#5141). The rest of those
    /// menus is already the mod's: `draw_native_screen` owns the background and
    /// `screen_height` its height.
    pub(in crate::game) fn stock_machine_slot(
        stock: Option<(f32, f32)>,
        mod_screen_shown: bool,
    ) -> Option<(f32, f32)> {
        if mod_screen_shown { None } else { stock }
    }
}
