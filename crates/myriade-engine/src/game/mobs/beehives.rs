//! Beehives and bee nests in the game (`crate::beehive` holds their logic):
//! their server tick, bees going in, and the bees let out, angry, when a
//! player breaks the hive or fire reaches it.
use super::{Game, item_abilities::ability};
use myriade_engine::{
    Result,
    beehive::{self, Beehive, BeehiveExt, HiveHost, Release},
    containers::BlockEntity,
    items::ItemStack,
    mobs::Who,
};
use horizon_core::{
    geometry::{Aabb, Vec3},
    random::{LegacyRandom, RandomSource},
};
use {horizon_core::nbt::Tag, horizon_world::registry::BlockStateId};

struct GameHive<'g, 'a> {
    game: &'g mut Game<'a>,
    /// `bees_stay_in_hive`, once asked: the time and weather it reads hold
    /// for the hive's turn, and every occupant whose time is up asks.
    stay: std::cell::Cell<Option<bool>>,
}
impl<'g, 'a> GameHive<'g, 'a> {
    fn new(game: &'g mut Game<'a>) -> Self {
        Self {
            game,
            stay: Default::default(),
        }
    }
}
impl HiveHost for GameHive<'_, '_> {
    fn random(&mut self) -> &mut LegacyRandom {
        &mut self.game.world.random
    }
    fn bees_stay_in_hive(&self, _at: [i32; 3]) -> bool {
        if let Some(stay) = self.stay.get() {
            return stay;
        }
        let world = self.game.mob_world_base(&[]);
        let night = (12542..23460).contains(&world.day_time.rem_euclid(24000));
        let stay = night || world.raining;
        self.stay.set(Some(stay));
        stay
    }
    fn blocked(&self, at: [i32; 3]) -> bool {
        !self
            .game
            .mob_world_base(&[])
            .block_collision_boxes(at[0], at[1], at[2])
            .is_empty()
    }
    fn set_honey_level(&mut self, at: [i32; 3], level: i32) {
        let state = self.game.world.block(at[0], at[1], at[2]);
        if let Some(state) = self.game.hive_with_honey(state, level) {
            let _ = self.game.level_set(at, state);
        }
    }
    fn spawn(&mut self, tag: Tag, at: [i32; 3], facing: [i32; 2], blocked: bool, delivered: bool) -> Option<u32> {
        let kind = myriade_engine::entity_nbt::kind(&tag)?;
        if !self.game.res.mob_data().entity_has("minecraft:beehive_inhabitors", &kind) {
            return None;
        }
        let Tag::Compound(c) = &tag else { return None };
        let center = at.map(|v| f64::from(v) + 0.5);
        let id = self.game.ecs_spawn(&kind, center, Some(c))?;
        self.game.ecs_with(|ecs, mw| {
            if let Some(e) = ecs.entity_of(id) {
                ecs.bee_exited(mw, e, at, facing, blocked, delivered);
            }
        });
        // `releaseOccupant`: GameEvent.BLOCK_CHANGE with the bee as the actor and the hive's state.
        let state = self.game.world.block(at[0], at[1], at[2]);
        self.game.post_block_game_event("minecraft:block_change", at, Some(Who::Mob(id)), Some(state));
        Some(id)
    }
    fn sound(&mut self, name: &'static str, at: [f64; 3]) {
        self.game
            .audio
            .play(&format!("minecraft:{name}"), Some(at), 1.0, 1.0);
    }
    fn set_changed(&mut self, at: [i32; 3]) {
        self.game.world.modified.insert((at[0] >> 4, at[2] >> 4));
    }
}

impl Game<'_> {
    /// Route a target assigned outside the mob tick through the same listener
    /// as AI, before changing the player's binding or the mob's state.
    pub(super) fn set_event_ai_target(
        &mut self,
        id: u32,
        target: Option<Who>,
        player: Option<u32>,
    ) -> bool {
        if matches!(target, Some(Who::Player))
            && (self.lan_acting_fake
                || player.is_some_and(|conn| {
                    self.lan
                        .as_ref()
                        .and_then(|host| host.hub.guest(conn))
                        .is_some_and(|guest| guest.is_fake())
                }))
        {
            return false;
        }
        let is_brain = self.ecs.has_brain(id);
        let mut chosen = target;
        let mut redirected = false;
        if !redirected
            && chosen == Some(Who::Player)
            && let Some(host) = self.lan.as_mut()
        {
            match player {
                Some(conn) => {
                    host.mob_players.insert(id, conn);
                }
                None => {
                    host.mob_players.remove(&id);
                }
            }
        }
        let who = match chosen {
            Some(w) => match self.ecs_who(w) {
                Some(w) => Some(w),
                None => return false,
            },
            None => None,
        };
        self.ecs_with(|ecs, mw| ecs.set_target(mw, id, who))
    }
    fn hive_with_honey(&self, state: BlockStateId, level: i32) -> Option<BlockStateId> {
        let registry = self.pack.assets.registry;
        let mut s = registry.state(state)?.clone();
        s.properties
            .insert("honey_level".to_owned(), level.to_string());
        registry.resolve(&s).ok()
    }
    /// The hive's honey level and front, from its block state.
    pub(super) fn hive_state(&self, at: [i32; 3]) -> Option<(Option<i32>, [i32; 2])> {
        let state = self.world.block(at[0], at[1], at[2]);
        let name =
            myriade_engine::block_entity_types::factory_block(&self.res.facts().get(state).name);
        if !self
            .res
            .items()
            .block_tags
            .get("minecraft:beehives")
            .is_some_and(|t| t.contains(name))
        {
            return None;
        }
        let props = &self.pack.assets.registry.state(state)?.properties;
        let honey = props.get("honey_level").and_then(|v| v.parse().ok());
        let facing = beehive::facing_step(props.get("facing").map(String::as_str));
        Some((honey, facing))
    }
    /// `BeehiveBlockEntity.serverTick` of the hive at `key`, after an
    /// emergency exit if fire is beside it (`BeehiveBlock.updateShape`).
    pub(super) fn tick_beehive(&mut self, key: (i32, i32, i32)) {
        let at = [key.0, key.1, key.2];
        let Some((honey, facing)) = self.hive_state(at) else {
            return;
        };
        let Some((lease, BlockEntity::Beehive(mut hive))) = self.world.block_entities.take(&key)
        else {
            return;
        };
        let fire = [
            [1, 0, 0],
            [-1, 0, 0],
            [0, 1, 0],
            [0, -1, 0],
            [0, 0, 1],
            [0, 0, -1],
        ]
        .iter()
        .any(|d| {
            let state = self.world.block(at[0] + d[0], at[1] + d[1], at[2] + d[2]);
            &*self.res.facts().get(state).name == "minecraft:fire"
        });
        let mut host = GameHive::new(self);
        if fire && !hive.bees.is_empty() {
            hive.release_all(at, facing, honey, Release::Emergency, &mut host);
        }
        hive.server_tick(at, facing, honey, &mut host);
        // A hive loaded full is told to the bees on its first tick, not only when its occupants change here.
        if self.ecs.hive_is_full(at) != hive.full() {
            self.ecs.set_hive_full(at, hive.full());
        }
        self.world
            .block_entities
            .put(lease, BlockEntity::Beehive(hive));
    }
    /// `BeehiveBlockEntity.addOccupant` for a bee its `BeeEnterHiveGoal` sent in: off its lead, its saved tag stored in the block, the
    /// entity removed from the level. A hive that is gone or full makes the bee forget it.
    pub(in crate::game) fn bee_enter_hive(&mut self, mob: u32, at: [i32; 3]) {
        let key = (at[0], at[1], at[2]);
        let Some(bee) = self.ecs.entity_of(mob) else { return };
        if !matches!(self.world.block_entities.get(&key), Some(BlockEntity::Beehive(h)) if !h.full()) {
            self.ecs.bee_forget_hive(bee);
            return;
        }
        // `addOccupant`: stopRiding, ejectPassengers, dropLeash (leash with its item, synchronized), in that order, before the tag is saved.
        self.ecs_dismount(mob);
        self.ecs.eject_passengers(bee);
        if self.ecs_view(mob).is_some_and(|v| v.leash.is_some()) {
            self.break_leash(mob, true, false);
        }
        let Some(nbt) = self.ecs.entity_nbt(bee) else { return };
        let tag = Tag::Compound(nbt);
        let Some(kind) = myriade_engine::entity_nbt::kind(&tag) else { return };
        let flower = match tag.get("flower_pos") {
            Some(Tag::IntArray(p)) if p.len() == 3 => Some([p[0], p[1], p[2]]),
            _ => None,
        };
        let Some((lease, BlockEntity::Beehive(mut hive))) = self.world.block_entities.take(&key) else {
            return;
        };
        let stored = hive.add_occupant(at, tag, flower, &mut GameHive::new(self));
        let full = hive.full();
        self.world.block_entities.put(lease, BlockEntity::Beehive(hive));
        if stored {
            self.ecs.set_hive_full(at, full);
            let state = self.world.block(at[0], at[1], at[2]);
            self.post_block_game_event("minecraft:block_change", at, Some(Who::Mob(mob)), Some(state));
            self.ecs.despawn(bee);
        }
    }
    /// `BeehiveBlockEntity.emptyAllLivingFromHive` with no player about.
    pub(super) fn empty_hive(&mut self, at: [i32; 3], facing: [i32; 2], status: Release) {
        let key = (at[0], at[1], at[2]);
        let Some((lease, BlockEntity::Beehive(mut hive))) = self.world.block_entities.take(&key)
        else {
            return;
        };
        hive.release_all(at, facing, None, status, &mut GameHive::new(self));
        self.ecs.set_hive_full(at, hive.full());
        self.world
            .block_entities
            .put(lease, BlockEntity::Beehive(hive));
    }
    /// `BeehiveBlock.playerDestroy` without silk touch: the bees come out
    /// angry at the player unless smoke calms them, then
    /// `angerNearbyBees`.
    pub(super) fn hive_broken_by_player(
        &mut self,
        at: [i32; 3],
        mut hive: Beehive,
        honey: Option<i32>,
        facing: [i32; 2],
    ) {
        let released = hive.release_all(
            at,
            facing,
            honey,
            Release::Emergency,
            &mut GameHive::new(self),
        );
        let sedated = beehive::smokey(&self.mob_world_base(&[]), at);
        self.ecs.set_hive_full(at, false);
        if let Some(p) = self.player_view().map(|p| p.position) {
            let ids = self.bees_within_of(&released, p);
            for id in ids {
                if sedated {
                    self.ecs.bee_stay_out(id, horizon_ecs_proto::behaviours::bee::SMOKED_STAY_OUT);
                } else {
                    self.set_event_ai_target(id, Some(Who::Player), self.lan_acting);
                }
            }
        }
        self.anger_nearby_bees(at);
    }
    /// `BeehiveBlock.useItemOn` with `held` in the acting hand: shears take
    /// the honeycombs and a glass bottle a honey bottle from a full hive
    /// (honey level 5), then the honey level resets and, unless a campfire
    /// smokes the hive, the bees come out angry. True when it took the click.
    pub(super) fn use_beehive(&mut self, pos: [i32; 3], held: Option<&ItemStack>) -> Result<bool> {
        let Some(held) = held else { return Ok(false) };
        let shears = self.stack_shears(held, ability::SHEARS_HARVEST);
        if !shears && &*held.item != "minecraft:glass_bottle" {
            return Ok(false);
        }
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if !matches!(
            &*self.res.facts().get(state).name,
            "minecraft:beehive" | "minecraft:bee_nest"
        ) {
            return Ok(false);
        }
        let prop = |key: &str| {
            self.pack
                .assets
                .registry
                .state(state)
                .and_then(|s| s.properties.get(key).cloned())
        };
        if prop("honey_level").and_then(|v| v.parse::<i32>().ok()) != Some(5) {
            return Ok(false);
        }
        let facing = beehive::facing_step(prop("facing").as_deref());
        let at = self
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        if shears {
            // dropHoneycomb (harvest/beehive), the shear sound, hurtAndBreak.
            for stack in self.sequence_loot(
                "minecraft:harvest/beehive",
                "minecraft:harvest/beehive",
                Some(held),
            ) {
                self.pop_resource(pos, stack);
            }
            self.audio
                .play("minecraft:block.beehive.shear", at, 1.0, 1.0);
            self.damage_held(1);
            self.post_block_game_event("minecraft:shear", pos, self.sculk_actor(), None);
        } else {
            // shrink(1), then the honey bottle in the hand, the inventory or
            // on the ground.
            self.audio.play("minecraft:item.bottle.fill", at, 1.0, 1.0);
            self.edit_block_use_stack(|cell| {
                if let Some(s) = cell.as_mut() {
                    s.count -= 1;
                    if s.count == 0 {
                        *cell = None;
                    }
                }
            });
            let bottle = ItemStack::new("minecraft:honey_bottle", 1);
            if self.held().is_none() {
                *self.held_mut() = Some(bottle);
            } else if let Some(rest) = self.add_stack_to_inventory(bottle)
                && let Some(eye) = self.eye_position()
            {
                // `player.drop(stack, false, PREDICTED)`: thrown from below
                // the eyes along the view, with no thrower.
                self.throw_from_eyes(rest, eye, false, false);
            }
            self.post_block_game_event("minecraft:fluid_pickup", pos, self.sculk_actor(), None);
        }
        self.award_stat("used", &held.item, 1);
        let smokey = beehive::smokey(&self.mob_world_base(&[]), pos);
        let key = (pos[0], pos[1], pos[2]);
        if !smokey
            && matches!(self.world.block_entities.get(&key), Some(BlockEntity::Beehive(h)) if !h.bees.is_empty())
        {
            self.anger_nearby_bees(pos);
        }
        // resetHoneyLevel.
        if let Some(reset) = self.with_props(state, &[("honey_level", "0")]) {
            self.level_set(pos, reset)?;
        }
        if !smokey
            && let Some((lease, BlockEntity::Beehive(mut hive))) =
                self.world.block_entities.take(&key)
        {
            // emptyAllLivingFromHive: the bees within 4 blocks of the player
            // go for them.
            let released = hive.release_all(
                pos,
                facing,
                None,
                Release::Emergency,
                &mut GameHive::new(self),
            );
            self.world
                .block_entities
                .put(lease, BlockEntity::Beehive(hive));
            self.ecs.set_hive_full(pos, false);
            if let Some(p) = self.player_view().map(|p| p.position) {
                for id in self.bees_within_of(&released, p) {
                    self.set_event_ai_target(id, Some(Who::Player), self.lan_acting);
                }
            }
        }
        // ServerPlayerGameMode.useItemOn: the action was consumed.
        self.trigger_block("minecraft:item_used_on_block", pos, held);
        Ok(true)
    }
    /// `BeehiveBlock.angerNearbyBees`: bees within 8 blocks (6 up and down)
    /// without a target go for a random player in the same box, the window's
    /// own or a guest (`None` is the window's player).
    pub(super) fn anger_nearby_bees(&mut self, at: [i32; 3]) {
        let min = Vec3::new(
            f64::from(at[0]) - 8.0,
            f64::from(at[1]) - 6.0,
            f64::from(at[2]) - 8.0,
        );
        let area = Aabb::new(min, Vec3::new(min.x + 17.0, min.y + 13.0, min.z + 17.0));
        let mut players: Vec<Option<u32>> = Vec::new();
        if self
            .player_view()
            .is_some_and(|p| p.bounds.intersects(area))
        {
            players.push(None);
        }
        if let Some(host) = self.lan.as_ref() {
            players.extend(
                host.hub
                    .guests
                    .iter()
                    .filter(|g| host.hub.here(g) && g.loaded && !g.dead && !g.is_fake())
                    .filter(|g| super::lan_mobs::guest_bounds(g).intersects(area))
                    .map(|g| Some(g.conn)),
            );
        }
        if players.is_empty() {
            return;
        }
        let bees = self.ecs.untargeted_bees_in(area);
        for e in bees {
            let id = self.ecs_id_of(e);
            // Util.getRandom over the players.
            let pick = self.world.random.next_int_bound(players.len() as i32).unwrap_or(0) as usize;
            self.set_event_ai_target(id, Some(Who::Player), players[pick]);
        }
    }
    /// The ids among `ids` of the ECS creatures within 4 blocks of `p` (`distanceToSqr <= 16`).
    fn bees_within_of(&self, ids: &[u32], p: Vec3) -> Vec<u32> {
        ids.iter()
            .copied()
            .filter(|&id| {
                self.ecs.position(id).is_some_and(|q| {
                    let d = [q[0] - p.x, q[1] - p.y, q[2] - p.z];
                    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= 16.0
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod honey_overflow_tests {
    use super::*;

    /// `BeehiveBlock.useItemOn`: a honey bottle that no longer fits is
    /// `player.drop(stack, false, PREDICTED)`: from 0.3 below the eyes,
    /// thrown along the view, with no thrower (#11377).
    #[test]
    #[ignore = "daily"]
    fn a_honey_bottle_that_does_not_fit_is_thrown_ahead() {
        super::super::native_entity_refs::tests::with_game(|game| {
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(myriade_engine::chunk::Chunk::empty(0, 0)),
            );
            let registry = &game.pack.assets.registry;
            let hive = registry.default_state("minecraft:beehive").unwrap();
            let mut full = registry.state(hive).unwrap().clone();
            full.properties.insert("honey_level".into(), "5".into());
            let full = registry.id(&full).unwrap();
            game.world.set(8, 64, 10, full).unwrap();
            game.player = Some(
                myriade_engine::motion::PlayerMotion::new(game.motion, Vec3::new(8.5, 64.0, 8.5))
                    .unwrap(),
            );
            // Facing south (+z), towards the hive.
            game.yaw = 0.0;
            game.pitch = 0.0;
            for slot in game.inventory.iter_mut() {
                *slot = Some(ItemStack::new("minecraft:stone", 64));
            }
            game.selected = 0;
            let bottles = ItemStack::new("minecraft:glass_bottle", 2);
            game.inventory[0] = Some(bottles.clone());
            game.ecs_clear_items();

            assert!(game.use_beehive([8, 64, 10], Some(&bottles)).unwrap());
            let eye = game.eye_position().unwrap();
            let honey: Vec<_> = game
                .ecs.item_views()
                .into_iter()
                .filter(|e| &*e.stack.item == "minecraft:honey_bottle")
                .collect();
            assert_eq!(honey.len(), 1);
            let e = honey[0].clone();
            assert!((e.position.y - (eye[1] - f64::from(0.3f32))).abs() < 1e-9);
            assert!(e.velocity.z > 0.2, "thrown ahead: {:?}", e.velocity);
            assert_eq!(e.thrower, None);
            assert_eq!(e.pickup_delay, 40);
        });
    }
}

#[cfg(test)]
mod stay_tests {
    use super::*;
    use myriade_engine::beehive::Occupant;
    use horizon_core::nbt::Compound;

    /// `BeehiveBlockEntity.tickOccupants`: every bee whose time is up asks
    /// `releaseOccupant`, which keeps it in at night (`BEES_STAY_IN_HIVE`)
    /// and lets it out by day. All of a hive's bees get the same answer in
    /// one turn, however many ask.
    #[test]
    #[ignore = "daily"]
    fn every_ready_bee_of_a_hive_turn_gets_the_same_answer() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(myriade_engine::chunk::Chunk::empty(0, 0)),
            );
            let state = game
                .pack
                .assets
                .registry
                .default_state("minecraft:beehive")
                .unwrap();
            let key = (8, 64, 8);
            game.world.set(key.0, key.1, key.2, state).unwrap();
            let bee = || {
                let mut tag = Compound::new();
                tag.insert("id".into(), Tag::String("minecraft:bee".into()));
                Occupant {
                    entity_data: Tag::Compound(tag),
                    ticks_in_hive: 601,
                    min_ticks_in_hive: 600,
                }
            };
            let hive = Beehive {
                bees: vec![bee(), bee(), bee()],
                flower: None,
            };
            game.world
                .block_entities
                .insert(key, BlockEntity::Beehive(Box::new(hive)));
            let inside = |game: &Game<'_>| match game.world.block_entities.get(&key) {
                Some(BlockEntity::Beehive(h)) => h.bees.len(),
                _ => panic!("the hive is gone"),
            };
            game.world.day_time = 13_000;
            game.tick_beehive(key);
            let out = |game: &Game<'_>| game.ecs.ids_near([8.5, 64.5, 8.5], 4.0).len();
            assert_eq!((inside(game), out(game)), (3, 0), "night");
            game.world.day_time = 1_000;
            game.tick_beehive(key);
            assert_eq!((inside(game), out(game)), (0, 3), "day");
        });
    }
}

#[cfg(test)]
mod changed_tests {
    use crate::game::game_tests::tests::server_game;
    use myriade_engine::{
        beehive::{Beehive, Occupant},
        containers::BlockEntity,
    };
    use horizon_core::nbt::Tag;

    fn hive_at(game: &mut super::Game<'_>, key: (i32, i32, i32), hive: Beehive) {
        let state = game
            .pack
            .assets
            .registry
            .default_state("minecraft:beehive")
            .unwrap();
        game.world.set(key.0, key.1, key.2, state).unwrap();
        game.world
            .block_entities
            .insert(key, BlockEntity::Beehive(Box::new(hive)));
    }

    /// `BeehiveBlockEntity.serverTick` -> `tickOccupants` calls `setChanged`
    /// when a bee leaves. The hive's saved `bees` list lost an entry, and
    /// nothing else in the tick changed a block state.
    #[test]
    #[ignore = "daily"]
    fn a_released_bee_marks_the_chunk() {
        let mut game_resources = None;
        let mut game = server_game(&mut game_resources);
        let key = (8, 64, 8);
        hive_at(
            &mut game,
            key,
            Beehive {
                bees: vec![Occupant {
                    entity_data: Tag::compound([("id", Tag::String("minecraft:bee".into()))]),
                    ticks_in_hive: 601,
                    min_ticks_in_hive: 600,
                }],
                flower: None,
            },
        );
        game.world.day_time = 1_000;
        game.world.modified.clear();

        game.tick_beehive(key);

        let Some(BlockEntity::Beehive(h)) = game.world.block_entities.get(&key) else {
            panic!("no hive");
        };
        assert!(h.bees.is_empty(), "the ready bee was let out");
        assert_eq!(game.ecs.ids_near([8.5, 64.5, 8.5], 4.0).len(), 1, "the bee reached the level");
        assert!(
            game.world.modified.contains(&(0, 0)),
            "a released bee left the chunk unmarked"
        );
    }

    /// `BeehiveBlockEntity.addOccupant` ends with `setChanged`: the hive
    /// stored the bee and the mob is gone, which no block state shows.
    #[test]
    #[ignore = "daily"]
    fn a_bee_going_in_marks_the_chunk() {
        let mut game_resources = None;
        let mut game = server_game(&mut game_resources);
        let key = (8, 64, 8);
        hive_at(&mut game, key, Beehive::default());
        let id = game.ecs_spawn("minecraft:bee", [8.0, 64.0, 8.0], None).unwrap();
        game.world.modified.clear();

        game.bee_enter_hive(id, [8, 64, 8]);
        assert!(!game.ecs.exists(id), "the bee left the world: its data is in the block");

        let Some(BlockEntity::Beehive(h)) = game.world.block_entities.get(&key) else {
            panic!("no hive");
        };
        assert_eq!(h.bees.len(), 1, "the bee went in");
        assert!(
            game.world.modified.contains(&(0, 0)),
            "a stored bee left the chunk unmarked"
        );
    }
}

#[cfg(test)]
mod ecs_hive_tests {
    use crate::game::ecs::test_support::ecs_game_with;
    use myriade_engine::{beehive::{Beehive, Occupant}, containers::BlockEntity};
    use horizon_core::nbt::{Compound, Tag};

    fn hive(game: &mut super::Game<'_>, bees: usize) {
        let state = game.pack.assets.registry.default_state("minecraft:beehive").unwrap();
        game.world.set(8, 64, 8, state).unwrap();
        let mut tag = Compound::new();
        tag.insert("id".into(), Tag::String("minecraft:bee".into()));
        let bee = || Occupant { entity_data: Tag::Compound(tag.clone()), ticks_in_hive: 0, min_ticks_in_hive: 600 };
        let hive = Beehive { bees: (0..bees).map(|_| bee()).collect(), flower: None };
        game.world.block_entities.insert((8, 64, 8), BlockEntity::Beehive(Box::new(hive)));
    }

    /// `addOccupant`: the entering bee is let off its vehicle and drops its riders before it leaves the world.
    #[test]
    #[ignore = "daily"]
    fn a_ridden_bee_entering_a_hive_frees_its_rider() {
        let (mut g, bee) = ecs_game_with("minecraft:bee", [8.5, 65.0, 8.5]);
        let rider = g.ecs_spawn("minecraft:zombie", [8.5, 65.0, 8.5], None).unwrap();
        assert!(g.ecs_mount(rider, bee));
        hive(&mut g, 0);
        g.bee_enter_hive(bee, [8, 64, 8]);
        assert_eq!(g.ecs_vehicle_of(rider), None);
        assert!(g.ecs.entity_of(bee).is_none());
        assert!(matches!(g.world.block_entities.get(&(8, 64, 8)), Some(BlockEntity::Beehive(h)) if h.bees.len() == 1));
    }

    /// A hive loaded full is known as full to the bees from its first tick, without any change of occupants.
    #[test]
    #[ignore = "daily"]
    fn a_hive_loaded_full_is_told_to_the_bees_on_its_first_tick() {
        let (mut g, _) = ecs_game_with("minecraft:bee", [8.5, 65.0, 8.5]);
        hive(&mut g, 3);
        assert!(!g.ecs.hive_is_full([8, 64, 8]));
        g.tick_beehive((8, 64, 8));
        assert!(g.ecs.hive_is_full([8, 64, 8]));
    }
}
