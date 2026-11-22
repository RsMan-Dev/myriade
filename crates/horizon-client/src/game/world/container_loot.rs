//! First-access loot shared by blocks and chest vehicles.
use super::Game;
use horizon_client::{containers::BlockEntity, save::PendingContainerLoot};
use horizon_core::{
    nbt::Tag,
    random::{LegacyRandom, RandomSource},
};

#[derive(Clone, Copy)]
pub(super) enum LootTarget {
    Block((i32, i32, i32)),
    Cart(u32),
    Boat(u32),
}
impl Game<'_> {
    pub(super) fn loot_inventory_item(
        &mut self,
        key: (i32, i32, i32),
        index: usize,
    ) -> Option<horizon_client::items::ItemStack> {
        self.unpack_loot(LootTarget::Block(key), None);
        self.world
            .block_entities
            .get(&key)?
            .slots()
            .get(index)?
            .clone()
    }
    /// A hopper cart's tick reads its contents from the ECS after the first access rolls them there.
    pub(super) fn unpack_ticked_cart_loot(
        &mut self,
        cart: &mut super::minecarts::Minecart,
    ) -> bool {
        if cart.loot.is_none() {
            return true;
        }
        if self.ecs.cart_kind(cart.id).is_none() {
            return false;
        }
        self.unpack_loot(LootTarget::Cart(cart.id), None);
        cart.loot = None;
        if let Some(items) = self.ecs.cart_items(cart.id) {
            cart.items = items;
        }
        true
    }
    /// Consumes metadata before callbacks, so reentrant access cannot roll twice.
    /// A player context is captured from the admitted actor, also during as_guest.
    pub(super) fn unpack_loot(&mut self, target: LootTarget, player: Option<mod_api::PlayerRef>) {
        let (pending, pos, occupied) = match target {
            LootTarget::Block(key) => {
                let Some(pending) = take_block_pending(&mut self.world, key) else {
                    return;
                };
                (
                    pending,
                    key,
                    self.world
                        .block_entities
                        .get(&key)
                        .unwrap()
                        .slots()
                        .iter()
                        .any(Option::is_some),
                )
            }
            LootTarget::Cart(id) => {
                let Some(view) = self.ecs_view(id) else {
                    return;
                };
                let Some((table, seed)) = self.ecs.cart_take_loot(id) else {
                    return;
                };
                let pending = PendingContainerLoot {
                    table: Tag::String(table.as_str().into()),
                    seed: Some(Tag::Long(seed)),
                };
                self.store_cart_items((i32::MAX - 15, super::minecarts::CART_KEY_Y, id as i32));
                let [x, y, z] = view.position;
                (
                    pending,
                    (x.floor() as i32, y.floor() as i32, z.floor() as i32),
                    self.ecs
                        .cart_items(id)
                        .is_some_and(|items| items.iter().any(Option::is_some)),
                )
            }
            LootTarget::Boat(id) => {
                let Some(view) = self.ecs_view(id) else {
                    return;
                };
                let Some((table, seed)) = self.ecs.boat_take_loot(id) else {
                    return;
                };
                let pending = PendingContainerLoot {
                    table: Tag::String(table.as_str().into()),
                    seed: Some(Tag::Long(seed)),
                };
                self.store_cart_items((i32::MAX - 15, super::minecarts::CART_KEY_Y, id as i32));
                let [x, y, z] = view.position;
                (
                    pending,
                    (x.floor() as i32, y.floor() as i32, z.floor() as i32),
                    self.ecs
                        .boat_items(id)
                        .is_some_and(|items| items.iter().any(Option::is_some)),
                )
            }
        };
        // Horizon-only: an inventory with both items and a table drops the
        // table on first access and never rolls, even if later emptied.
        if occupied {
            return;
        }
        let table = pending.table_id();
        if player.is_some() {
            self.trigger_loot_table(&table);
        }
        let Some(loot) = self.server_loot.get() else {
            return;
        };
        let biome = self
            .biome_name_at(super::Vec3::new(pos.0 as f64, pos.1 as f64, pos.2 as f64))
            .unwrap_or_default();
        let mut entity = player.map(|_| self.killer_loot_entity(None));
        if let Some(entity) = &mut entity {
            entity.on_fire = self.player.as_ref().is_some_and(|p| p.fire_ticks > 0);
        }
        let luck = if player.is_some() {
            self.player
                .as_mut()
                .and_then(|p| p.attribute("minecraft:luck").ok())
                .unwrap_or(0.0) as f32
        } else {
            0.0
        };
        let context = horizon_world::loot::ContainerLootContext {
            biome: Some(&biome),
            luck,
            entity: entity.as_ref(),
        };
        let Some(mut slots) = (match target {
            LootTarget::Block(key) => self
                .world
                .block_entities
                .get(&key)
                .map(|e| e.slots().to_vec()),
            LootTarget::Cart(id) => self.ecs.cart_items(id),
            LootTarget::Boat(id) => self.ecs.boat_items(id),
        }) else {
            return;
        };
        let origin = match target {
            LootTarget::Block(_) => [pos.0 as f64 + 0.5, pos.1 as f64 + 0.5, pos.2 as f64 + 0.5],
            LootTarget::Cart(id) | LootTarget::Boat(id) => self
                .ecs_view(id)
                .map_or([pos.0 as f64, pos.1 as f64, pos.2 as f64], |v| v.position),
        };
        let mut random = self.world.random.clone();
        let mut sequences = std::mem::take(&mut self.rand_sequences);
        fill_with_random(
            &pending,
            &mut slots,
            &loot,
            &mut random,
            &mut sequences,
            self.level.seed,
            context,
            |items| self.container_fill_hook(&table, target, origin, player, items),
        );
        self.world.random = random;
        self.rand_sequences = sequences;
        match target {
            LootTarget::Block(key) => {
                if let Some(e) = self.world.block_entities.get_mut(&key) {
                    e.slots_mut().clone_from_slice(&slots);
                }
            }
            LootTarget::Cart(id) => {
                for (slot, stack) in slots.iter().cloned().enumerate() {
                    self.ecs.cart_set_slot(id, slot, stack);
                }
            }
            LootTarget::Boat(id) => {
                for (slot, stack) in slots.iter().cloned().enumerate() {
                    self.ecs.boat_set_slot(id, slot, stack);
                }
            }
        }
        let id = match target {
            LootTarget::Cart(id) | LootTarget::Boat(id) => Some(id),
            _ => None,
        };
        if let Some(id) = id {
            let items = match target {
                LootTarget::Cart(_) => self.ecs.cart_items(id),
                _ => self.ecs.boat_items(id),
            };
            if let (Some(items), Some(BlockEntity::Chest(open))) = (
                items,
                self.world.block_entities.get_mut(&(
                    i32::MAX - 15,
                    super::minecarts::CART_KEY_Y,
                    id as i32,
                )),
            ) {
                *open = items;
            }
        }
    }
}

/// Consume the sole block-owned marker before any callback or inventory mutation.
pub(super) fn take_block_pending(
    world: &mut horizon_client::world::World,
    key: (i32, i32, i32),
) -> Option<PendingContainerLoot> {
    if !world.block_entities.contains_key(&key)
        || !world
            .chunks
            .get(&(key.0 >> 4, key.2 >> 4))
            .and_then(|c| c.saved.as_ref())
            .is_some_and(|s| s.pending_loot.contains_key(&key))
    {
        return None;
    }
    let mut pending = None;
    world.with_chunk_extras(key, |extras| pending = extras.pending_loot.remove(&key));
    world.modified.insert((key.0 >> 4, key.2 >> 4));
    pending
}

/// Vanilla LootContext.Builder: explicit seed, named sequence, then level random.
fn fill_with_random(
    pending: &PendingContainerLoot,
    slots: &mut [Option<horizon_client::items::ItemStack>],
    loot: &horizon_world::loot::LootData,
    level_random: &mut LegacyRandom,
    sequences: &mut horizon_client::random_sequences::RandomSequences,
    world_seed: i64,
    context: horizon_world::loot::ContainerLootContext<'_>,
    mut hook: impl FnMut(&mut Vec<horizon_world::loot::LootStack>),
) {
    if let Some(seed) = pending.seed_value().filter(|seed| *seed != 0) {
        unpack_slots(
            pending,
            slots,
            loot,
            &mut LegacyRandom::new(seed),
            context,
            &mut hook,
        );
    } else if let Some(id) = loot.random_sequence(&pending.table_id()) {
        unpack_slots(
            pending,
            slots,
            loot,
            sequences.get(id, world_seed),
            context,
            &mut hook,
        );
    } else {
        unpack_slots(pending, slots, loot, level_random, context, &mut hook);
    }
}

/// The single roll/placement implementation used by Game and its mutable level adapter.
fn unpack_slots<R: RandomSource + Clone>(
    pending: &PendingContainerLoot,
    slots: &mut [Option<horizon_client::items::ItemStack>],
    loot: &horizon_world::loot::LootData,
    random: &mut R,
    context: horizon_world::loot::ContainerLootContext<'_>,
    mut hook: impl FnMut(&mut Vec<horizon_world::loot::LootStack>),
) {
    // Entry points discard a pre-existing mixed state. Items added by the
    // player's advancement rewards keep their slots while this roll fills free ones.
    let Some(mut items) = loot.roll_container_with(&pending.table_id(), random, context) else {
        return;
    };
    hook(&mut items);
    let mut placed = vec![None; slots.len()];
    let free = slots
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.is_none().then_some(i))
        .collect();
    loot.place_container_loot(items, &mut placed, free, random);
    for (slot, stack) in slots.iter_mut().zip(placed) {
        if let Some(stack) = stack {
            *slot = Some(horizon_client::structure_blocks::loot_item_with_components(
                &stack,
            ));
        }
    }
}

impl super::GameLevel<'_> {
    /// Null-player accesses inside world behavior (comparator, destruction).
    pub(super) fn unpack_loot(&mut self, target: LootTarget, player: Option<mod_api::PlayerRef>) {
        debug_assert!(player.is_none());
        if let LootTarget::Cart(id) = target {
            let Some(view) = self
                .ecs
                .entity_of(id)
                .zip(self.ecs.world())
                .and_then(|(e, w)| horizon_ecs_proto::bridge::view::view(w, e))
            else {
                return;
            };
            let Some((table, seed)) = self.ecs.cart_take_loot(id) else {
                return;
            };
            let pending = PendingContainerLoot {
                table: Tag::String(table.as_str().into()),
                seed: Some(Tag::Long(seed)),
            };
            let Some(loot) = self.loot.as_ref() else {
                return;
            };
            let [x, y, z] = view.position;
            let pos = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
            let biome = self.world.chunks.get(&(pos.0 >> 4, pos.2 >> 4)).map(|c| {
                self.biomes
                    .get(c.biome(pos.0 & 15, pos.1, pos.2 & 15))
                    .name
                    .clone()
            });
            let key = (i32::MAX - 15, super::minecarts::CART_KEY_Y, id as i32);
            let mut slots = self.ecs.cart_items(id).unwrap_or_default();
            if let Some(BlockEntity::Chest(open)) = self.world.block_entities.get(&key) {
                slots.clone_from(open);
            }
            if slots.iter().any(Option::is_some) {
                self.put_cart_slots(id, slots);
                return;
            }
            let loot = loot.clone();
            let mut random = self.world.random.clone();
            let mut sequences = std::mem::take(self.rand_sequences);
            fill_with_random(
                &pending,
                &mut slots,
                &loot,
                &mut random,
                &mut sequences,
                self.seed,
                horizon_world::loot::ContainerLootContext {
                    biome: biome.as_deref(),
                    ..Default::default()
                },
                |items| {
                    self.container_fill_hook(
                        &pending.table_id(),
                        target,
                        [x, y, z],
                        player,
                        items,
                    )
                },
            );
            self.world.random = random;
            *self.rand_sequences = sequences;
            if let Some(BlockEntity::Chest(open)) = self.world.block_entities.get_mut(&key) {
                open.clone_from(&slots);
            }
            self.put_cart_slots(id, slots);
            return;
        }
        let LootTarget::Block(key) = target else {
            unreachable!("level inventory access")
        };
        let Some(pending) = take_block_pending(self.world, key) else {
            return;
        };
        let Some(loot) = self.loot.as_ref() else {
            return;
        };
        let biome = self.world.chunks.get(&(key.0 >> 4, key.2 >> 4)).map(|c| {
            self.biomes
                .get(c.biome(key.0 & 15, key.1, key.2 & 15))
                .name
                .clone()
        });
        let Some(entity) = self.world.block_entities.get(&key) else {
            return;
        };
        if entity.slots().iter().any(Option::is_some) {
            return;
        }
        let mut slots = entity.slots().to_vec();
        let loot = loot.clone();
        let mut random = self.world.random.clone();
        let mut sequences = std::mem::take(self.rand_sequences);
        fill_with_random(
            &pending,
            &mut slots,
            &loot,
            &mut random,
            &mut sequences,
            self.seed,
            horizon_world::loot::ContainerLootContext {
                biome: biome.as_deref(),
                ..Default::default()
            },
            |items| {
                self.container_fill_hook(
                    &pending.table_id(),
                    target,
                    [key.0 as f64 + 0.5, key.1 as f64 + 0.5, key.2 as f64 + 0.5],
                    player,
                    items,
                )
            },
        );
        self.world.random = random;
        *self.rand_sequences = sequences;
        if let Some(entity) = self.world.block_entities.get_mut(&key) {
            entity.slots_mut().clone_from_slice(&slots);
        }
    }
}

#[cfg(test)]
#[path = "container_loot/tests.rs"]
mod tests;

impl super::GameLevel<'_> {
    /// The lists of the walk; carts are the ECS's, not a list of it.
    pub(super) fn entities(&self) -> super::entity::EntityLists<'_> {
        self.other_entities
    }
    /// Writes the contents of the cart `id` slot by slot through the ECS.
    fn put_cart_slots(&mut self, id: u32, slots: Vec<Option<horizon_client::items::ItemStack>>) {
        for (slot, stack) in slots.into_iter().enumerate() {
            self.ecs.cart_set_slot(id, slot, stack);
        }
    }
    /// Detector rails prioritize command carts, then read the first container.
    pub(super) fn detector_rail_signal(
        &mut self,
        pos: horizon_core::position::BlockPos,
        powered: bool,
    ) -> i32 {
        use horizon_core::geometry::Vec3;
        if !powered {
            return 0;
        }
        use super::minecarts::CartKind;
        let (x, y, z) = (pos.x as f64, pos.y as f64, pos.z as f64);
        let ids = self.ecs.carts_in(
            Vec3::new(x + 0.2, y, z + 0.2),
            Vec3::new(x + 0.8, y + 0.8, z + 0.8),
        );
        let mut container = None;
        for id in ids {
            match self.ecs.cart_kind(id).and_then(CartKind::from_id) {
                Some(CartKind::CommandBlock) => {
                    return self.ecs.cart_command(id).map_or(0, |c| c.success_count);
                }
                Some(CartKind::Chest | CartKind::Hopper) if container.is_none() => {
                    container = Some(id)
                }
                _ => {}
            }
        }
        let Some(id) = container else { return 0 };
        self.unpack_loot(LootTarget::Cart(id), None);
        let key = (i32::MAX - 15, super::minecarts::CART_KEY_Y, id as i32);
        let slots = match self.world.block_entities.get(&key) {
            Some(BlockEntity::Chest(open)) => open.clone(),
            _ => self.ecs.cart_items(id).unwrap_or_default(),
        };
        super::minecarts::Minecart::container_signal(&slots, self.items)
    }
}
