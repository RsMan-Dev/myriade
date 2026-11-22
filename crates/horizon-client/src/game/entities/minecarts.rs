//! Minecarts (`AbstractMinecart` with `OldMinecartBehavior`): following
//! rails and slopes, powered and activator rails, coming off the track,
//! riding with the rider's push, pushing by players and other carts,
//! `MinecartItem` placement, `VehicleEntity` breaking, the chest, hopper,
//! furnace and TNT variants, saving, and `AbstractMinecartRenderer`'s old
//! rail-following render with the display block.
#[cfg(test)]
#[path = "minecarts/lifetime_tests.rs"]
mod lifetime_tests;
#[path = "minecarts/section_steps.rs"]
mod section_steps;

use super::{Game, Hit, InteractTarget, Keys, entity::Entity};
use crate::ecs_bridge::cart_variants::CartAttacker;
use horizon_ecs_proto::cart_variants::HitResult;
use horizon_client::{
    blocks::Fluid,
    containers::BlockEntity,
    items::ItemStack,
    mobs::riders,
};
use horizon_core::geometry::Vec3;
#[cfg(test)]
use std::sync::Arc;
// The cart state and its pure rules live in horizon-gameplay; the old paths keep working.
pub(in crate::game) use horizon_gameplay::minecarts::*;
impl Game<'_> {
    fn max_speed(&self, c: &Minecart) -> f64 {
        let base = if c.in_water { 0.2 } else { 0.4 };
        furnace_speed(c, base)
    }
    /// `AbstractMinecart.getMaxSpeed`: the behavior's, which a furnace
    /// minecart cuts (`MinecartFurnace.getMaxSpeed`).
    pub(super) fn cart_max_speed(&self, c: &Minecart) -> f64 {
        if self.minecart_improvements() {
            furnace_speed(c, self.max_speed_new(c))
        } else {
            self.max_speed(c)
        }
    }
    /// `AbstractMinecart`: `minecart_improvements` picks the smoother
    /// physics.
    pub(super) fn minecart_improvements(&self) -> bool {
        self.enabled_feature_ids()
            .iter()
            .any(|f| f == "minecraft:minecart_improvements")
    }
    /// `NewMinecartBehavior.getMaxSpeed`: the game rule over water's half,
    /// every cart alike (`cart_max_speed` cuts a furnace minecart's).
    fn max_speed_new(&self, c: &Minecart) -> f64 {
        f64::from(
            self.session
                .rules
                .int_at(horizon_client::gamerules::vanilla::MAX_MINECART_SPEED),
        ) * if c.in_water { 0.5 } else { 1.0 }
            / 20.0
    }
    /// `positionRider` for the mobs in minecarts: each sits on the cart's
    /// passenger attachment, less its own vehicle attachment.
    pub(super) fn seat_cart_mobs(&mut self, riding: &super::passengers::Riders) {
        // The riders of an ECS cart sit where P puts them: nothing to seat here.
        if self.ecs.is_active() {
            return;
        }
        for i in 0..self.carts.len() {
            let id = self.carts[i].id;
            let Some(js) = riding.get(&id) else {
                continue;
            };
            let (pos, prev) = (self.carts[i].position, self.carts[i].previous);
            let cart_box = self.carts[i].bounds();
            let seats: Vec<(usize, f64, f64)> = js
                .iter()
                .map(|&j| {
                    let m = &self.remote_entities[j];
                    // Minecart's box is 0.98 wide.
                    let attachment = riders::vehicle_attachment(m, 0.98);
                    (j, attachment, cart_seat_offset(m.species(), attachment))
                })
                .collect();
            for (j, attachment, offset) in seats {
                let m = &mut self.remote_entities[j];
                m.seated(id, attachment, cart_box);
                m.position = cart_seat(pos, offset);
                m.previous = cart_seat(prev, offset);
                m.velocity = Vec3::ZERO;
                m.fall_distance = 0.0;
                m.on_ground = false;
                m.extra.passenger = true;
                m.ai.nav.stop();
            }
        }
    }
    /// The rider's tick: the cart moves with the rider's input as its push.
    pub(super) fn tick_cart_riding(&mut self, keys: Keys) -> bool {
        let Some(id) = self.riding.filter(|&id| self.ecs.cart_kind(id).is_some()) else {
            return false;
        };
        if keys.shift {
            self.dismount_cart(id);
            return true;
        }
        // ServerPlayer.getLastClientMoveIntent: getInputVector((left, 0, forward), 1, yRot).
        let left = f64::from(i8::from(keys.left) - i8::from(keys.right));
        let forward = f64::from(i8::from(keys.forward) - i8::from(keys.backward));
        let len = left.hypot(forward);
        let (left, forward) = if len > 1.0 {
            (left / len, forward / len)
        } else {
            (left, forward)
        };
        // The cart moves in P with this input as its push; the rider is seated where the cart is.
        if let Ok(player) = u32::try_from(self.own_entity_id()) {
            self.ecs.pilot_input(id, player, forward as f32, left as f32, self.yaw, self.pitch, 0.0, false, false, true);
        }
        let step = section_steps::seat_rider(self, id);
        section_steps::tell_player(self, step);
        true
    }
    /// `getDismountLocationForPassenger`: beside the cart's motion direction,
    /// else on top.
    pub(super) fn dismount_cart(&mut self, id: u32) {
        if self.riding == Some(id) && !self.native_may_leave(id) {
            return;
        }
        self.riding = None;
        let Some(at) = self.ecs_view(id).map(|v| Vec3::new(v.position[0], v.position[1], v.position[2])) else {
            return;
        };
        if self.dismount_in_portal(at) {
            return;
        }
        let target = self.cart_player_dismount_target(id);
        let from = self.player.as_ref().map(|player| player.position);
        if let Some(p) = self.player.as_mut() {
            p.position = target;
            p.velocity = Vec3::ZERO;
        }
        section_steps::tell_player(self, from.map(|from| (from, target)));
    }
    /// Where the cart at `i` lets a player off.
    pub(super) fn cart_player_dismount_target(&self, id: u32) -> Vec3 {
        let Some(v) = self.ecs_view(id) else {
            return Vec3::ZERO;
        };
        let [px, py, pz] = v.position;
        let (bx, by, bz) = (px.floor() as i32, py.floor() as i32, pz.floor() as i32);
        let mut target = None;
        let flipped = self.ecs.cart_flipped(id).unwrap_or(false);
        let face = cart_motion_face(v.yaw, flipped, self.minecart_improvements());
        'search: for dy in [0, 1, -1] {
            for (dx, dz) in dismount_offsets(face) {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                let below = self.res.facts().get(self.world.block(x, y - 1, z));
                let here = self.res.facts().get(self.world.block(x, y, z));
                let head = self.res.facts().get(self.world.block(x, y + 1, z));
                if (below.full_collision || below.legacy_solid)
                    && !here.full_collision
                    && !head.full_collision
                    && here.fluid == Fluid::None
                {
                    target = Some(Vec3::new(
                        f64::from(x) + 0.5,
                        f64::from(y),
                        f64::from(z) + 0.5,
                    ));
                    break 'search;
                }
            }
        }
        target.unwrap_or(Vec3::new(px, py + HEIGHT, pz))
    }
    /// `MinecartItem.useOn`: a cart on the clicked rail.
    pub(super) fn place_minecart(&mut self, hit: &Hit, stack: &ItemStack) -> bool {
        let Some(kind) = CartKind::from_id(&stack.item) else {
            return false;
        };
        let Some(rail) = self.rail(None, hit.pos[0], hit.pos[1], hit.pos[2]) else {
            return false;
        };
        let offset = if rail.shape.is_slope() { 0.5 } else { 0.0 };
        let at = Vec3::new(
            f64::from(hit.pos[0]) + 0.5,
            f64::from(hit.pos[1]) + 0.0625 + offset,
            f64::from(hit.pos[2]) + 0.5,
        );
        let mut next = self.next_entity;
        let spawned = self.ecs_with(|ecs, mw| {
            ecs.spawn_cart_id(mw, kind.id(), [at.x, at.y, at.z], 0.0, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        let Some(id) = spawned else {
            return false;
        };
        self.ecs.cart_start(id);
        self.world
            .modified
            .insert((hit.pos[0] >> 4, hit.pos[2] >> 4));
        self.apply_item_stack_config(super::commands::Target::Cart(id), kind.id(), stack, true);
        if self.serving.is_some() || self.options.dedicated.is_some() {
            self.native_vehicle_create(kind.id(), [at.x, at.y, at.z]);
        }
        // MinecartItem.useOn: ENTITY_PLACE at the rail, by the player, with the block under it.
        let below = self.world.block(hit.pos[0], hit.pos[1] - 1, hit.pos[2]);
        self.post_block_game_event(
            "minecraft:entity_place",
            hit.pos,
            self.sculk_actor(),
            Some(below),
        );
        if !self.creative {
            self.consume_held();
        }
        // ItemStack.useOn counts the use.
        self.award_stat("used", &stack.item, 1);
        self.swing_arm();
        true
    }
    /// The cart the aim enters first within reach, by its network id; the cart the player rides is passed over.
    pub(super) fn cart_hit(&self) -> Option<(u32, f64)> {
        let (eye, dir) = self.aim()?;
        self.ecs.aimed_cart_except(Vec3::new(eye[0], eye[1], eye[2]), Vec3::new(dir[0], dir[1], dir[2]), self.entity_reach, self.riding)
    }
    /// The slots of the cart `id`, for a command: `getChestVehicleItem` and
    /// `setChestVehicleItem` first unpack its loot table with no player, so
    /// no `player_generates_container_loot` fires later. While its menu is
    /// open the live slots are the placeholder copy (`open_cart_container`).
    pub(super) fn edit_cart_slots<R>(
        &mut self,
        id: u32,
        edit: impl FnOnce(&mut Vec<Option<ItemStack>>) -> R,
    ) -> Option<R> {
        self.ecs.cart_kind(id)?;
        self.unpack_loot(super::container_loot::LootTarget::Cart(id), None);
        match self
            .world
            .block_entities
            .get_mut(&(i32::MAX - 15, CART_KEY_Y, id as i32))
        {
            Some(BlockEntity::Chest(open)) => Some(edit(open)),
            _ => {
                let mut items = self.ecs.cart_items(id)?;
                let r = edit(&mut items);
                for (slot, stack) in items.into_iter().enumerate() {
                    self.ecs.cart_set_slot(id, slot, stack);
                }
                Some(r)
            }
        }
    }
    fn unpack_cart_loot(&mut self, id: u32) {
        self.unpack_loot(
            super::container_loot::LootTarget::Cart(id),
            self.native_acting_player(),
        );
    }
    /// `interact`: ride, open or fuel the cart under the crosshair, or the one
    /// a guest's interact packet named. A TNT cart has no interact of its own
    /// (`Entity.interact` passes), so nothing here lights it.
    pub(super) fn interact_cart(&mut self, target: Option<InteractTarget>) -> bool {
        let Some((id, d)) = (match target {
            Some(t) => u32::try_from(t.id).ok().and_then(|id| self.ecs.cart_kind(id).map(|_| (id, 0.0))),
            None => self.cart_hit(),
        }) else {
            return false;
        };
        // Only a client's own pick is stopped by a block: the range check passed.
        if target.is_none() && !self.before_block(d) {
            return false;
        }
        let Some(kind) = self.cart_genre(id) else {
            return false;
        };
        let sneaking = self.sneak_held;
        let held = self.held().cloned();
        match kind {
            CartKind::Rideable => {
                // isVehicle: a mob already sits in it. A player in another
                // vehicle may board: `Entity.startRiding` leaves the old one.
                let has_mob = !self.seated_passengers(id).is_empty();
                if sneaking || self.ridden_by_guest(id) || has_mob {
                    return false;
                }
                self.start_riding_target(
                    super::commands::Target::Player,
                    super::commands::Target::Cart(id),
                    false,
                );
            }
            // AbstractMinecartContainer.interact: no sneak check.
            CartKind::Chest | CartKind::Hopper => {
                let Some(player) = self.acting_player_uuid_id() else {
                    return false;
                };
                self.open_cart_container(id, player);
            }
            CartKind::Furnace => {
                let fuel = held
                    .as_ref()
                    .is_some_and(|s| matches!(&*s.item, "minecraft:coal" | "minecraft:charcoal"));
                let fueled = fuel && self.ecs.cart_feed(id);
                if fueled && !self.creative {
                    self.consume_held();
                }
            }
            // MinecartTNT has no interact of its own (`Entity.interact` passes).
            CartKind::Tnt => return false,
            // MinecartCommandBlock.interact: a game master gets SUCCESS (the
            // client opens the editor, `open_cart_command_block`), anyone else
            // PASSes to the held item. SUCCESS swings on the client only.
            CartKind::CommandBlock => return self.can_use_game_master_blocks(),
            // MinecartSpawner has no interact of its own (`Entity.interact` passes).
            CartKind::Spawner => return false,
        }
        self.swing_arm();
        true
    }
    /// `AbstractMinecartContainer.interact`: the menu opens over the cart's
    /// own slots, kept under a placeholder key until it closes
    /// (`store_cart_items`/`close_cart_container`).
    pub(super) fn open_cart_container(&mut self, id: u32, _player: u32) {
        self.unpack_cart_loot(id);
        let (Some(kind), Some(items), Some(at)) = (
            self.cart_genre(id),
            self.ecs.cart_items(id),
            self.ecs_view(id).map(|v| v.position),
        ) else {
            return;
        };
        let key = (i32::MAX - 15, CART_KEY_Y, id as i32);
        // `startOpen` shares one container, it does not reset it: a stand-in
        // another player's menu is still editing over stays as it is.
        if !self.world.block_entities.contains_key(&key) {
            self.world
                .block_entities
                .insert(key, BlockEntity::Chest(items));
        }
        self.container = Some(key);
        self.right = false;
        if kind == CartKind::Chest {
            // `MinecartChest.interact` (which overrides
            // `AbstractMinecartContainer.interact`): a successful open fires
            // CONTAINER_OPEN with the player and angers the piglins that see
            // it. `MinecartHopper` does not override it, so a hopper minecart
            // fires neither. Vanilla plays no open sound for a chest minecart.
            let ctx = self.player_context(None);
            self.post_game_event("minecraft:container_open", at, ctx);
            self.anger_piglins(true);
        }
    }
    /// Closing the cart's container open under `key` copies the items back.
    pub(super) fn close_cart_container(&mut self, key: (i32, i32, i32)) {
        if key.1 != CART_KEY_Y {
            return;
        }
        self.store_cart_items(key);
        let guest_viewers = self
            .lan
            .as_ref()
            .is_some_and(|host| Self::vehicle_has_other_viewers(None, &host.hub.guests, None, key));
        if !guest_viewers {
            self.world.block_entities.remove(&key);
        }
    }
    /// A chest or hopper minecart or a chest boat is about to be removed
    /// (`Containers.dropContents` / `AbstractChestBoat.remove`): what an open
    /// menu edited goes back to the vehicle, so it is what drops or travels
    /// with it, and the menu's stand-in leaves the level. A menu that is
    /// still open over it then has nothing left to take.
    pub(super) fn release_vehicle_container(&mut self, id: u32) {
        let key = (i32::MAX - 15, CART_KEY_Y, id as i32);
        self.store_cart_items(key);
        self.world.block_entities.remove(&key);
        if self.container == Some(key) {
            self.container = None;
        }
    }
    /// The menu of a chest or hopper minecart or a chest boat works on the
    /// vehicle's own items (`ContainerEntity`): the placeholder under `key`
    /// that the menu edits is copied back to the vehicle.
    pub(super) fn store_cart_items(&mut self, key: (i32, i32, i32)) {
        if key.1 != CART_KEY_Y {
            return;
        }
        let Some(BlockEntity::Chest(items)) = self.world.block_entities.get(&key) else {
            return;
        };
        let items = items.clone();
        let id = key.2 as u32;
        if self.ecs.cart_kind(id).is_some() {
            for (slot, stack) in items.into_iter().enumerate() {
                self.ecs.cart_set_slot(id, slot, stack);
            }
        } else {
            for (slot, stack) in items.into_iter().enumerate() {
                self.ecs.boat_set_slot(id, slot, stack);
            }
        }
    }
    /// The genre of the cart `id`, None when it is not a cart.
    pub(super) fn cart_genre(&self, id: u32) -> Option<CartKind> {
        CartKind::from_id(self.ecs.cart_kind(id)?)
    }
    /// The player's own blow on the cart `id` (`Player.attack`): a fire aspect lights it first. True when the cart was hurt.
    pub(super) fn cart_player_hurt(&mut self, id: u32, attacker: i32, damage: f32, fire: bool) -> bool {
        if fire {
            self.ecs.cart_ignite(id, None);
        }
        let creative = self.creative;
        let attacker = Some(CartAttacker::Player(attacker as u32));
        matches!(
            self.ecs_with(|ecs, mw| ecs.cart_hit(mw, id, damage, attacker, creative)),
            HitResult::Hurt
        )
    }
}

#[cfg(test)]
#[path = "minecarts/minecart_tnt_tests.rs"]
mod minecart_tnt_tests;

#[cfg(test)]
#[path = "minecarts/tests.rs"]
mod tests;

#[cfg(test)]
mod cart_push_tests {
    use super::{CartKind, Minecart};
    use horizon_client::{items::ItemStack, save::PendingContainerLoot};
    use horizon_core::geometry::Vec3;

}

#[cfg(test)]
mod hook_mutation_regressions {
    use super::*;
    use horizon_client::mobs::Mob;
    use std::sync::{Arc, Mutex};

    struct MutateRider {
        discard: bool,
        heard: Arc<Mutex<Vec<(u32, bool)>>>,
    }
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for MutateRider {
        fn entity_mount(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            self.heard
                .lock()
                .unwrap()
                .push((event.entity, event.mounting));
            if !event.mounting && event.entity == 72 {
                if self.discard {
                    assert!(host.entity(72).unwrap().discard(host));
                } else {
                    let later = host.entity(73).unwrap();
                    let replacement = host.entity(74).unwrap();
                    assert!(later.start_riding(host, replacement, true));
                }
            }
            mod_api::Flow::Continue
        }
    }
    struct DiscardDamagedVehicle(Arc<Mutex<usize>>);
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for DiscardDamagedVehicle {
        fn vehicle_damage(
            &self,
            host: &mut dyn mod_api::ServerHost,
            _: &mut mod_api::VehicleDamageEvent<'_>,
        ) -> mod_api::Flow {
            assert!(host.entity(71).unwrap().discard(host));
            mod_api::Flow::Continue
        }
        fn vehicle_destroy(
            &self,
            _: &mut dyn mod_api::ServerHost,
            _: &mut mod_api::VehicleDestroyEvent<'_>,
        ) {
            *self.0.lock().unwrap() += 1;
        }
    }
    #[test]
    #[ignore = "daily"]
    fn a_damage_callback_discard_does_not_also_destroy_or_drop_the_vehicle() {
        let destroyed = Arc::new(Mutex::new(0));
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
            "discard_damaged",
            Box::new(DiscardDamagedVehicle(destroyed.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            super::super::native_entity_refs::tests::with_game(|game| {
                game.options.dedicated = Some(Vec::new());
                game.session
                    .rules
                    .set("entity_drops", horizon_client::gamerules::Value::Bool(true));
                let boat = game.ecs_spawn("minecraft:oak_boat", [0.0; 3], None).unwrap();
                game.hurt_boat(boat, 5.0, false);
                assert_eq!(*destroyed.lock().unwrap(), 0);
                assert!(
                    game.dropped.is_empty(),
                    "discard must not create the vehicle item"
                );
                game.native_removals_flush();
                assert_eq!(*destroyed.lock().unwrap(), 0);
                assert!(game.dropped.is_empty());
                assert!(game.entity(boat).is_none());
            });
        });
    }
}

#[cfg(test)]
mod tnt_cart_power_tests {
    use super::*;

}

#[cfg(test)]
mod cart_cost_bench {
    use super::{CartKind, Minecart};
    use crate::game::game_tests::tests::server_game;
    use horizon_core::geometry::Vec3;

}

#[cfg(test)]
#[path = "minecarts/minecart_riders_tests.rs"]
mod minecart_riders_tests;
