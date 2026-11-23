//! Monster spawners in the game: `SpawnerBlockEntity.serverTick` and
//! `clientTick` for every spawner (`mobs::spawner`), the mobs they add, a
//! spawn egg used on one (`SpawnEggItem.useOn`), their events and the mob
//! spinning inside (`SpawnerRenderer`).
#[path = "spawner/entity_registry.rs"]
mod entity_registry;
use super::{
    Game,
    creatures::{Finalized, SpawnReason},
};
pub(super) use entity_registry::entity_type_saves;
use horizon_client::{
    Result,
    containers::BlockEntity,
    mobs::{
        Who,
        spawner::{SpawnDataExt, Spawner, SpawnerAdded, SpawnerEvent, SpawnerExt, SpawnerSpawn},
        trial_spawner::{TrialSpawnerExt, TrialState},
    },
};
use horizon_core::nbt::Tag;
use horizon_core::{
    geometry::Vec3,
    random::{LegacyRandom, LevelRandom},
};
use horizon_ecs_proto::spawn_sheet::SpawnSheet;
use std::collections::{HashMap, HashSet};

type Pos = (i32, i32, i32);

/// The box a standing adult of a kind of `width` by `height` takes at `at` (`Entity.makeBoundingBox`), before the mob is made.
fn kind_bounds(width: f64, height: f64, at: Vec3) -> horizon_core::geometry::Aabb {
    let w = width / 2.0;
    horizon_core::geometry::Aabb::new(
        Vec3::new(at.x - w, at.y, at.z - w),
        Vec3::new(at.x + w, at.y + height, at.z + w),
    )
}

/// What a spawner tick decided to add, created once the level is free.
pub(super) enum Planned {
    /// A fresh mob, its spawn sheet made by the hooks and the spawn data's `equipment`; `special` is None when its finalize was skipped.
    Finalized {
        kind: String,
        at: [f64; 3],
        yaw: f32,
        sheet: SpawnSheet,
        reason: SpawnReason,
        special: Option<f32>,
        key: u32,
    },
    /// A mob loaded from the spawn data (with its passengers) and its
    /// `equipment`; the provisional id its events carry.
    Loaded(Tag, Option<Tag>, u32),
}

/// What the spawner code keeps between ticks.
#[derive(Default)]
pub(super) struct SpawnerState {
    /// Every entity type id (`EntityType.byString`), read once.
    entity_types: Option<HashSet<String>>,
    /// Dimensions from the same loaded entity-type registry, not a second loader.
    entity_dimensions: HashMap<String, [f32; 2]>,
}

impl Game<'_> {
    /// The server tick of the spawner at `key`.
    ///
    /// The caller only reaches this for a `BlockEntity::Spawner`, and changing
    /// the block removes or replaces that entity, so the block is not read
    /// again here.
    pub(super) fn tick_spawner(&mut self, key: Pos) -> Result<()> {
        let Some((lease, BlockEntity::Spawner(mut spawner))) = self.world.block_entities.take(&key)
        else {
            return Ok(());
        };
        let result = self.server_tick_spawner(key, &mut spawner);
        self.world
            .block_entities
            .put(lease, BlockEntity::Spawner(spawner));
        result
    }
    fn server_tick_spawner(&mut self, key: Pos, spawner: &mut Spawner) -> Result<()> {
        self.server_tick_spawner_of(key, spawner, None)
    }
    /// `BaseSpawner.serverTick` at `key`: a spawner block's, or the spawner minecart
    /// `cart` is at (its `broadcastEvent` is an entity event on the cart).
    pub(super) fn server_tick_spawner_of(
        &mut self,
        key: Pos,
        spawner: &mut Spawner,
        cart: Option<u32>,
    ) -> Result<()> {
        // `BaseSpawner.serverTick` does nothing without an alive player in
        // range (`isNearPlayer`) or with the rule off: no level to build for it.
        let at = [key.0, key.1, key.2].map(|c| f64::from(c) + 0.5);
        let range = f64::from(spawner.required_player_range);
        if !self.alive_player_near(at[0], at[1], at[2], range)
            || !self
                .session
                .rules
                .bool_at(horizon_client::gamerules::vanilla::SPAWNER_BLOCKS_WORK)
        {
            return Ok(());
        }
        // While its delay counts down, `serverTick` only takes one off it: no
        // level, random or event (a server with hundreds of dungeons near its
        // players built the level for every one of them, every tick).
        if spawner.spawn_delay > 0 {
            spawner.spawn_delay -= 1;
            return Ok(());
        }
        let pos = [key.0, key.1, key.2];
        let mut random = std::mem::replace(
            &mut self.world.random,
            LevelRandom::for_level(LegacyRandom::new(0)),
        );
        let mut events = Vec::new();
        let mut planned: Vec<Planned> = Vec::new();
        // A minecart spawner has no block.
        let source = cart
            .is_none()
            .then(|| mod_api::SpawnSource::Block(mod_api::BlockPos::new(key.0, key.1, key.2)));
        let rides = {
            let mut level = self.natural_level();
            // A minecart spawner has no block to tell the spawn checks of.
            level.spawner = cart.is_none().then_some(mod_api::Spawner::Block {
                pos: mod_api::BlockPos::new(key.0, key.1, key.2),
            });
            let mut spawn = |l: &super::spawn_level::GameLevel,
                             _rng: &mut LevelRandom,
                             r: &SpawnerSpawn| {
                l.game.plan_spawner_mob(l, r, source, &mut planned)
            };
            spawner.server_tick(&level, &mut random, pos, &mut spawn, &mut events);
            std::mem::take(&mut *level.rides.borrow_mut())
        };
        self.world.random = random;
        let ids = self.add_planned(planned, rides);
        self.apply_spawner_events(key, events, &ids, cart)
    }
    /// Creates the mobs a spawner tick planned; their provisional ids
    /// (counting down from u32::MAX) map to the mobs' ids.
    /// (counting down from `u32::MAX`) map to the mobs' ids.
    pub(super) fn add_planned(
        &mut self,
        planned: Vec<Planned>,
        _rides: Vec<(u32, u32)>,
    ) -> HashMap<u32, u32> {
        // The planned mobs go into the ECS level, in spawn order; the map gives each provisional id its network id. The
        // rides are not carried yet (no ECS call for them).
        let mut ids = HashMap::new();
        for plan in planned {
            match plan {
                Planned::Finalized { kind, at, yaw, sheet, reason, special, key } => {
                    let finalize = special.map(|special| (special, sheet.group));
                    if let Some((id, _)) = self.ecs_spawn_sheet(&kind, at, None, reason, finalize, &sheet) {
                        // The spawner's `moveTo(..., yaw)`: the template holds only the id, so the yaw is all it places.
                        if let Some(e) = self.ecs.entity_of(id) {
                            self.ecs.set_rotation(e, yaw, 0.0);
                        }
                        ids.insert(key, id);
                    }
                }
                Planned::Loaded(Tag::Compound(tag), _equipment, key) => {
                    if let Some(id) = self.ecs_load(&tag) {
                        ids.insert(key, id);
                    }
                }
                Planned::Loaded(..) => {}
            }
        }
        ids
    }
    /// The spawner's `loadEntityRecursive` and `tryAddFreshEntityWithPassengers`:
    /// the mob it will add (created after the tick) and its box.
    pub(super) fn plan_spawner_mob(
        &self,
        l: &super::spawn_level::GameLevel,
        r: &SpawnerSpawn,
        source: Option<mod_api::SpawnSource>,
        planned: &mut Vec<Planned>,
    ) -> Option<SpawnerAdded> {
        let kind = self.res.mob_types().get(r.id)?.clone();
        let [x, y, z] = r.position;
        let at = Vec3::new(x, y, z);
        let (bounds, key) = if r.finalize {
            // The sheet a listened position check made, else a new one; the spawn data's equipment goes in first, then the hooks' edits.
            let reason = if r.trial {
                SpawnReason::TrialSpawner
            } else {
                SpawnReason::Spawner
            };
            let mut sheet = l.take_checked(r.id, r.position, r.yaw).unwrap_or_default();
            // TrialSpawner.spawnMob: setPersistenceRequired.
            sheet.persistent = r.trial;
            if let Some(eq) = r.equipment {
                for (slot, stack) in horizon_items::equipment::Equipment::read(eq).iter().enumerate() {
                    if let Some(stack) = stack {
                        sheet.set_equipment(slot, Some(stack.clone()));
                    }
                }
            }
            let special = match self.finalize_spawn_event(r.id, [x, y, z], reason, source, &mut sheet) {
                (Finalized::Cancelled, _) => return None,
                (Finalized::Skipped, _) => None,
                (Finalized::Done, difficulty) => Some(difficulty.special()),
            };
            let key = l.provisional_id();
            let bounds = kind_bounds(kind.width, kind.height, at);
            planned.push(Planned::Finalized { kind: r.id.to_owned(), at: [x, y, z], yaw: r.yaw, sheet, reason, special, key });
            (bounds, key)
        } else {
            // A UUID already in the level is refused.
            let mut tag = r.entity.clone();
            if let Some(uuid) = horizon_client::entity_nbt::uuid(&Tag::Compound(tag.clone()))
                && self.persist.uuids.values().any(|u| *u == uuid)
            {
                return None;
            }
            let double = |v: f64| Tag::Double(v.to_bits());
            tag.insert(
                "Pos".into(),
                Tag::List(vec![double(x), double(y), double(z)]),
            );
            tag.insert(
                "Rotation".into(),
                Tag::List(vec![
                    Tag::Float(r.yaw.to_bits()),
                    Tag::Float(0f32.to_bits()),
                ]),
            );
            if r.trial {
                tag.insert("PersistenceRequired".into(), Tag::Byte(1));
            }
            let mut tag = Tag::Compound(tag);
            // EntityType.loadEntityRecursive applies the spawner's placement
            // callback to every passenger as well as the root.
            super::commands::place_summoned(&mut tag, r.position);
            let bounds = kind_bounds(kind.width, kind.height, at);
            let key = l.provisional_id();
            planned.push(Planned::Loaded(tag, r.equipment.cloned(), key));
            (bounds, key)
        };
        l.pending.borrow_mut().push(bounds);
        l.pending_ids.borrow_mut().push((r.id.to_owned(), bounds));
        Some(SpawnerAdded {
            id: key,
            bounds,
            mob: true,
        })
    }
    /// What a spawner's tick or a spawn egg did, shown and sent.
    fn apply_spawner_events(
        &mut self,
        key: Pos,
        events: Vec<SpawnerEvent>,
        ids: &HashMap<u32, u32>,
        cart: Option<u32>,
    ) -> Result<()> {
        let pos = [key.0, key.1, key.2];
        for event in events {
            match event {
                // A cart has no block to update: `sendBlockUpdated` finds nothing to send.
                SpawnerEvent::BlockUpdated if cart.is_some() => {}
                SpawnerEvent::BlockUpdated => self.spawner_block_updated(key),
                // `broadcastEvent`: `level.broadcastEntityEvent(cart, id)`.
                SpawnerEvent::BlockEvent(id) if cart.is_some() => {
                    let at = pos.map(|c| f64::from(c) + 0.5);
                    self.lan_entity_event(cart.unwrap_or_default() as i32, id as i8, at);
                }
                SpawnerEvent::BlockEvent(id) => {
                    self.lan_block_event(pos, id, 0);
                    // The client copy's onEventTriggered.
                    if let Some(BlockEntity::Spawner(s)) = self.world.block_entities.get_mut(&key) {
                        s.client_event(id);
                    }
                }
                SpawnerEvent::LevelEvent(id) => self.level_event_at(id, pos, 0),
                SpawnerEvent::EntityPlace(mob, at) => {
                    let who = ids.get(&mob).map(|id| Who::Mob(*id));
                    self.post_block_game_event("minecraft:entity_place", at, who, None);
                }
                SpawnerEvent::SpawnAnim(mob) => {
                    if let Some(id) = ids.get(&mob) {
                        self.mob_poof(*id);
                    }
                }
            }
        }
        Ok(())
    }
    /// `sendBlockUpdated`: the clients load the new update tag (the client
    /// copy's delay follows it, its display mob is made again).
    fn spawner_block_updated(&mut self, key: Pos) {
        self.world
            .path_type_block_updated(horizon_core::position::BlockPos::new(key.0, key.1, key.2));
        if let Some(BlockEntity::Spawner(s)) = self.world.block_entities.get_mut(&key) {
            s.client_loaded();
        }
        if let Some(log) = self.world.block_log.as_mut() {
            log.push(key);
        }
        self.world.modified.insert((key.0 >> 4, key.2 >> 4));
    }
    /// `LivingEntity.makePoofParticles` (entity event 20) for a mob.
    pub(super) fn mob_poof(&mut self, id: u32) {
        let Some(at) = self.ecs.position(id) else {
            return;
        };
        self.lan_entity_event(id as i32, 20, at);
    }
    /// `SpawnEggItem.useOn` on a spawner: the egg sets its mob, and an op's
    /// egg its entity `data` too (`block_entity_item::spawn_egg_on_spawner`).
    /// None when the clicked block has no spawner.
    pub(super) fn use_spawn_egg_on_spawner(
        &mut self,
        pos: [i32; 3],
        kind: &str,
        data: Option<&horizon_core::nbt::Compound>,
    ) -> Option<bool> {
        let key = (pos[0], pos[1], pos[2]);
        let trial = match self.world.block_entities.get(&key) {
            Some(BlockEntity::Spawner(_)) => false,
            Some(BlockEntity::TrialSpawner(_)) => true,
            _ => return None,
        };
        if !self
            .session
            .rules
            .bool_at(horizon_client::gamerules::vanilla::SPAWNER_BLOCKS_WORK)
        {
            self.system_message(horizon_client::text::Text::translate(
                "advMode.notEnabled.spawner",
                &[],
            ));
            return Some(false);
        }
        if trial {
            // TrialSpawnerBlockEntity.setEntityId and setEntityData:
            // overrideEntityToSpawn, which ends in setState(INACTIVE).
            if let Some(BlockEntity::TrialSpawner(t)) = self.world.block_entities.get_mut(&key) {
                match data {
                    Some(data) => t.override_entity_data(kind, data),
                    None => t.override_entity(kind),
                }
            }
            let ominous = self.trial_spawner_block(key).is_some_and(|(_, o)| o);
            let _ = self.set_trial_spawner_state(key, TrialState::Inactive, ominous);
        } else {
            let mut events = Vec::new();
            let mut random = std::mem::replace(
                &mut self.world.random,
                LevelRandom::for_level(LegacyRandom::new(0)),
            );
            if let Some(BlockEntity::Spawner(s)) = self.world.block_entities.get_mut(&key) {
                match data {
                    Some(data) => s.set_entity_data(kind, data, &mut random, &mut events),
                    None => s.set_entity_id(kind, &mut random, &mut events),
                }
            }
            self.world.random = random;
            let _ = self.apply_spawner_events(key, events, &HashMap::new(), None);
        }
        // sendBlockUpdated(3) and the player's block_change game event.
        self.spawner_block_updated(key);
        self.post_block_game_event("minecraft:block_change", pos, Some(Who::Player), None);
        Some(true)
    }
    /// `PlayerList.isOp` for the player acting now: a server's guest by their
    /// own level, else the window's player.
    pub(super) fn acting_player_is_op(&self) -> bool {
        self.lan_acting_permission
            .unwrap_or(self.session.registries.permission)
            > 0
    }
    /// `EntityType.createDefaultStackConfig` on an entity an item made (a
    /// vehicle, a hanging entity, an armor stand): the item's custom name and
    /// custom data (`applyComponentsFromItemStack`:
    /// `Entity.applyImplicitComponent`'s two, saved as `CustomName` and
    /// `data`), then its `minecraft:entity_data` merged over the entity's own
    /// compound and loaded (`updateCustomEntityTag`) when it names this type.
    /// An op-only type (command block and spawner minecarts) takes it from an
    /// op player only, never from a dispenser (`by_player` false).
    pub(super) fn apply_item_stack_config(
        &mut self,
        target: super::commands::Target,
        kind: &str,
        stack: &horizon_client::items::ItemStack,
        by_player: bool,
    ) {
        let op = by_player && self.acting_player_is_op();
        let configures = stack
            .data()
            .is_some_and(|d| d.custom_name.is_some() || d.other("minecraft:custom_data").is_some())
            || horizon_client::block_entity_item::spawn_egg_entity_data(stack, kind, op).is_some();
        if !configures {
            return;
        }
        let Some(own) = self.command_target_nbt(target) else {
            return;
        };
        let tag =
            horizon_client::block_entity_item::stack_configured_entity_by(stack, kind, &own, op);
        // The entity's own compound, its name and data merged in: it loads back.
        let _ = self.apply_entity_nbt(target, tag);
    }
}
