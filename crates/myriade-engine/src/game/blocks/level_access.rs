//! The world behaviour adapter, kept beside the block operations it serves.
use super::*;

impl LevelAccess for GameLevel<'_> {
    fn infiniburn(&self) -> Option<&horizon_world::level::data::BlockSet> {
        self.world.infiniburn.as_deref()
    }
    fn cake_can_eat(&self, vanilla: bool) -> bool {
        if self.cake_probe {
            self.cake_requested.set(true);
            false
        } else {
            self.cake_answer.unwrap_or(vanilla)
        }
    }

    fn within_world_border(&self, pos: BlockPos) -> bool {
        self.border
            .is_within_bounds(f64::from(pos.x), f64::from(pos.z), 0.0)
    }
    fn redstone_experiments(&self) -> bool {
        self.redstone_experiments
    }
    fn has_native_block_entity(&self, state: BlockStateId) -> bool {
        self.world.block_entities.stateful_entity(state.0)
    }
    fn allow_piston_move(
        &self,
        piston: BlockPos,
        direction: horizon_core::geometry::Direction,
        extending: bool,
        push: &[BlockPos],
        destroy: &[BlockPos],
    ) -> bool {
        self.piston_move_allowed(piston, direction, extending, push, destroy)
    }
    fn observe_piston_moved(
        &self,
        piston: BlockPos,
        direction: horizon_core::geometry::Direction,
        extending: bool,
        push: &[BlockPos],
        destroy: &[BlockPos],
    ) {
        self.piston_moved(piston, direction, extending, push, destroy);
    }
    fn adjust_analog_output(
        &self,
        _pos: BlockPos,
        _state: BlockStateId,
        _direction: Direction,
        vanilla: i32,
    ) -> i32 {
        vanilla.clamp(0, 15)
    }
    fn is_client_side(&self) -> bool {
        self.client_side
    }
    fn world_surface(&self, x: i32, z: i32) -> i32 {
        self.world.world_surface(x, z, |s| self.facts.is_air(s))
    }
    fn precipitation(&self, pos: BlockPos) -> Option<horizon_world::level::Precipitation> {
        self.behaviours.precipitation_info(self, pos)
    }
    fn is_raining_at(&self, pos: BlockPos) -> bool {
        self.behaviours.is_raining_at(self, pos)
    }
    /// `getEntitiesWithContainerOpen` is not empty: a player or guest has the
    /// container open, or a copper golem has the chest open
    /// (`CopperGolem.hasContainerOpen`).
    fn container_open(&self, pos: BlockPos) -> bool {
        let key = (pos.x, pos.y, pos.z);
        self.open_containers.contains(&key) || self.golem_chests.contains(&key)
    }
    /// `ContainerOpenersCounter.recheckOpeners` for the block entity at `pos`,
    /// through `ChestBlock.tick`, `EnderChestBlock.tick` and `BarrelBlock.tick`:
    /// the count a recount of the players with that container open finds, and
    /// the count it replaces. Only the block entities that keep a counter and
    /// a scheduled recheck answer; anything else is `None`.
    fn recheck_container_openers(&mut self, pos: BlockPos) -> Option<(i32, i32)> {
        use horizon_world::level::classes::Class;
        let data = self.behaviours.data(self.world.block(pos.x, pos.y, pos.z));
        if !(data.is(Class::Chest) || data.is(Class::EnderChest) || data.is(Class::Barrel)) {
            return None;
        }
        Some(crate::game::block_entity_anims::recheck_opener_count(
            self.lid_openers,
            self.open_containers,
            (pos.x, pos.y, pos.z),
        ))
    }

    fn block_entity_analog_signal(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Direction,
    ) -> i32 {
        if let Some(s) = self.registry.state(state)
            && s.name == "minecraft:detector_rail"
        {
            return self.detector_rail_signal(
                pos,
                s.properties.get("powered").is_some_and(|v| v == "true"),
            );
        }
        if let Some(signal) = command_blocks::analog_signal(self.world, (pos.x, pos.y, pos.z)) {
            return signal;
        }
        if let Some(signal) = sculk::sensor_signal(self.world, self.behaviours, state, pos) {
            return signal;
        }
        if let Some((_, comparator)) = jukebox::jukebox_signals(self.world, self.items, pos) {
            return comparator;
        }
        if let Some(signal) = creaking::heart_signal(self.world, pos) {
            return if self.registry.state(state).is_some_and(|s| {
                s.properties
                    .get("creaking_heart_state")
                    .is_some_and(|v| v == "uprooted")
            }) {
                0
            } else {
                signal
            };
        }
        if let Some(signal) = lectern_signal(self.world, self.registry, state, pos) {
            return signal;
        }
        if let Some(signal) =
            machines::shelf_signal(self.world, self.registry, state, pos, direction)
        {
            return signal;
        }
        machines::container_signal(self, pos)
    }
    fn item_frame_analog_output(&self, pos: BlockPos, facing: Direction) -> Option<i32> {
        // Only a single frame in the cell counts; `(has item, rotation)` per frame.
        let frames = self.ecs.frames_at([pos.x, pos.y, pos.z], facing as u8);
        let &[(has_item, rotation)] = frames.as_slice() else {
            return None;
        };
        // ItemFrame.getAnalogOutput.
        Some(if has_item { i32::from(rotation % 8) + 1 } else { 0 })
    }
    fn block_entity_signal(
        &self,
        pos: BlockPos,
        _state: BlockStateId,
        _direction: Direction,
    ) -> i32 {
        if let Some(count) = self.lid_openers.get(&(pos.x, pos.y, pos.z)) {
            return (*count).clamp(0, 15);
        }
        jukebox::jukebox_signals(self.world, self.items, pos).map_or(0, |(signal, _)| signal)
    }
    fn biome_name(&self, pos: BlockPos) -> Option<&str> {
        let chunk = self.world.chunks.get(&(pos.x >> 4, pos.z >> 4))?;
        Some(&self.biomes.get(chunk.biome(pos.x, pos.y, pos.z)).name)
    }
    fn world_seed(&self) -> i64 {
        self.seed
    }
    fn day_time(&self) -> i64 {
        self.world.day_time
    }
    fn game_rules(&self) -> horizon_world::level::GameRules {
        self.rules
    }
    fn difficulty(&self) -> i32 {
        self.difficulty
    }
    /// `ServerLevel.canSpreadFireAround`: a player (not a spectator) closer
    /// than the `fire_spread_radius_around_player` rule to the block's corner
    /// (`ChunkMap.anyPlayerCloseEnoughTo`); -1 spreads everywhere.
    fn can_spread_fire_around(&self, pos: BlockPos) -> bool {
        self.player_near_fire(pos)
    }
    fn increased_fire_burnout(&self, pos: BlockPos) -> bool {
        self.biome_burns_out_fire(pos)
    }
    fn block_state(&self, pos: BlockPos) -> BlockStateId {
        self.world.block(pos.x, pos.y, pos.z)
    }
    fn set_raw(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
        // `Level.setBlock`: a server level of the Debug preset never changes.
        if self.debug && !self.client_side {
            return None;
        }
        if !self.world.loaded(pos.x, pos.z)
            || pos.y < self.world.bottom_y()
            || pos.y >= self.world.top_y()
        {
            return None;
        }
        let previous = self.world.block(pos.x, pos.y, pos.z);
        self.world
            .set_with_notification(pos.x, pos.y, pos.z, state, false)
            .ok()?;
        Some(previous)
    }
    fn send_block_updated(&mut self, pos: BlockPos) {
        if (self.client_side || self.tickets.block_ticking(pos.x >> 4, pos.z >> 4))
            && let Some(log) = &mut self.world.block_log
        {
            log.push((pos.x, pos.y, pos.z));
        }
        // Vanilla's full chunk status, not the stricter simulation-ticket gate
        // used by the existing network log above. Independent of log presence.
        if !self.client_side && self.tickets.loaded_ticking(pos.x >> 4, pos.z >> 4) {
            self.world.path_type_block_updated(pos);
        }
    }
    fn is_loaded(&self, pos: BlockPos) -> bool {
        self.world.loaded(pos.x, pos.z)
    }
    fn min_y(&self) -> i32 {
        self.world.shape.min_y
    }
    fn max_y(&self) -> i32 {
        // Exclusive, as LevelAccess::max_y documents.
        self.world.shape.max_y()
    }
    fn random(&mut self) -> &mut horizon_core::random::LegacyRandom {
        &mut self.world.random
    }
    fn game_time(&self) -> i64 {
        self.world.game_time
    }
    fn block_light(&self, pos: BlockPos) -> i32 {
        i32::from(self.world.light(pos.x, pos.y, pos.z) & 15)
    }
    fn sky_light(&self, pos: BlockPos) -> i32 {
        i32::from(self.world.light(pos.x, pos.y, pos.z) >> 4)
    }
    fn sky_darken(&self) -> i32 {
        self.darken
    }
    fn sun_angle(&self, _pos: BlockPos) -> f32 {
        self.environment
            .sample(&myriade_engine::environment::Sample {
                day_time: self.world.day_time,
                ..Default::default()
            })
            .sun_angle
    }
    fn is_raining(&self) -> bool {
        self.raining
    }
    fn is_thundering(&self) -> bool {
        self.thundering
    }
    fn dimension(&self) -> LevelDimension {
        self.dimension
    }
    /// `Level.isUnobstructed`: the placed state's collision shape against the player.
    /// `Level.isUnobstructed(state, pos, CollisionContext.of(player))`: no
    /// entity that blocks building is in the state's collision shape.
    fn is_unobstructed(&self, pos: BlockPos, state: BlockStateId) -> bool {
        let player = self.player_collision.as_ref();
        let context = player.map_or_else(
            || CollisionContext::empty(false),
            |player| {
                CollisionContext::for_entity(
                    CollisionEntity {
                        identity: EntityIdentity(0),
                        kind: horizon_world::collision_context::EntityContextKind::Player,
                        y: player.y,
                        descending: player.descending,
                        main_hand: Some(CollisionItem {
                            item: &player.main_hand_item,
                            count: player.main_hand_count,
                        }),
                    },
                    false,
                )
            },
        );
        let mut scene = collisions::SceneCollisions {
            world: &*self.world,
            binding: self.collision,
            subject: player.map(|player| myriade_engine::collisions::PowderSnowSubject {
                fall_distance: player.fall_distance,
                leather_boots: player.leather_boots,
                ..Default::default()
            }),
        };
        let Ok(shape) = self
            .collision
            .collision_shape_with_context(&mut scene, state, pos, &context)
        else {
            return false;
        };
        if shape.is_empty() {
            return true;
        }
        let shape = shape.moved(horizon_core::geometry::Vec3::new(
            f64::from(pos.x),
            f64::from(pos.y),
            f64::from(pos.z),
        ));
        let Some(zone) = shape.bounds() else {
            return true;
        };
        horizon_world::entity_obstruction::is_unobstructed(
            &mut BuildingObstruction(self.building_blocker_boxes(zone)),
            None,
            &shape,
        )
        .unwrap_or(false)
    }
    /// `ServerLevel.shouldTickBlocksAt`: only the block-ticking ticket range.
    fn should_tick_blocks_at(&self, chunk: horizon_core::position::ChunkPos) -> bool {
        self.tickets.block_ticking(chunk.x, chunk.z)
    }
    /// `ServerLevel.isPositionTickingWithEntitiesLoaded`: block ticking, a
    /// ticking chunk, and its entities loaded.
    fn is_ticking_chunk(&self, chunk: horizon_core::position::ChunkPos) -> bool {
        self.tickets.block_ticking(chunk.x, chunk.z)
            && ticking_chunk(self.world, chunk.x, chunk.z)
            && !self.world.entities_pending.contains(&(chunk.x, chunk.z))
    }
    /// Entities that trigger pressure plates, tripwires, detector rails and
    /// wooden buttons: the players (the host and the LAN guests), the mobs,
    /// the minecarts and the other entities that don't ignore block triggers,
    /// and for `EntityQuery::Arrows` the arrows.
    fn count_entities(&self, area: Aabb, query: EntityQuery) -> usize {
        let hit = |(lo, hi): &([f64; 3], [f64; 3])| {
            area.intersects(Aabb::new(
                Vec3::new(lo[0], lo[1], lo[2]),
                Vec3::new(hi[0], hi[1], hi[2]),
            ))
        };
        let carts = || self.ecs.cart_bounds_in(area).len();
        let others = || {
            self.ecs
                .species_bounds("minecraft:end_crystal")
                .into_iter()
                .chain(self.trigger_extras.iter().copied())
                .filter(|b| area.intersects(*b))
                .count()
        };
        // Arrows and tridents (`AbstractArrow`): the ECS projectiles that stick.
        let arrows = || self.ecs.count_arrows_in(area);
        let players = || {
            self.player_box
                .iter()
                .chain(&self.guest_boxes)
                .filter(|b| hit(b))
                .count()
        };
        match query {
            EntityQuery::Minecart => carts(),
            EntityQuery::Arrows => arrows(),
            EntityQuery::Mobs => players() + self.ecs.count_triggers_in(area, true),
            EntityQuery::Everything => players() + self.ecs.count_triggers_in(area, false) + others(),
        }
    }
    fn emit(&mut self, event: LevelEvent) {
        match event {
            // Block.dropResources draws its loot and spots right away.
            // `popResource` only drops server-side (`!level.isClientSide`):
            // a client's moveBlocks (a piston's block event) skips it.
            LevelEvent::DropResources { pos, state } if !self.client_side => {
                self.drop_resources_now(pos, state)
            }
            LevelEvent::DropResources { .. } => {}
            // `ClientLevel.playSeededSound` plays only when the excluded
            // player is this client's own: the block's `playSound(null, ...)`
            // (a piston's move, a note) reaches it from the server instead.
            LevelEvent::Sound { .. } if self.client_side => {}
            // NoteBlock.triggerEvent plays the head's sound with a null
            // player, so only the server's level plays it; a guest or a
            // remote client hears it from the sound packet.
            LevelEvent::BlockEvent {
                kind: "note_custom_sound",
                ..
            } if self.client_side => {}
            // Block.popResource of the block's own item (tall flowers, flower beds).
            LevelEvent::BlockEvent {
                pos,
                state,
                kind: "pop_block_item",
            } => self.pop_block_item(pos, state),
            // ComposterBlock.extractProduce: bone meal above the composter.
            LevelEvent::Player {
                pos,
                effect: PlayerEffect::DropItem(item),
                ..
            } => self.pop_above(pos, item),
            // Picking a sweet berry bush or cave vines: the harvest loot,
            // popped before the pick sound draws its pitch.
            LevelEvent::Player {
                pos,
                state,
                effect: PlayerEffect::InteractLoot(table),
            } => self.drop_interact_loot(pos, state, table),
            event => self.events.push(event),
        }
    }
    fn adjust_piston_carries_block_entity(
        &self,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Option<bool> {
        self.piston_carries_block_entity(pos, state)
    }
    fn carry_block_entity(&mut self, from: BlockPos, to: BlockPos) {
        self.carry_mod_block_entity(from, to);
    }
    fn land_block_entity(&mut self, pos: BlockPos) -> bool {
        self.land_carried_block_entity(pos)
    }
    fn discard_carried_block_entity(&mut self, pos: BlockPos) {
        self.world
            .block_entities
            .discard_carried(&(pos.x, pos.y, pos.z));
    }
    /// `ShulkerBoxBlockEntity.isClosed`: the lid state the block-entity tick
    /// copied from `be_anims` (`ShulkerBoxBlock.getBlockSupportShape`).
    fn shulker_is_closed(&self, pos: BlockPos) -> Option<bool> {
        self.world
            .shulkers
            .get(&[pos.x, pos.y, pos.z])
            .map(|s| s.status == myriade_engine::block_entity_anim::ShulkerStatus::Closed)
    }
    fn remove_block_entity(&mut self, pos: BlockPos, old: BlockStateId, side_effects: bool) -> u8 {
        if let Some(change) = &mut self.command_block_transition
            && change.preserve(self.world, pos, old)
        {
            // Returning 0 skips remove_entity's side effects once; command blocks have no contents or drops.
            return 0;
        }
        self.remove_entity(pos, old, side_effects)
    }
    fn load_block_entity(&mut self, pos: BlockPos, mut nbt: horizon_core::nbt::Compound) {
        use horizon_core::nbt::Tag;
        for (k, v) in [("x", pos.x), ("y", pos.y), ("z", pos.z)] {
            nbt.insert(k.into(), Tag::Int(v));
        }
        let block = self.world.block(pos.x, pos.y, pos.z);
        let Some(block) = self.registry.state(block) else {
            return;
        };
        let name = &block.name;
        let tag = Tag::Compound(nbt);
        if let Some((key, entity)) = myriade_engine::save::decode_block_entity_for_block(&tag, name)
        {
            self.world.block_entities.insert(key, entity);
            if tag.get("LootTable").is_some() {
                self.emit(LevelEvent::ContainerLootTable { pos, tag });
            }
        }
    }
    fn create_feature_spawner(&mut self, pos: BlockPos, nbt: horizon_core::nbt::Compound) {
        // Guard at the owner/insert boundary as well as the live feature sink.
        // Existing entries of every class retain the prior no-op behavior.
        if self
            .registry
            .state(self.world.block(pos.x, pos.y, pos.z))
            .is_none_or(|state| state.name != "minecraft:spawner")
            || self
                .world
                .block_entities
                .contains_key(&(pos.x, pos.y, pos.z))
        {
            return;
        }
        self.load_block_entity(pos, nbt);
    }
    fn set_command_block_powered(&mut self, pos: BlockPos, powered: bool) -> Option<(bool, bool)> {
        command_blocks::set_powered(self.world, self.registry, (pos.x, pos.y, pos.z), powered)
    }
    /// `PotentSulfurBlockEntity.resetCountdown`.
    fn reset_potent_sulfur_countdown(&mut self, pos: BlockPos) {
        if let Some(BlockEntity::PotentSulfur(sulfur)) =
            self.world.block_entities.get_mut(&(pos.x, pos.y, pos.z))
        {
            sulfur.waiting_countdown = -1;
        }
    }
}

impl GameLevel<'_> {
    /// Whether a player's feet in the level keep the fire spreading around
    /// them: the radius rule, -1 meaning everywhere, else the distance to a
    /// player's feet under it.
    pub(in crate::game) fn player_near_fire(&self, pos: BlockPos) -> bool {
        let radius = self.rules.fire_spread_radius_around_player;
        radius == -1
            || self.fire_players.iter().any(|p| {
                let d = [
                    p[0] - f64::from(pos.x),
                    p[1] - f64::from(pos.y),
                    p[2] - f64::from(pos.z),
                ];
                d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < f64::from(radius) * f64::from(radius)
            })
    }

    /// The `INCREASED_FIRE_BURNOUT` environment attribute of the biome at `pos`.
    pub(in crate::game) fn biome_burns_out_fire(&self, pos: BlockPos) -> bool {
        self.world
            .chunks
            .get(&(pos.x >> 4, pos.z >> 4))
            .is_some_and(|chunk| {
                self.biomes
                    .get(chunk.biome(pos.x, pos.y, pos.z))
                    .increased_fire_burnout
            })
    }
}

/// `LecternBlock.getAnalogOutputSignal` and
/// `LecternBlockEntity.getRedstoneSignal`: the state's `has_book` gates the
/// answer, and an entity with no book reads 14 (a page count of 0 is a page
/// progress of 1.0 with no book to add).
fn lectern_signal(
    world: &myriade_engine::world::World,
    registry: &horizon_world::registry::BlockRegistry,
    state: horizon_world::registry::BlockStateId,
    pos: horizon_core::position::BlockPos,
) -> Option<i32> {
    let s = registry.state(state)?;
    if s.name != "minecraft:lectern" {
        return None;
    }
    if s.properties.get("has_book").is_none_or(|v| v != "true") {
        return Some(0);
    }
    match world.block_entities.get(&(pos.x, pos.y, pos.z)) {
        Some(myriade_engine::containers::BlockEntity::Lectern(l)) => Some(l.book.as_ref().map_or(14, |b| {
            let data = b.data();
            let count = data
                .and_then(|d| d.written.as_ref().map(|w| w.pages.len()))
                .or_else(|| data.map(|d| d.writable_pages.len()))
                .unwrap_or(0)
                .max(1);
            let f = if count > 1 {
                l.page as f32 / (count - 1) as f32
            } else {
                1.0
            };
            // `LecternBlockEntity.hasBook`: a `written_book_content` or
            // `writable_book_content` component, not just any stack. A book
            // item carries its content by default, so its id counts too.
            let has_book = matches!(
                &*b.item,
                "minecraft:writable_book" | "minecraft:written_book"
            ) || data
                .is_some_and(|d| d.written.is_some() || !d.writable_pages.is_empty());
            (f * 14.0).floor() as i32 + i32::from(has_book)
        })),
        _ => None,
    }
}
