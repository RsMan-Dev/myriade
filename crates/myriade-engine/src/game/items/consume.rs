//! Finishing a consumable (`Consumable.onConsume` and the use remainder),
//! its particles and sounds, and `TeleportRandomlyConsumeEffect` (chorus
//! fruit) for players and foxes. The level random stands in for the player's
//! own, as for every other player draw.
use super::{Dim, Game};
use myriade_types::{TransactionHost, TransactionSeal};
use myriade_engine::{
    items::{ConsumeEffect, ItemInfo, ItemStack},
    mobs::Who,
};
use horizon_core::{
    geometry::Vec3, position::BlockPos, random::LegacyRandom, random::RandomSource,
};
use std::convert::Infallible;
use {
    horizon_core::use_animation::UseAnimation,
    horizon_world::{
        consumable::{Consumable, EmissionWorld},
        teleport_consume::{TeleportConsume, TeleportConsumeWorld},
    },
};

type ItemConsumable = Consumable<String, ConsumeEffect>;

impl TransactionHost for Game<'_> {
    fn transaction_open(&mut self) -> TransactionSeal {
        Game::transaction_open(self)
    }
    fn transaction_close(&mut self, seal: &TransactionSeal, commit: bool) {
        Game::transaction_close(self, seal, commit)
    }
}

/// `EntityEvent.USE_ITEM_COMPLETE`: the server finished the player's use.
pub(super) const USE_ITEM_COMPLETE_EVENT: i8 = 9;

impl Game<'_> {
    /// `stack.get(DataComponents.CONSUMABLE)`: item prototype, data patch or wire patch.
    pub(super) fn consumable(&self, stack: &ItemStack) -> Option<ItemConsumable> {
        myriade_engine::item_sync::consumable(stack, self.items())
    }

    /// `ServerPlayer.completeUsingItem`: the player's own client is told
    /// first, and completes the use itself (eating the food there too).
    pub(super) fn send_use_item_complete(&mut self) {
        if self.lan_acting.is_some() {
            self.guest_entity_events.push(USE_ITEM_COMPLETE_EVENT);
        }
    }
    /// `Consumable.emitParticlesAndSounds(user.getRandom(), user, stack, count)`.
    pub(super) fn emit_consume(
        &mut self,
        consumable: &ItemConsumable,
        stack: &ItemStack,
        count: i32,
    ) {
        let Ok(()) = consumable.emit(&mut Emitter(self), (), stack.clone(), count);
    }
    /// `LivingEntity.completeUsingItem` for a consumable: `Consumable.onConsume`,
    /// then `ItemStack.applyAfterUseComponentSideEffects`' use remainder.
    pub(super) fn finish_consuming(
        &mut self,
        info: &ItemInfo,
        consumable: &ItemConsumable,
        stack: &ItemStack,
    ) {
        self.emit_consume(consumable, stack, 16);
        // ServerPlayer: the used statistic and the consume_item trigger.
        self.award_stat("used", &stack.item, 1);
        self.trigger_item("minecraft:consume_item", stack);
        // FoodProperties.onConsume, the player's one ConsumableListener.
        if let Some((nutrition, saturation)) = myriade_engine::item_sync::food(stack, info.food) {
            let at = self.feet();
            let pitch = self.world.random.triangle_float(1.0, 0.4);
            self.audio.play(&consumable.sound, at, 1.0, pitch);
            if let Some(p) = self.player.as_mut() {
                // FoodData.eat(nutrition, saturation)
                let food = &mut p.health.food;
                food.food = (food.food + nutrition as i32).clamp(0, 20);
                food.saturation = (food.saturation + saturation).clamp(0.0, food.food as f32);
            }
            // Mth.randomBetween(random, 0.9, 1.0)
            let pitch = self.world.random.next_float() * (1.0f32 - 0.9f32) + 0.9;
            self.audio
                .play("minecraft:entity.player.burp", self.feet(), 0.5, pitch);
        }
        // SuspiciousStewEffects.onConsume, a ConsumableListener: each entry is
        // added as a `MobEffectInstance(effect, duration)` (amplifier 0).
        if let Some(data) = stack.extra.as_deref() {
            for (id, duration) in &data.stew_effects {
                if let Some(def) = myriade_engine::brewing::effect(id) {
                    self.add_player_effect(myriade_engine::effects::EffectInstance::new(
                        def.id, *duration, 0,
                    ));
                }
            }
        }
        // Consumable.onConsume applies these entries on the logical server only;
        // the client's listener, particles and sounds above still run there.
        for effect in &consumable.effects {
            self.apply_consume_effect(effect, None);
        }
        let event = if consumable.animation == UseAnimation::Drink {
            "minecraft:drink"
        } else {
            "minecraft:eat"
        };
        self.post_entity_event(event, Who::Player);
        // ItemStack.consume, then UseRemainder.convertIntoRemainder: nothing
        // with infinite materials, the hand when emptied, else the inventory
        // (ServerPlayer.handleExtraItemsCreatedOnUse) or a drop.
        self.consume_held();
        let remainder = myriade_engine::item_sync::use_remainder(stack, info.use_remainder.clone());
        self.apply_use_remainder(remainder.as_deref(), stack.count);
    }
    /// `ItemStack.applyAfterUseComponentSideEffects` uses the stack captured
    /// before the item handler, even when that handler changes the hand.
    pub(super) fn apply_after_use_components(&mut self, stack: &ItemStack) {
        let remainder = self
            .items()
            .get(&stack.item)
            .and_then(|info| info.use_remainder.clone());
        let remainder = myriade_engine::item_sync::use_remainder(stack, remainder);
        self.apply_use_remainder(remainder.as_deref(), stack.count);
        self.apply_use_cooldown(stack);
    }
    fn apply_use_remainder(&mut self, remainder: Option<&str>, count_before: u32) {
        if self.creative || self.held().map_or(0, |stack| stack.count) >= count_before {
            return;
        }
        if let Some(remainder) = remainder {
            let remainder = ItemStack::new(remainder, 1);
            if self.held().is_none() {
                *self.held_mut() = Some(remainder);
            } else if let Some(rest) = self.add_stack_to_inventory(remainder) {
                self.player_drop(rest);
            }
        }
    }
    fn feet(&self) -> Option<[f64; 3]> {
        self.player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z])
    }
    /// `TeleportRandomlyConsumeEffect.apply` for the local player.
    pub(super) fn teleport_randomly(
        &mut self,
        teleport: &TeleportConsume,
    ) -> bool {
        if self.player.is_none() {
            return false;
        }
        let dim = self.dim;
        let Ok(result) = teleport.apply(&mut Teleporter(self), dim);
        result
    }
    /// `HostEvent::HeldConsume`: the item in the mouth of the ECS creature `e` is eaten now. The effects, the sounds and the remainder
    /// are the item's (host data); P owns the mouth, the clock and the stack, and is told at the end (`finish_held_use`).
    pub(in crate::game) fn ecs_held_consume_event(&mut self, e: bevy_ecs::entity::Entity, stack: ItemStack, seed: i64, silent: bool) {
        let mob = self.entity_id(e);
        self.apply_held_consumption_ecs(mob, &stack, seed, silent);
    }

    /// `apply_fox_consumption` for the ECS creature `mob`: the same steps, the mob side through the bridge. `seed` seeds the random of
    /// the sounds and of the teleport (P's own generator is not a `LegacyRandom`).
    fn apply_held_consumption_ecs(&mut self, mob: u32, stack: &ItemStack, seed: i64, _silent: bool) -> bool {
        if !self.ecs.held_use_valid(mob, stack, None) || self.ecs_view(mob).is_none() {
            return false;
        }
        let Some(consumable) = self.consumable(stack) else {
            return false;
        };
        let info = self.items().get(&stack.item);
        let food = myriade_engine::item_sync::food(stack, info.and_then(|i| i.food));
        let remainder = info.and_then(|i| i.use_remainder.as_deref()).map(std::sync::Arc::<str>::from);
        let remainder = myriade_engine::item_sync::use_remainder(stack, remainder.map(|value| value.to_string()));
        let dim = self.dim;
        let result: Result<bool, ()> = myriade_types::run_transaction(self, |game, _| {
            let Some(at) = game.ecs.position(mob) else {
                return Ok(false);
            };
            let mut rng = LegacyRandom::new(seed);
            // `emitParticlesAndSounds(random, user, stack, 16)`, then `FoodProperties.onConsume`'s draw for the sound's pitch (its sound is not played: no mob events).
            let _ = consumable.emit(&mut HeldEater { rng: &mut rng }, (), stack.clone(), 16);
            if food.is_some() {
                let _ = rng.triangle_float(1.0, 0.4);
            }
            for effect in &consumable.effects {
                if let ConsumeEffect::TeleportRandomly(teleport) = effect {
                    let mut actor = EcsHeldTeleporter { game, mob, stack, from: Vec3::new(at[0], at[1], at[2]), rng: &mut rng, dim };
                    if teleport.apply(&mut actor, dim).is_err() {
                        return Ok(false);
                    }
                } else if game.apply_consume_effect(effect, Some(mob)).is_none() {
                    return Ok(false);
                }
            }
            let remainder = remainder.clone().map(|name| ItemStack::new(&name, 1));
            Ok(game.ecs.finish_held_use(mob, stack, remainder))
        });
        result.unwrap_or(false)
    }

    /// `LivingEntity.randomTeleport(x, y, z, true, avoid)`: down to the first
    /// block entities can teleport onto, inside the border, in a loaded chunk.
    pub(super) fn random_teleport(&mut self, target: Vec3, avoid: Option<&str>) -> bool {
        let [min_x, min_z, max_x, max_z] = self.border.bounds();
        // WorldBorder.clampVec3ToBound (Mth.clamp, 1.0E-5F inside the far edge).
        let clamp = |v: f64, low: f64, high: f64| if v < low { low } else { v.min(high) };
        let x = clamp(target.x, min_x, max_x - f64::from(1.0e-5_f32));
        let z = clamp(target.z, min_z, max_z - f64::from(1.0e-5_f32));
        let mut y = target.y;
        let mut pos = BlockPos::containing(Vec3::new(x, y, z));
        if !self.world.loaded(pos.x, pos.z) {
            return false;
        }
        while pos.y > self.world.shape.min_y {
            pos.y -= 1;
            let state = self.world.block(pos.x, pos.y, pos.z);
            if self.state_in_tag("minecraft:entities_can_teleport_to", state) {
                return self.check_position_and_teleport(Vec3::new(x, y, z), state, avoid);
            }
            y -= 1.0;
        }
        false
    }
    /// `LivingEntity.checkPositionAndTeleport`, then `ServerPlayer.teleportTo`
    /// (the motion and rotation kept) and entity event 46.
    fn check_position_and_teleport(
        &mut self,
        at: Vec3,
        ground: horizon_world::registry::BlockStateId,
        avoid: Option<&str>,
    ) -> bool {
        if avoid.is_some_and(|tag| self.state_in_tag(tag, ground)) {
            return false;
        }
        let entities = self.player_entity_collisions();
        let Some(player) = self.player.as_mut() else {
            return false;
        };
        let bounds = player.dimensions.bounds(at);
        let free = myriade_engine::motion::MotionWorld {
            entities: &entities,
            player,
            scene: &mut self.world,
            assets: &self.pack.assets,
            collision_binding: self.collision,
            resources: self.motion,
            health: &self.health_resources,
            loop_context: &self.loop_context,
            damage_hooks: &self.damage_hooks,
            tag_items: Some(world_items!(self)),
            block_registries: Some(&self.pack.block_registries),
        }
        .no_player_collision(bounds)
        .unwrap_or(false);
        // LevelReader.containsAnyLiquid and findBlocksIn: the blocks from the
        // box's minimum corner to the one holding its maximum corner.
        let (lo, hi) = (
            BlockPos::containing(bounds.min_position()),
            BlockPos::containing(bounds.max_position()),
        );
        let blocks = (lo.x..=hi.x)
            .flat_map(|x| (lo.y..=hi.y).flat_map(move |y| (lo.z..=hi.z).map(move |z| (x, y, z))));
        let states: Vec<_> = blocks.map(|(x, y, z)| self.world.block(x, y, z)).collect();
        let liquid = states
            .iter()
            .any(|&s| self.res.facts().get(s).fluid != myriade_engine::blocks::Fluid::None);
        if !free
            || liquid
            || avoid.is_some_and(|tag| states.iter().any(|&s| self.state_in_tag(tag, s)))
        {
            return false;
        }
        if let Some(p) = self.player.as_mut() {
            p.position = at;
        }
        // The position packet moves the old position along (absolute xyz).
        self.previous_position = at;
        // `broadcastEntityEvent(46)` after the teleport.
        if let Some(id) = self.lan_acting_entity {
            self.lan_entity_event(id as i32, 46, [at.x, at.y, at.z]);
        }
        true
    }
    fn state_in_tag(&self, tag: &str, state: horizon_world::registry::BlockStateId) -> bool {
        let name = &self.res.facts().get(state).name;
        self.res
            .items()
            .block_tags
            .get(tag)
            .is_some_and(|s| s.contains(&**name))
    }
    /// `LivingEntity.stopRiding`.
    pub(super) fn stop_riding(&mut self) {
        let Some(id) = self.riding else {
            return;
        };
        // RESTE-SANS-ECS: `removed_vehicles` is retired; a removed vehicle's last height is no longer kept.
        let removed_y = None::<f64>.or_else(|| {
                use super::commands::Target;
                let boat = Target::Boat(id);
                let cart = Target::Cart(id);
                if self.persist.removals.contains(&boat)
                    || self.persist.destroying_vehicles.contains(&boat)
                {
                    self.ecs_view(id).map(|v| v.position[1])
                } else if self.persist.removals.contains(&cart)
                    || self.persist.destroying_vehicles.contains(&cart)
                {
                    self.ecs_view(id).map(|v| v.position[1])
                } else {
                    None
                }
            });
        if let Some(y) = removed_y {
            if self.riding == Some(id) {
                self.dismount_removed_vehicle(y);
            }
            return;
        }
        if self.ecs.boat_status(id).is_some() {
            self.dismount_boat(id);
        } else if self.ecs.cart_kind(id).is_some() {
            self.dismount_cart(id);
        } else if let Some(c) = self.cushion_saved(id) {
            self.get_up_from_cushion(&c);
        } else if self.seat(id).is_some() {
            self.get_off_seat();
        } else {
            self.dismount_mount();
        }
    }
}

/// `Consumable.emitParticlesAndSounds` against the local player.
struct Emitter<'a, 'g>(&'a mut Game<'g>);
impl EmissionWorld for Emitter<'_, '_> {
    type Error = Infallible;
    type Stack = ItemStack;
    type Sound = String;
    type Random = ();
    type Source = LegacyRandom;
    fn random_source(&mut self, (): ()) -> &mut LegacyRandom {
        &mut self.0.world.random
    }
    /// `LivingEntity.spawnItemParticles`.
    fn spawn_item_particles(&mut self, stack: ItemStack, count: i32) -> Result<(), Infallible> {
        self.0.eat_particles(&stack, count.max(0) as u32);
        Ok(())
    }
    fn overrides_consume_sound(&self) -> bool {
        false
    }
    fn consume_sound(&mut self, _: ItemStack) -> Result<String, Infallible> {
        unreachable!("players play the consumable's own sound")
    }
    /// `Player.playSound`: at the player's position, for everyone but the
    /// player, whose own client plays it (`level.playSound(player, ...)`):
    /// a guest's server turn leaves the guest out.
    fn play_sound(&mut self, sound: String, volume: f32, pitch: f32) -> Result<(), Infallible> {
        let at = self.0.feet();
        if self.0.lan_acting.is_some() {
            if let Some(at) = at {
                self.0.guest_player_sounds.push((sound, at, volume, pitch));
            }
        } else {
            self.0.audio.play(&sound, at, volume, pitch);
        }
        Ok(())
    }
}

/// The local player as `TeleportRandomlyConsumeEffect`'s user.
struct Teleporter<'a, 'g>(&'a mut Game<'g>);
impl Teleporter<'_, '_> {
    fn at(&self) -> Vec3 {
        self.0.player.as_ref().map_or(Vec3::ZERO, |p| p.position)
    }
}
impl TeleportConsumeWorld for Teleporter<'_, '_> {
    type Error = Infallible;
    type Level = Dim;
    type Random = ();
    type Source = LegacyRandom;
    fn position_field(&self) -> Vec3 {
        self.at()
    }
    fn get_random(&mut self) -> Result<(), Infallible> {
        Ok(())
    }
    fn random_source(&mut self, (): ()) -> &mut LegacyRandom {
        &mut self.0.world.random
    }
    fn min_y(&mut self, level: Dim) -> Result<i32, Infallible> {
        Ok(self.0.level_shapes.get(level.id()).min_y)
    }
    fn logical_height(&mut self, level: Dim) -> Result<i32, Infallible> {
        Ok(self.0.level_shapes.get(level.id()).logical_height)
    }
    fn is_passenger(&mut self) -> Result<bool, Infallible> {
        Ok(self.0.riding.is_some())
    }
    fn stop_riding(&mut self) -> Result<(), Infallible> {
        self.0.stop_riding();
        Ok(())
    }
    fn position(&mut self) -> Result<Vec3, Infallible> {
        Ok(self.at())
    }
    /// Without a hook asking, the spot is the one vanilla drew.
    fn teleport_allowed(&mut self, _target: &mut Vec3) -> Result<bool, Infallible> {
        Ok(true)
    }
    fn random_teleport(&mut self, target: Vec3) -> Result<bool, Infallible> {
        Ok(self
            .0
            .random_teleport(target, Some("minecraft:consumable_does_not_teleport_to")))
    }
    fn teleport_event(&mut self, _: Dim, old: Vec3) -> Result<(), Infallible> {
        let ctx = self.0.who_context(Some(Who::Player), None);
        self.0
            .post_game_event("minecraft:teleport", [old.x, old.y, old.z], ctx);
        Ok(())
    }
    fn is_fox(&self) -> bool {
        false
    }
    fn teleport_sound(&mut self, _: Dim, at: Vec3, _: bool) -> Result<(), Infallible> {
        self.0.audio.play(
            "minecraft:item.chorus_fruit.teleport",
            Some([at.x, at.y, at.z]),
            1.0,
            1.0,
        );
        Ok(())
    }
    fn block_position(&mut self) -> Result<BlockPos, Infallible> {
        Ok(BlockPos::containing(self.at()))
    }
    fn directional_particles(
        &mut self,
        _: Dim,
        origin: BlockPos,
        offset: i32,
    ) -> Result<(), Infallible> {
        self.0
            .level_event_at(2017, [origin.x, origin.y, origin.z], offset);
        Ok(())
    }
    fn reset_fall_distance(&mut self) -> Result<(), Infallible> {
        if let Some(p) = self.0.player.as_mut() {
            p.fall_distance = 0.0;
        }
        Ok(())
    }
    fn reset_impulse(&mut self) -> Result<(), Infallible> {
        if let Some(p) = self.0.player.as_mut() {
            p.impulse.reset();
        }
        Ok(())
    }
}

/// `FoxTeleporter` for an ECS creature that eats: the position, the riding and the fall come from the bridge, the random is seeded by P.
struct EcsHeldTeleporter<'a, 'g> {
    game: &'a mut Game<'g>,
    mob: u32,
    stack: &'a ItemStack,
    from: Vec3,
    rng: &'a mut LegacyRandom,
    dim: Dim,
}
impl EcsHeldTeleporter<'_, '_> {
    fn at(&self) -> Result<Vec3, ()> {
        self.game.ecs.held_use_valid(self.mob, self.stack, None).then(|| self.game.ecs.position(self.mob)).flatten().map(|p| Vec3::new(p[0], p[1], p[2])).ok_or(())
    }
}
impl TeleportConsumeWorld for EcsHeldTeleporter<'_, '_> {
    type Error = ();
    type Level = Dim;
    type Random = ();
    type Source = LegacyRandom;
    fn position_field(&self) -> Vec3 {
        self.at().unwrap_or(self.from)
    }
    fn get_random(&mut self) -> Result<(), ()> {
        self.at()?;
        Ok(())
    }
    fn random_source(&mut self, (): ()) -> &mut LegacyRandom {
        self.rng
    }
    fn min_y(&mut self, _: Dim) -> Result<i32, ()> {
        Ok(self.game.world.shape.min_y)
    }
    fn logical_height(&mut self, _: Dim) -> Result<i32, ()> {
        Ok(self.game.world.shape.logical_height)
    }
    fn is_passenger(&mut self) -> Result<bool, ()> {
        Ok(self.game.ecs_vehicle_of(self.mob).is_some())
    }
    fn stop_riding(&mut self) -> Result<(), ()> {
        self.game.ecs_dismount(self.mob);
        Ok(())
    }
    fn position(&mut self) -> Result<Vec3, ()> {
        self.at()
    }
    fn teleport_allowed(&mut self, target: &mut Vec3) -> Result<bool, ()> {
        let from = self.at()?;
        if self.game.dim != self.dim
            || !self.game.ecs.held_use_valid(self.mob, self.stack, Some([from.x, from.y, from.z]))
        {
            return Err(());
        }
        if target.is_finite() {
            let shape = self.game.world.shape;
            let high = shape.min_y + shape.logical_height - 1;
            target.y = target.y.clamp(f64::from(shape.min_y), f64::from(high));
        }
        Ok(true)
    }
    fn random_teleport(&mut self, at: Vec3) -> Result<bool, ()> {
        let mob = self.mob;
        Ok(self.game.ecs.is_active()
            && self.game.ecs_with(|ecs, mw| ecs.random_teleport(mw, mob, at, Some("minecraft:consumable_does_not_teleport_to"))))
    }
    fn teleport_event(&mut self, _: Dim, old: Vec3) -> Result<(), ()> {
        let ctx = self.game.who_context(Some(Who::Mob(self.mob)), None);
        self.game.post_game_event("minecraft:teleport", [old.x, old.y, old.z], ctx);
        Ok(())
    }
    fn is_fox(&self) -> bool {
        true
    }
    fn teleport_sound(&mut self, _: Dim, at: Vec3, _: bool) -> Result<(), ()> {
        self.game.audio.play("minecraft:entity.fox.teleport", Some([at.x, at.y, at.z]), 1.0, 1.0);
        Ok(())
    }
    fn block_position(&mut self) -> Result<BlockPos, ()> {
        Ok(BlockPos::containing(self.at()?))
    }
    fn directional_particles(&mut self, _: Dim, origin: BlockPos, offset: i32) -> Result<(), ()> {
        self.game.level_event_at(2017, [origin.x, origin.y, origin.z], offset);
        Ok(())
    }
    fn reset_fall_distance(&mut self) -> Result<(), ()> {
        let e = self.game.ecs.entity_of(self.mob).ok_or(())?;
        self.game.ecs.set_fall_distance(e, 0.0);
        Ok(())
    }
    fn reset_impulse(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

/// The eating creature as `Consumable`'s user: a random seeded by P, and its own ears (a silent one is not heard).
struct HeldEater<'a> {
    rng: &'a mut LegacyRandom,
}
impl EmissionWorld for HeldEater<'_> {
    type Error = Infallible;
    type Stack = ItemStack;
    type Sound = String;
    type Random = ();
    type Source = LegacyRandom;
    fn random_source(&mut self, (): ()) -> &mut LegacyRandom {
        self.rng
    }
    /// A server does nothing with the crumbs (the clients throw them on entity event 45).
    fn spawn_item_particles(&mut self, _: ItemStack, _: i32) -> Result<(), Infallible> {
        Ok(())
    }
    fn overrides_consume_sound(&self) -> bool {
        false
    }
    fn consume_sound(&mut self, _: ItemStack) -> Result<String, Infallible> {
        unreachable!("the consumable plays its own sound")
    }
    /// `Entity.playSound` of the creature: not played, the mob events being gone.
    fn play_sound(&mut self, _: String, _: f32, _: f32) -> Result<(), Infallible> {
        Ok(())
    }
}
