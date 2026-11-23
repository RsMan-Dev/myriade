//! Dropped items and experience orbs: their tick (`tick_items`, `tick_dropped_items`,
//! `tick_experience`), the stacks a destroyed container lets fall, and spawning orbs.
use super::*;

/// Whether a native mod's invulnerability, the vanilla `isInvulnerableTo` and a saved
/// `damage_resistant` refuse `$damage` (one of `ITEM_ENVIRONMENT_DAMAGE`) on item `$id`, with
/// `$saved` what `saved_refuses_damage` answered for each, in `$world`.
macro_rules! item_refuses {
    ($s:ident, $id:expr, $saved:expr, $world:expr, $damage:expr) => {{
        let damage: &str = $damage;
        let vanilla = match damage {
            "minecraft:on_fire" => $saved[0],
            "minecraft:lava" => $saved[1],
            "minecraft:in_fire" => $saved[2],
            "minecraft:cactus" => $saved[3],
            _ => false,
        };
        let source = myriade_engine::health::HurtSource {
            damage_type: damage,
            causing: 0,
            direct: 0,
            position: None,
        };
        myriade_engine::health::DamageLevel::of(
            $world,
            $s.pack.assets.registry,
            Some(world_items!($s)),
            Some(&$s.pack.block_registries),
            &$s.damage_hooks,
        )
        .invulnerability_check(
            myriade_engine::health::Hurting::Entity($id as i32, "minecraft:item", source),
            vanilla,
        )
    }};
}

impl Game<'_> {
    pub(super) fn tick_items(&mut self) {
        let mut round = self.item_update_round();
        self.tick_dropped_items(&mut round);
        horizon_threads::work::clear_work();
        self.drain_entity_inside();
        self.native_tick_post_batch(round);
    }
    /// An item that joined during this tick's entity pass: it first ticks on
    /// the next one.
    pub(super) fn joined_during_entity_pass(&self, id: u32) -> bool {
        id > self.ticked_entities_through || self.contact_drops.contains(&id)
    }
    pub(super) fn tick_dropped_items(&mut self, _round: &mut native_ticks::TickRound) {
        // The stacks tick, merge and expire in the ECS (P's `item`): the host only lets the player take those in reach, by position.
        if self.ecs.is_active()
            && let Some(p) = self.player.as_ref().filter(|p| !p.health.dead_or_dying() && !self.spectator())
        {
            let feet = [p.position.x, p.position.y, p.position.z];
            self.ecs_player_pickups(feet);
        }
    }
    pub(super) fn spawn_orbs(&mut self, at: [f64; 3], value: u32) {
        self.spawn_orbs_toward(at, [0.0; 3], value);
    }
    /// `ExperienceOrb.awardWithDirection`: `award` whose fresh orbs start
    /// `roughly` that way (`new ExperienceOrb(level, pos, roughly, value)`).
    pub(super) fn spawn_orbs_toward(&mut self, at: [f64; 3], roughly: [f64; 3], mut value: u32) {
        while value > 0 {
            let v = ExperienceOrb::split(value);
            value -= v;
            // The merge id is drawn from the level random; the ECS merges the orb with those near it.
            let _ = self.world.random.next_int_bound(40);
            self.add_orb_toward(at, roughly, v);
        }
    }
    /// `ExperienceOrb.award` for one orb, unmerged: the ECS merges it with those near it.
    pub(super) fn award_orb(&mut self, at: [f64; 3], value: u32) {
        self.add_orb(at, value);
    }
    /// `ExperienceOrb.unstuckIfPossible`, which the orb's constructor runs on an
    /// orb whose box is in a block: the free spot nearest its box's centre. An
    /// orb in open air keeps its position.
    pub(super) fn unstuck_orb(&self, at: [f64; 3]) -> [f64; 3] {
        let b = self.res.behaviours();
        let border = self.border_bounds();
        ExperienceOrb::unstuck(Vec3::new(at[0], at[1], at[2]), &border, |p| {
            b.collision_shape(self.world.block(p.x, p.y, p.z))
                .map(|s| s.moved(Vec3::new(f64::from(p.x), f64::from(p.y), f64::from(p.z))))
        })
        .map_or(at, |p| [p.x, p.y, p.z])
    }
    /// `new ExperienceOrb(level, pos, Vec3.ZERO, value)`: yaw and motion from
    /// the orb's own random.
    pub(in crate::game) fn add_orb(&mut self, at: [f64; 3], value: u32) {
        self.add_orb_toward(at, [0.0; 3], value);
    }
    /// `new ExperienceOrb(level, pos, roughly, value)`: a random motion that
    /// points against `roughly` is turned round, and the orb starts half its
    /// size (0.25) along `roughly`, unstuck if it collides there.
    pub(super) fn add_orb_toward(&mut self, at: [f64; 3], roughly: [f64; 3], value: u32) {
        let len_sqr = roughly.iter().map(|v| v * v).sum::<f64>();
        // Vec3.normalize: a vector shorter than 1e-5 has no direction.
        let len = len_sqr.sqrt();
        let unit = if len < 1.0e-5 {
            [0.0; 3]
        } else {
            roughly.map(|v| v / len)
        };
        let at = self.unstuck_orb([0, 1, 2].map(|a| at[a] + unit[a] * 0.25));
        let mut r = random_source::entity();
        let _yaw = r.next_float();
        let p = Vec3::new(at[0], at[1], at[2]);
        let mut velocity = Vec3::new(
            (r.next_double() * 0.2 - 0.1) * 2.0,
            r.next_double() * 0.2 * 2.0,
            (r.next_double() * 0.2 - 0.1) * 2.0,
        );
        let dot = roughly[0] * velocity.x + roughly[1] * velocity.y + roughly[2] * velocity.z;
        if len_sqr > 0.0 && dot < 0.0 {
            velocity = Vec3::new(-velocity.x, -velocity.y, -velocity.z);
        }
        self.ecs_orb_moving(value, [p.x, p.y, p.z], [velocity.x, velocity.y, velocity.z]);
        self.native_joins_flush(false, 0);
    }
    /// The player's pickups of orbs: the ECS moves and merges them.
    pub(in crate::game) fn tick_experience(&mut self) {
        if self.xp_delay > 0 {
            self.xp_delay -= 1;
        }
        let spectator = self.spectator();
        let player = self
            .player
            .as_ref()
            .filter(|p| !p.health.dead_or_dying() && !spectator);
        let feet = player.map(|p| [p.position.x, p.position.y, p.position.z]);
        self.ecs_player_orbs(feet);
    }

    /// `Player.dropEquipment` for the player who died: unless keep_inventory
    /// is on, `destroyVanishingCursedItems`, then `Inventory.dropAll` from
    /// `at` (the eyes less 0.3): slots 0..36, armor feet to head (36..39),
    /// the off hand (40), each thrown in a random direction. The mods hear
    /// the drops (`native_living_drops`) before they join the level.
    pub(super) fn drop_inventory(&mut self, at: [f64; 3], dying: &creatures::DyingDrops<'_>) {
        let mut stacks: Vec<ItemStack> = Vec::new();
        if !self
            .session
            .rules
            .bool_at(myriade_engine::gamerules::vanilla::KEEP_INVENTORY)
        {
            stacks.extend(self.inventory.iter_mut().filter_map(Option::take));
            stacks.extend(self.armor.iter_mut().rev().filter_map(Option::take));
            stacks.extend(self.offhand.take());
            stacks.extend(self.equipment.take_all());
            stacks.retain(|s| s.enchantment_level("minecraft:vanishing_curse") == 0);
            self.apply_equipment();
        }
        let stacks = stacks.into_iter().map(|stack| (stack, at, 0)).collect();
        let mut random = random_source::entity();
        self.drop_death_items(dying, stacks, 40, |game| {
            Some(myriade_engine::entities::drop_motion(
                &mut random,
                game.yaw,
                game.pitch,
                true,
            ))
        });
    }

    /// `new ItemEntity(level, x, y, z, stack, dx, dy, dz)`.
    pub(super) fn spawn_item(
        &mut self,
        stack: ItemStack,
        position: [f64; 3],
        velocity: [f64; 3],
        pickup_delay: u32,
    ) -> Option<u32> {
        self.spawn_prepared_item(stack, position, Some(velocity), pickup_delay, None).0
    }

    /// `new ItemEntity(level, x, y, z, stack)`: the item leaves with its own
    /// random's motion, which this returns.
    pub(super) fn spawn_item_own(
        &mut self,
        stack: ItemStack,
        position: [f64; 3],
        pickup_delay: u32,
    ) -> [f64; 3] {
        self.add_item_entity(stack, position, None, pickup_delay)
    }

    /// The dispenser constructs default motion before replacing it with the throw.
    pub(super) fn spawn_item_thrown(
        &mut self,
        stack: ItemStack,
        position: [f64; 3],
        velocity: [f64; 3],
    ) {
        let random = spill::prepare_item_random(None);
        self.add_item_entity_prepared(stack, position, Some(velocity), 0, Some(random));
    }

    pub(super) fn add_item_entity(
        &mut self,
        stack: ItemStack,
        position: [f64; 3],
        velocity: Option<[f64; 3]>,
        pickup_delay: u32,
    ) -> [f64; 3] {
        self.add_item_entity_prepared(stack, position, velocity, pickup_delay, None)
    }

    /// Block.popResource
    pub(super) fn pop_resource(&mut self, pos: [i32; 3], mut stack: ItemStack) {
        self.pending.attach(pos, &mut stack);
        // Block.popResource: Mth.nextDouble offsets from the level random; the
        // motion is `new ItemEntity(..)`'s own, from the item's random.
        let p = spill::pop_position(&mut self.world.random, pos);
        // Block.popResource draws its offsets before the private overload checks the gamerule.
        if !self
            .session
            .rules
            .bool_at(myriade_engine::gamerules::vanilla::BLOCK_DROPS)
        {
            return;
        }
        self.spawn_item_own(stack, p, 10);
    }

    /// `Containers.dropItemStack`: a crafting remainder leaves from inside the
    /// block, split by the level random, and can be picked up at once. Unlike
    /// `Block.popResource` it has no `block_drops` test.
    pub(super) fn drop_item_stack_containers(&mut self, pos: [i32; 3], stack: ItemStack) {
        let count = stack.count;
        let at = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
        for piece in
            horizon_world::level::removal::drop_item_stack_at(&mut self.world.random, at, count)
        {
            self.spawn_item_thrown(
                ItemStack {
                    count: piece.count,
                    ..stack.clone()
                },
                piece.pos,
                piece.motion,
            );
        }
    }

    /// `Block.popResourceFromFace`: the loot leaves the clicked face with the
    /// face's own motion instead of the item's random one.
    pub(super) fn pop_resource_from_face(
        &mut self,
        pos: [i32; 3],
        face: [i32; 3],
        mut stack: ItemStack,
    ) {
        self.pending.attach(pos, &mut stack);
        let (p, v) = spill::pop_position_from_face(&mut self.world.random, pos, face);
        // The private overload checks the gamerule after the offsets are drawn.
        if !self
            .session
            .rules
            .bool_at(myriade_engine::gamerules::vanilla::BLOCK_DROPS)
        {
            return;
        }
        self.spawn_item(stack, p, v, 10);
    }

    /// Player.drop: thrown from the eyes along the view direction. True when
    /// the selected stack's `on_dropped_by_player` Part kept it in the slot
    /// (`ServerPlayer.drop(boolean)`, as NeoForge asks it).
    pub(in crate::game) fn drop_held(&mut self, all: bool) -> bool {
        if self.eye_position().is_none() {
            return false;
        }
        let spectator = self.spectator();
        if !spectator && self.native_dropped_by_player_event(all) {
            return true;
        }
        let Some(thrown) = myriade_engine::items::take_held_drop(
            &mut self.inventory[self.selected],
            all,
            spectator,
        ) else {
            return false;
        };
        self.player_drop(thrown);
        false
    }

    /// A guest's drop key. A drop the held item's Part refused sends the
    /// inventory back, undoing the removal the client predicted.
    pub(in crate::game) fn guest_drop_held(&mut self, gi: usize, conn: u32, all: bool) {
        let kept = self.as_guest(conn, |g| g.drop_held(all)) == Some(true);
        if let Some(host) = self.lan.as_mut() {
            if kept {
                host.hub.send_inventory(gi);
            } else {
                host.hub.guests.update(gi, |g| g.told_selected());
            }
        }
    }

    /// `Player.createItemStackToDrop`: an item entity thrown from the eyes
    /// along the view direction. `thrown_from_hand` (`drop(stack, true)`, the
    /// drop key) names the player as the thrower; `drop(stack, false)`, for
    /// what no longer fits in the inventory, does not.
    pub(super) fn throw_from_eyes(
        &mut self,
        thrown: ItemStack,
        eye: [f64; 3],
        randomly: bool,
        thrown_from_hand: bool,
    ) -> Option<u32> {
        // `Player.drop` draws the spread from the player's own random. A throw with no player
        // in place (a `/give` remainder whose target this game doesn't hold) keeps a fresh one
        // rather than losing the item.
        let v = match self.player.as_mut() {
            Some(player) => myriade_engine::entities::drop_motion(
                &mut player.random,
                self.yaw,
                self.pitch,
                randomly,
            ),
            None => myriade_engine::entities::drop_motion(
                &mut random_source::entity(),
                self.yaw,
                self.pitch,
                randomly,
            ),
        };
        let at = [eye[0], eye[1] - f64::from(0.3f32), eye[2]];
        let uuid = thrown_from_hand.then(|| self.acting_player_uuid());
        if self.ecs.is_active() {
            return self.ecs_toss(thrown, at, v, uuid, !randomly);
        }
        // Only `drop(.., false)` tosses ask `item_toss`: a death's scatter (`randomly`) does not.
        let id = if randomly {
            self.spawn_item(thrown, at, v, 40)?
        } else {
            self.toss_item(thrown, at, v, uuid)?
        };
        if let (Some(e), Some(who)) = (self.ecs.entity_of(id), uuid) {
            self.ecs.set_thrower(e, who);
        }
        Some(id)
    }

    /// `FurnaceResultSlot.checkTakeAchievements`: the player took from the output of the
    /// furnace at `pos`, which pops the experience of what it made at them
    /// (`awardUsedRecipesAndPopExperience`) and forgets it.
    pub(super) fn award_furnace_recipes(&mut self, pos: (i32, i32, i32), at: [f64; 3]) {
        let used = match self.world.block_entities.get_mut(&pos) {
            Some(BlockEntity::Furnace(f)) => {
                let used = f.recipe_experience(world_recipes!(self));
                f.recipes_used.clear();
                used
            }
            _ => return,
        };
        for (count, experience) in used {
            let xp = myriade_engine::containers::create_experience(
                count,
                experience,
                &mut self.world.random,
            );
            self.spawn_orbs(at, xp);
        }
    }
}

#[cfg(test)]
mod escape_tests {
    use crate::game::ecs::test_support::active_server_game;
    use myriade_engine::items::ItemStack;
    use horizon_core::random::RandomSource;

    /// `Entity.moveTowardsClosestSpace` uses the item's own random, never the
    /// level's random. Seeded Java values pin vanilla's float arithmetic (#13043).
    #[test]
    #[ignore = "daily"]
    fn an_item_escapes_a_block_at_its_own_randoms_speed() {
        let mut game_resources = None;
        let mut game = active_server_game(&mut game_resources);
        crate::game::native_guest_turn_tests::lan_guests_on(&mut game);
        super::native_entity_refs::tests::prepare_entity_ticking_neighborhood(
            &mut game,
            horizon_core::position::ChunkPos::new(0, 0),
        );
        game.update_chunk_tickets();
        assert!(game.entity_ticking_area()(8, 8));
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        game.world.set(8, 64, 8, stone).unwrap();
        let stack = ItemStack::new("minecraft:stone", 2);
        game.spawn_item(stack, [8.5, 64.4, 8.5], [0.0, 0.0, 0.0], 10);
        let mut level = game.world.random.clone();
        game.ticked_entities_through = u32::MAX;
        game.tick_items();
        let item = game.ecs.item_views().pop().unwrap();
        // Up is the nearest open side. Independent Java Random(12345):
        // nextFloat()*0.2F+0.1F, widened before multiplying by widened 0.98F.
        assert_eq!(item.velocity.y, f64::from_bits(0x3fc5_9ef4_570f_5500));
        assert_eq!(game.world.random.next_float(), level.next_float());
    }
}

#[cfg(test)]
#[path = "tests/item_cactus_damage_tests.rs"]
mod item_cactus_damage_tests;
#[cfg(test)]
#[path = "tests/thrown_items_tests.rs"]
mod thrown_items_tests;
