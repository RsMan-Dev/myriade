//! The whole stack of the item a player uses on an entity (its data components, its wear), for the rules that carry it over: the host
//! inserts it around one interaction (`EcsWorld::interact_with`); a rule reads it with `of`. Absent: only the item id is known.
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;

#[derive(Resource, Clone, Debug)]
pub struct UsedStack {
    pub stack: ItemStack,
    /// The player has infinite materials (`Player.hasInfiniteMaterials`): nothing is spent, nothing wears.
    pub creative: bool,
}

/// The stack used, when the host gave it and it is of `item`.
pub fn of(w: &World, item: u32) -> Option<&UsedStack> {
    let name = crate::items::item_name(item)?;
    w.get_resource::<UsedStack>().filter(|u| *u.stack.item == *name)
}
