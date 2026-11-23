//! The horse inventory's rules (`AbstractHorse`, `AbstractChestedHorse`): the saddle and
//! body armour slots, and the columns of a mount's chest.
use horizon_client::mobs::{Species, llama};

/// Menu slot ids: the saddle and the body armour.
pub const SADDLE: usize = 700;
/// `AbstractChestedHorse.INV_CHEST_COUNT` and `getInventoryColumns`.
const COLUMNS: usize = 5;

