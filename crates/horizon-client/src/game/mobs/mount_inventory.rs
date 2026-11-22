//! The horse inventory's rules (`AbstractHorse`, `AbstractChestedHorse`): the saddle and
//! body armour slots, and the columns of a mount's chest.
use horizon_client::mobs::{Mob, Species, llama};

/// Menu slot ids: the saddle and the body armour.
pub const SADDLE: usize = 700;
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
