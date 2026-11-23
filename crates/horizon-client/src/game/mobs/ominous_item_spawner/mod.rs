//! `OminousItemSpawner`: the item an ominous trial spawner drops on its
//! fight. It is an ECS entity (`ominous_spawner.rs`: its warning sound, its
//! drop and its particles); the host keeps no list of them.
use super::Game;
use horizon_client::items::ItemStack;

const SPAWNER: &str = "minecraft:ominous_item_spawner";

impl Game<'_> {
    /// `OminousItemSpawner.create` added at `position`: the ECS spawns it, dropping `item` after `delay` ticks.
    pub(super) fn add_ominous_item_spawner(&mut self, item: ItemStack, position: [f64; 3], delay: i32) {
        let mut next = self.next_entity;
        self.ecs_with(|ecs, mw| {
            ecs.spawn_ominous(mw, position, item, delay as u32, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
    }
    /// The network ids of the ominous item spawners the ECS holds.
    pub(in crate::game) fn ominous_ids(&self) -> Vec<u32> {
        self.ecs.species_ids(SPAWNER)
    }
    /// The item, position and delay of the spawner `id`, as its saved NBT gives them.
    pub(in crate::game) fn ominous_saved(&self, id: u32) -> Option<(ItemStack, [f64; 3], i64)> {
        let e = self.ecs.entity_of(id)?;
        let tag = horizon_core::nbt::Tag::Compound(self.ecs.entity_nbt(e)?);
        let (item, at, after) = horizon_client::entity_nbt::ominous_item_spawner_from_nbt(&tag)?;
        Some((item, [at.x, at.y, at.z], after))
    }
}

