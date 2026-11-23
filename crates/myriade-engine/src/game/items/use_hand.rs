use super::*;
use myriade_types::Hand;

/// The level's result for a block's answer: an answer that used the action is a
/// success, any other is a pass.
fn level_result(answer: myriade_types::InteractionResult) -> horizon_world::level::InteractionResult {
    use myriade_types::InteractionResult as Answer;
    use horizon_world::level::InteractionResult as Level;
    match answer {
        Answer::Success | Answer::SuccessServer => Level::Success,
        Answer::Consume => Level::Consume,
        Answer::Fail | Answer::Pass | Answer::TryWithEmptyHand => Level::Pass,
    }
}

impl Game<'_> {
    /// `Level.playSound(player, ...)`: a guest's client already predicted the
    /// fire-starter sound, so forward it to the other guests without leaving
    /// it in the broadcast capture.
    pub(super) fn play_block_use_sound(&mut self, sound: &str, at: [f64; 3], pitch: f32) {
        if self.lan_acting.is_some() {
            self.guest_block_sounds
                .push((sound.to_owned(), at, 1.0, pitch));
        } else {
            self.audio.play(sound, Some(at), 1.0, pitch);
        }
    }

    /// One hand's turn in `startUseItem`: `useItemOn` the block aimed at,
    /// then `useItem`. Whether the click ends here (it acted or failed).
    pub(in crate::game) fn use_hand(&mut self, block_hit: Option<&Hit>) -> Result<bool> {
        if self.guest_use.is_none()
            && let Some(hit) = block_hit
        {
            return self.use_target_then_item(hit);
        }
        self.use_failed = false;
        // isSecondaryUseActive with something in either hand keeps blocks
        // from reacting.
        let sneaking = self.sneak_held;
        let hands_full = self.stack_in(self.selected).is_some() || self.offhand.is_some();
        let mut anchor_passed = false;
        use myriade_types::TriState;
        let on_block = matches!(self.guest_use, Some(GuestUse::OnBlock(_)));
        let mut block_hit = block_hit;
        let (use_block, use_item) = (TriState::Default, TriState::Default);
        let block_phase = myriade_engine::spans::guard("use_block");
        // ServerPlayerGameMode.useItemOn: after the spectator and sneak gates, BlockState.useItemOn first. ITEM_USED_ON_BLOCK sees the stack from before it, whichever phase consumes.
        let held = self.held().cloned();
        let empty = ItemStack::new("minecraft:air", 0);
        let before = held.as_ref().unwrap_or(&empty);
        if use_block.or(!(sneaking && hands_full))
            && let Some(hit) = block_hit
        {
            let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
            let mut block_answer = None;
            let mut chains_sign = false;
            if block_answer.is_none() {
                // TntBlock.useItemOn belongs to the block phase, before
                // ItemStack.useOn's adventure can_place_on gate. It reads the
                // hand as the block phase's listeners left it, as
                // `state.useItemOn(player.getItemInHand(hand), ...)` does.
                if let Some(stack) = self.held().cloned()
                    && self.ignite_tnt(&stack)?
                {
                    return Ok(true);
                }
                if self.use_on_cauldron(hit.pos)? {
                    self.swing_arm();
                    return Ok(true);
                }
                if let Some(placed) = self.use_on_campfire(hit.pos)? {
                    if placed {
                        self.swing_arm();
                    }
                    return Ok(true);
                }
                if self.use_lectern(hit.pos, held.as_ref()) {
                    self.swing_arm();
                    return Ok(true);
                }
                if self.use_sign_applicator(hit.pos, held.as_ref()) {
                    self.swing_arm();
                    return Ok(true);
                }
                if self.use_golem_statue(hit.pos, held.as_ref()) {
                    self.swing_arm();
                    return Ok(true);
                }
                if let Some(swings) = self.use_shelf(hit) {
                    self.trigger_block("minecraft:item_used_on_block", hit.pos, before);
                    if swings {
                        self.swing_arm();
                    }
                    return Ok(true);
                }
                if self.use_composter(hit.pos, held.as_ref()) {
                    self.swing_arm();
                    return Ok(true);
                }
                if self.use_vault(hit.pos, held.as_ref())? {
                    self.swing_arm();
                    return Ok(true);
                }
                if self.use_pumpkin(hit.pos, hit.face, held.as_ref())? {
                    self.swing_arm();
                    return Ok(true);
                }
                if self.use_beehive(hit.pos, held.as_ref())? {
                    self.swing_arm();
                    return Ok(true);
                }
                if self.use_campfire_douse(hit, held.as_ref())? {
                    self.swing_arm();
                    return Ok(true);
                }
                block_answer = self.use_redstone_ore(hit.pos, held.as_ref())?;
                match self.use_respawn_anchor(hit.pos, held.as_ref())? {
                    respawn_anchor::AnchorUse::Charged => {
                        self.swing_arm();
                        return Ok(true);
                    }
                    // The main hand passes; its item is used, then the off hand's glowstone.
                    respawn_anchor::AnchorUse::Pass => anchor_passed = true,
                    respawn_anchor::AnchorUse::Other => {}
                }
                // Hanging signs chain on ceiling undersides and wall edges unless click commands run.
                chains_sign = self.chains_hanging_sign(hit.pos, hit.face, held.as_ref());
            }
            if block_answer.is_none() {
                use horizon_world::level::UseBranch::ItemOn;
                block_answer = self
                    .use_jukebox(hit.pos, state, held.as_ref(), ItemOn)
                    .or_else(|| self.use_decorated_pot(hit.pos, state, held.as_ref(), ItemOn))
                    .or_else(|| self.use_chiseled_bookshelf(hit, state, held.as_ref(), ItemOn));
            }
            // TRY_WITH_EMPTY_HAND: only the main hand's turn uses the block itself (useWithoutItem). The block's item behaviour hears both hands.
            if !anchor_passed && !chains_sign && (self.hand == Hand::Main || block_answer.is_some())
            {
                let face = match hit.face {
                    [0, 1, 0] => Direction::Up,
                    [0, -1, 0] => Direction::Down,
                    [0, 0, -1] => Direction::North,
                    [0, 0, 1] => Direction::South,
                    [-1, 0, 0] => Direction::West,
                    _ => Direction::East,
                };
                let player = self.use_context();
                let block_hit = horizon_core::geometry::BlockHit {
                    block: BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]),
                    direction: face,
                    location: Vec3::new(hit.point[0], hit.point[1], hit.point[2]),
                    inside: hit.inside,
                };
                let item = held.as_ref().map(|s| s.item.to_string());
                let adds_candle = self
                    .with_level(|b, _, _| b.cake_candle_state(state, item.as_deref()).is_some());
                let pots_flower = self
                    .with_level(|b, _, _| b.potted_flower_state(state, item.as_deref()).is_some());
                let sign_consumes = self.sign_consumes_click(hit.pos);
                use horizon_world::level::{InteractionResult, UseBranch};
                let item_result = match block_answer {
                    Some(answer) => Some(level_result(answer)),
                    None => self.with_level(|b, level, ticks| {
                        b.block_use_item_on(level, ticks, state, block_hit, player, item.as_deref())
                    }),
                };
                let mut empty_template_ran = false;
                let (result, branch) = if let Some(result) = item_result {
                    (result, UseBranch::ItemOn)
                } else if self.hand == Hand::Main {
                    if self
                            .res
                            .behaviours()
                            .data(state)
                            .is(horizon_world::level::classes::Class::Fence)
                        && self.use_knot(hit.pos, false)
                    {
                        self.trigger_default_block_use(hit.pos);
                        return Ok(true);
                    }
                    let answer = self
                        .use_jukebox(hit.pos, state, None, UseBranch::WithoutItem)
                        .or_else(|| {
                            self.use_decorated_pot(hit.pos, state, None, UseBranch::WithoutItem)
                        })
                        .or_else(|| {
                            self.use_chiseled_bookshelf(hit, state, None, UseBranch::WithoutItem)
                        });
                    let result = if let Some(answer) = answer {
                        level_result(answer)
                    } else {
                        empty_template_ran = true;
                        let player = self.use_context();
                        self.with_block_use_level(|b, level, ticks| {
                            b.use_without_item(level, ticks, state, block_hit, player)
                        })
                    };
                    (result, UseBranch::WithoutItem)
                } else {
                    (InteractionResult::Pass, UseBranch::ItemOn)
                };
                self.process_level_events()?;
                // CakeBlock.useItemOn: the candle is used up and counted.
                if block_answer.is_none()
                    && adds_candle
                    && result == horizon_world::level::InteractionResult::Success
                    && let Some(used) = held.as_ref()
                {
                    self.award_stat("used", &used.item, 1);
                    self.consume_held();
                }
                // FlowerPotBlock.useItemOn: the plant is potted, used up, and counted.
                if block_answer.is_none()
                    && pots_flower
                    && result == horizon_world::level::InteractionResult::Success
                    && let Some(used) = held.as_ref()
                {
                    self.award_stat("used", &used.item, 1);
                    self.award_custom_stat("pot_flower", 1);
                    self.consume_held();
                }
                // These effects belong to the vanilla method, not its Part replacement.
                if empty_template_ran && result == horizon_world::level::InteractionResult::Success
                {
                    let name = self
                        .res
                        .facts()
                        .get(self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]))
                        .name
                        .clone();
                    match &*name {
                        "minecraft:note_block" => self.award_custom_stat("tune_noteblock", 1),
                        "minecraft:bell" => self.award_custom_stat("bell_ring", 1),

                        _ => {}
                    }
                }
                let sign_passes = empty_template_ran
                    && branch == UseBranch::WithoutItem
                    && self.res.facts().get(state).sign_form().is_some()
                    && !sign_consumes;
                if result.consumes_action() && !sign_passes {
                    // ServerPlayerGameMode.useItemOn: ITEM_USED_ON_BLOCK with the stack before the use, or DEFAULT_BLOCK_USE.
                    match branch {
                        horizon_world::level::UseBranch::ItemOn => {
                            self.trigger_block("minecraft:item_used_on_block", hit.pos, before);
                        }
                        horizon_world::level::UseBranch::WithoutItem => {
                            self.trigger_default_block_use(hit.pos);
                        }
                    }
                    if result.should_swing() {
                        // SUCCESS swings on the client too; the beds, light, respawn anchor and sign's own uses are SUCCESS_SERVER.
                        if server_only_block_use(self.res.facts().get(state), branch) {
                            self.swing_arm();
                        } else {
                            self.swing_arm_predicted();
                        }
                    }
                    return Ok(true);
                }
            }
        }
        // RightClickBlock's useItem FALSE: no ItemStack.useOn, so only Item.use is left (a guest's UseItem asks for it).
        if !use_item.or(true) {
            if on_block {
                return Ok(false);
            }
            block_hit = None;
        }
        drop(block_phase);
        let _item_phase = myriade_engine::spans::guard("use_item");
        let stack = self.held().cloned();
        let info = stack
            .as_ref()
            .and_then(|s| self.items().get(&s.item))
            .cloned();
        if stack.is_some() && info.is_none() {
            return Ok(false);
        }
        // ServerPlayerGameMode.useItem and useItemOn: an item on cooldown is not used (PASS, so the other hand may still act), unless
        // RightClickBlock's useItem TRUE forces a block click's useOn, even with an empty hand (the broadcast phase can still handle it).
        let forced = on_block && use_item == TriState::True;
        if !forced
            && stack
                .as_ref()
                .zip(info.as_ref())
                .is_none_or(|(stack, info)| {
                    self.cooldowns
                        .is_on_cooldown(&myriade_engine::cooldowns::group(
                            stack,
                            info.use_cooldown.as_ref(),
                        ))
                })
        {
            return Ok(false);
        }
        // ItemStack.useOn: adventure's predicate selects Item.useOn before its event and Part, not just before the eventual block placement.
        if let Some(hit) = block_hit
            && !self.may_build()
            && !self.can_place_on_in_adventure(hit.pos)
        {
            if on_block {
                return Ok(false);
            }
            block_hit = None;
        }
        // An empty stack has no item Part or vanilla Item.useOn implementation.
        let (Some(stack), Some(info)) = (stack, info) else {
            return Ok(false);
        };
        // LeadItem.useOn follows the block and the item's override, including
        // when sneaking bypassed the block's useWithoutItem.
        if block_hit.is_some() && &*stack.item == "minecraft:lead" && self.interact_knot() {
            return Ok(true);
        }
        // ItemStack.useOn: after the block's own useItemOn/useWithoutItem
        // passed, and only where the player may use an item on that block.
        // DebugStickItem.useOn cycles the selected property.
        let may_use_on = block_hit.is_some_and(|hit| self.may_use_on(hit.pos));
        if may_use_on
            && let Some(hit) = block_hit
            && let Some(success) = self.debug_stick_use_on(hit.pos)?
        {
            // `Minecraft.startUseItem`: the stick's client-side `useOn` is
            // SUCCESS, so the actor's hand swings; a remote guest swings in
            // its own client, not on the server.
            if success {
                // `ServerPlayerGameMode.useItemOn`: a consuming `useOn` triggers ITEM_USED_ON_BLOCK.
                self.trigger_block("minecraft:item_used_on_block", hit.pos, before);
            } else {
                // DebugStickItem.useOn's FAIL.
                self.use_failed = true;
            }
            return Ok(true);
        }
        if may_use_on && self.use_bone_meal(&stack)? {
            return Ok(true);
        }
        // ShearsItem.useOn: shears crop a growing plant's head to its top age.
        if may_use_on
            && let Some(hit) = block_hit
            && self.shears_crop(hit.pos, &stack)?
        {
            return self.finish_item_block_use(block_hit, before);
        }
        // A guest's UseItemOn stops at ItemStack.useOn: what Item.use does
        // (throw, drink, eat, raise, equip) waits for its UseItem.
        let spawn_egg = stack.item.ends_with("_spawn_egg");
        use crate::hand_use::{UseResult};
        let swings = self.swings_started;
        match self.use_tool(&stack, block_hit.is_some())? {
            UseResult::Success { swing } => {
                if (UseResult::Success { swing }).should_swing(swings, self.swings_started) {
                    self.swing_arm();
                }
                return self.finish_item_block_use(block_hit, before);
            }
            UseResult::Fail => {
                self.use_failed = true;
                return Ok(true);
            }
            UseResult::Pass => {}
        }
        // PotionItem.useOn comes before PotionItem.use's drink.
        if may_use_on
            && let Some(hit) = block_hit
            && self.use_water_bottle_on_block(hit.pos, hit.face, &stack)?
        {
            return Ok(true);
        }
        if (!on_block && self.use_potion_item(&stack)?)
            || ((!on_block || (spawn_egg && block_hit.is_some())) && self.throw_item(&stack)?)
        {
            return Ok(true);
        }
        // BrushItem.useOn: brushing starts on the block aimed at (CONSUME).
        if block_hit.is_some() && &*stack.item == "minecraft:brush" {
            self.start_using_item();
            return Ok(true);
        }
        if !on_block
            && (self.raise_shield(self.held_slot())
                || self.use_fishing_rod(&stack)
            || self.use_crossbow(&stack)
            || self.use_trident(&stack)
            || self.use_spear(&stack)
            || self.boost_firework(&stack)
            || self.use_empty_map(&stack)
                || self.use_book(&stack)
                // KnowledgeBookItem.use: the recipes it names become known.
                || self.use_knowledge_book(&stack)
                // Block items (heads, carved pumpkins) aimed at a block are
                // placed first (useItemOn before useItem).
                || (!(info.block.is_some() && block_hit.is_some()) && self.equip_from_hand(&stack)))
        {
            return Ok(true);
        }
        // FoodOnAStickItem.use with no boost (not riding its mount, or the
        // mount refused): PASS, but counted as a use of the stick.
        if !on_block
            && matches!(
                &*stack.item,
                "minecraft:carrot_on_a_stick" | "minecraft:warped_fungus_on_a_stick"
            )
        {
            self.award_stat("used", &stack.item, 1);
        }
        // BundleItem.use: startUsingItem, then onUseTick empties it.
        if !on_block && info.bundle {
            self.start_using_item();
            self.swing_arm();
            return Ok(true);
        }
        if !on_block && &*stack.item == "minecraft:goat_horn" {
            self.use_instrument(&stack);
            return Ok(true);
        }
        if !on_block && &*stack.item == "minecraft:bow" {
            // BowItem.use: ArrowNockEvent, then FAIL without ammunition.
            let found_projectile = self.get_projectile(&stack).is_some();
            let has_ammo = found_projectile || self.creative;
            if has_ammo {
                self.start_using_item();
            }
            return Ok(true);
        }
        // Block foods attempt placement before starting consumption.
        if !on_block && (info.block.is_none() || block_hit.is_none()) && self.start_eating(&info) {
            return Ok(true);
        }
        // BoatItem.use also performs its own fluid/block raycast.
        if !on_block && self.place_boat(&stack) {
            return Ok(true);
        }
        // ItemStack.useOn passes in adventure mode unless the item's
        // can_place_on matches the clicked block; Item.use then runs as usual.
        if let Some(hit) = block_hit
            && self.may_use_on(hit.pos)
            && (self.launch_firework(hit, &stack)
                || self.place_minecart(hit, &stack)
                || self.place_painting(hit, &stack)
                || self.place_item_frame(hit, &stack)
                || self.use_lodestone_compass(hit, &stack, before)
                || self.use_map_on_banner(hit, &stack, before)
                || self.place_armor_stand(hit, &stack)
                || self.place_cushion(hit, &stack)
                || self.place_end_crystal(hit, &stack))
        {
            return Ok(true);
        }
        if self.use_ender_eye(&stack, block_hit.is_some())? {
            return Ok(true);
        }
        // PlaceOnWaterBlockItem.use: the lily pad and frogspawn aim at the block
        // or water source the SOURCE_ONLY ray meets and are placed on the
        // position above it.
        let water_placement = matches!(&*stack.item, "minecraft:lily_pad" | "minecraft:frogspawn");
        let water_hit;
        let hit = if water_placement {
            let Some(h) = self.pick_fluid() else {
                return Ok(false);
            };
            water_hit = Hit {
                pos: [h.pos[0], h.pos[1] + 1, h.pos[2]],
                ..h
            };
            &water_hit
        } else {
            let Some(hit) = block_hit else {
                return Ok(false);
            };
            hit
        };
        if info.block.is_none() {
            return Ok(false);
        }
        if !self.may_use_on(hit.pos) {
            // ItemStack.useOn passes in adventure mode; useItem eats.
            return Ok(!on_block && self.start_eating(&info));
        }
        let face = match hit.face {
            [0, 1, 0] => Direction::Up,
            [0, -1, 0] => Direction::Down,
            [0, 0, -1] => Direction::North,
            [0, 0, 1] => Direction::South,
            [-1, 0, 0] => Direction::West,
            _ => Direction::East,
        };
        // BlockItem.place through the vanilla placement rules.
        let ctx = PlaceContext {
            clicked_pos: BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]),
            face,
            hit: Vec3::new(hit.point[0], hit.point[1], hit.point[2]),
            inside: hit.inside,
            yaw: self.yaw,
            pitch: self.pitch,
            sneaking: self.sneak_held,
            item: stack.item.to_string(),
            can_use_game_master_blocks: self.can_use_game_master_blocks(),
        };
        let outcome = self.with_level(|b, level, ticks| b.place_block(level, ticks, &ctx));
        let PlaceOutcome::Placed { pos, state } = outcome else {
            self.process_level_events()?;
            // BlockItem.useOn: a consumable that could not be placed is used,
            // which ItemStack.useOn counts as a use of the item.
            if self.start_eating(&info) {
                self.award_stat("used", &stack.item, 1);
            } else {
                // BlockItem.place's FAIL.
                self.use_failed = true;
            }
            return Ok(true);
        };
        let target = [pos.x, pos.y, pos.z];
        let game_master = self.can_use_game_master_blocks();
        let state = self.lectern_placed(target, state, &stack, game_master)?;
        let state = self.apply_item_block_state(target, state, &stack)?;
        let key = (target[0], target[1], target[2]);
        if !self.world.block_entities.stateful_entity(state.0)
            && let Some(entity) = self.new_block_entity_for_state(state)
        {
            self.world.block_entities.insert(key, entity);
        }
        if self.world.block_entities.get(&key).is_some() {
            self.apply_placed_item(target, &stack, game_master);
            // Block.setPlacedBy.
            match self.world.block_entities.get(&key) {
                Some(BlockEntity::CommandBlock(_)) => {
                    let data = stack.data().is_some_and(|d| {
                        d.other(myriade_engine::block_entity_item::BLOCK_ENTITY_DATA)
                            .is_some()
                    });
                    self.command_block_placed(key, !data);
                }
                Some(BlockEntity::StructureBlock(_)) => self.structure_block_placed(key),
                Some(BlockEntity::TestBlock(_)) => self.test_block_placed(key),
                Some(BlockEntity::Jukebox(_)) => self.jukebox_placed(target, &stack),
                _ => {}
            }
        }
        // BlockItem.place's SUCCESS: the client swings with its own prediction.
        self.swing_arm_predicted();
        // BlockItem.place's sound is among the level events;
        // SolidBucketItem.getPlaceSound replaces it with the bucket's own.
        let solid_bucket = &*stack.item == "minecraft:powder_snow_bucket";
        if solid_bucket {
            for event in &mut self.level_events {
                if let LevelEvent::Sound { pos: at, sound, .. } = event
                    && *at == pos
                {
                    *sound = "minecraft:item.bucket.empty_powder_snow".into();
                }
            }
        }
        self.process_level_events()?;
        // BlockItem.place: the used statistic and the placed_block trigger.
        self.award_stat("used", &stack.item, 1);
        self.trigger_block("minecraft:placed_block", target, &stack);
        self.check_wither_spawn(target)?;
        self.check_golem_spawn(target)?;
        self.consume_held();
        // SolidBucketItem.useOn: BucketItem.getEmptySuccessItem in hand.
        if solid_bucket && !self.creative {
            *self.held_mut() = Some(ItemStack::new("minecraft:bucket", 1));
        }
        // ItemStack.useOn applies the pre-use stack's component side effects
        // after the successful placement and its hand replacement.
        self.apply_use_cooldown(&stack);
        Ok(true)
    }
}

impl Game<'_> {
    pub(super) fn apply_consume_effect(
        &mut self,
        effect: &myriade_engine::items::ConsumeEffect,
        mob: Option<u32>,
    ) -> Option<bool> {
        use myriade_engine::items::ConsumeEffect;
        Some(match effect {
            ConsumeEffect::ApplyEffects {
                effects,
                probability,
            } => {
                // ApplyStatusEffectsConsumeEffect draws from the user's random,
                // even at probability 1. The level random stands in for a player's.
                let draw = match mob {
                    Some(id) => self.ecs_next_float(id)?,
                    None => self.world.random.next_float(),
                };
                if draw >= *probability {
                    return Some(false);
                }
                let mut changed = false;
                for eff in effects {
                    let Some(def) = myriade_engine::brewing::effect(&eff.id) else {
                        continue;
                    };
                    if let Some(id) = mob {
                        changed |= self.ecs_mob_add_effect(
                            id,
                            def.id,
                            eff.duration,
                            eff.amplifier,
                            eff.show_particles,
                            eff.ambient,
                        );
                    } else {
                        let mut instance = myriade_engine::effects::EffectInstance::new(
                            def.id,
                            eff.duration,
                            eff.amplifier,
                        );
                        instance.ambient = eff.ambient;
                        instance.visible = eff.show_particles;
                        instance.show_icon = eff.show_icon;
                        changed |= self.add_player_effect(instance);
                    }
                }
                changed
            }
            ConsumeEffect::ClearAllEffects => match mob {
                Some(id) => self.ecs.retain_effects(id, &[]),
                None => self.remove_player_effects(None),
            },
            ConsumeEffect::RemoveEffects(effects) => {
                let mut changed = false;
                for effect in effects {
                    changed |= match mob {
                        Some(id) => myriade_engine::brewing::effect(effect)
                            .is_some_and(|def| self.ecs.remove_effect(id, def.index as i32)),
                        None => myriade_engine::brewing::effect(effect)
                            .is_some_and(|def| self.remove_player_effects(Some(def.index))),
                    };
                }
                changed
            }
            ConsumeEffect::TeleportRandomly(teleport) => {
                if mob.is_some() {
                    return None;
                }
                self.teleport_randomly(teleport)
            }
            ConsumeEffect::PlaySound(sound) => {
                let at = match mob {
                    Some(id) => {
                        let p = self.ecs_vec3(id)?;
                        horizon_core::geometry::Vec3::new(p[0], p[1], p[2])
                    }
                    None => self.player.as_ref()?.position,
                };
                let b = horizon_core::position::BlockPos::containing(at);
                let at = [b.x, b.y, b.z].map(|v| f64::from(v) + 0.5);
                self.play_block_use_sound(sound, at, 1.0);
                true
            }
            // A native effect is handled by no code here: it changes nothing.
            ConsumeEffect::Native { .. } => false,
        })
    }
    /// BowItem.releaseUsing: power from charge time, arrows at power * 3, as
    /// many as the weapon's `projectile_count` says, fanned by its
    /// `projectile_spread`.
    pub(super) fn release_bow(&mut self, stack: &ItemStack, ticks: u32) {
        let ammo = self.get_projectile(stack);
        let charge = i32::try_from(ticks).unwrap_or(i32::MAX);
        let power = item_use::bow_release_power(ammo.is_some(), charge, false);
        let (Some(power), Some(ammo)) = (power, ammo) else {
            return;
        };
        let arrow_slot = ammo.slot;
        let weapon = Some(stack.clone());
        let Some(eye) = self.eye_position() else {
            return;
        };
        // BowItem.shootProjectile: shootFromRotation(power * 3, inaccuracy 1).
        let p = Vec3::new(eye[0], eye[1] - 0.1, eye[2]);
        let ammo = ammo.stack;
        let (power_bonus, infinity) = self.bow_enchantments(&ammo.item);
        let effect = dispenser::tipped_effect(&ammo).or_else(|| {
            (&*ammo.item == "minecraft:spectral_arrow").then_some(("minecraft:glowing", 200))
        });
        let kind = myriade_engine::projectiles::ProjectileKind::Arrow {
            base_damage: 2.0 + power_bonus,
            critical: power >= 1.0,
            effect,
        };
        // ProjectileWeaponItem.draw and shoot: the weapon's enchantments decide
        // how many arrows leave and how far apart they fan (Multishot: two more
        // arrows, ten degrees of spread).
        let (count, spread) = self.bow_volley(stack);
        let (velocity, on_ground) = self.player.as_ref().map_or((Vec3::ZERO, true), |pl| {
            (self.known_player_movement(), pl.ground.on_ground)
        });
        // ProjectileWeaponItem.shoot: an odd count is centred on the view, the
        // arrows alternate either side of it, `direction` flipping per arrow.
        // A count of 0 draws nothing and vanilla skips `shoot` altogether.
        let step = if count <= 1 {
            0.0
        } else {
            2.0 * spread / (count - 1) as f32
        };
        let offset = (count.saturating_sub(1) % 2) as f32 * step / 2.0;
        let mut direction = 1.0f32;
        for index in 0..count {
            let angle = offset + direction * index.div_ceil(2) as f32 * step;
            direction = -direction;
            let mut arrow = myriade_engine::projectiles::Projectile::new(
                kind.clone(),
                p,
                Some(myriade_engine::mobs::Who::Player),
            );
            // AbstractArrow's constructor asks the weapon for its piercing count.
            let pierce = self.bow_pierce(stack, &ammo);
            // ProjectileWeaponItem.useAmmo: creative and Infinity draw without
            // using the ammo, so the arrow is CREATIVE_ONLY, and only the first
            // of a volley is the player's to take; else setOwner(player) lets
            // it be picked up.
            if let Some(a) = arrow.arrow.as_mut() {
                a.set_item(&myriade_engine::arrows::drawn_ammo(
                    &ammo,
                    self.creative || infinity || index > 0,
                ));
                a.set_owner(Some(myriade_engine::mobs::Who::Player));
                a.fired_from_weapon = weapon.clone();
                a.pierce = pierce;
            }
            // Flame: the arrow is spawned burning (EnchantmentHelper.onProjectileSpawned).

            // BowItem.shootProjectile: shootFromRotation(power * 3, inaccuracy 1).
            arrow.shoot_from_rotation(
                self.pitch,
                self.yaw + angle,
                0.0,
                power * 3.0,
                1.0,
                velocity,
                on_ground,
            );
            let shot = self.arrow_shot_of(arrow.arrow.as_deref().expect("an arrow has its state"), effect);
            let (pos, vel) = (arrow.position, arrow.velocity);
            let owner = self.ecs_owner(Some(myriade_engine::mobs::Who::Player));
            self.ecs_shoot_arrow([pos.x, pos.y, pos.z], [vel.x, vel.y, vel.z], owner, shot, weapon.clone());
            // ProjectileWeaponItem.shoot: one point of wear per arrow, and a
            // bow that breaks mid-volley shoots no more.
            let held = self.held().is_some();
            self.damage_held(1);
            if held && self.held().is_none() {
                break;
            }
        }
        // ArrowState posts projectile_shoot on the arrow's first tick.
        let pitch = 1.0 / (self.world.random.next_float() * 0.4 + 1.2) + power * 0.5;
        // BowItem.releaseUsing: `playSound(null, getX(), getY(), getZ(), ...)`, at
        // the feet: an eye-height source a few cm from the listener pans hard.
        let feet = self
            .player
            .as_ref()
            .map_or(eye, |pl| [pl.position.x, pl.position.y, pl.position.z]);
        self.audio
            .play("minecraft:entity.arrow.shoot", Some(feet), 1.0, pitch);
        self.award_stat("used", "minecraft:bow", 1);
        // useAmmo takes one arrow for the first drawn projectile and none for
        // the rest, and none at all when the weapon draws none.
        if count > 0
            && !self.creative
            && !infinity
            && let Some(i) = arrow_slot
        {
            let cell = self.stack_in_mut(i);
            if let Some(s) = cell.as_mut() {
                s.count -= 1;
                if s.count == 0 {
                    *cell = None;
                }
            }
        }
    }
    /// `EnchantmentHelper.processProjectileCount` and `processProjectileSpread`
    /// for `weapon`: the arrows a shot draws (1, plus Multishot's 2) and the
    /// spread they fan over (0, plus Multishot's 10 degrees).
    fn bow_volley(&mut self, weapon: &ItemStack) -> (usize, f32) {
        let ctx = myriade_engine::enchant::EffectContext {
            this: Some("minecraft:player"),
            items: Some(world_items!(self)),
            ..Default::default()
        };
        let enchantments = &world_stations!(self).enchantments;
        let count = enchantments.modify_value(
            Some(weapon),
            "minecraft:projectile_count",
            &ctx,
            &mut self.world.random,
            1.0,
        );
        let spread = enchantments.modify_value(
            Some(weapon),
            "minecraft:projectile_spread",
            &ctx,
            &mut self.world.random,
            0.0,
        );
        (count.floor().max(0.0) as usize, spread.max(0.0))
    }
    /// `EnchantmentHelper.getPiercingCount` for `ammo`: the weapon's
    /// `projectile_piercing` (Piercing, one entity per level).
    fn bow_pierce(&mut self, weapon: &ItemStack, ammo: &ItemStack) -> u8 {
        let ctx = myriade_engine::enchant::EffectContext {
            tool: Some(&ammo.item),
            items: Some(world_items!(self)),
            ..Default::default()
        };
        let pierce = world_stations!(self).enchantments.modify_value(
            Some(weapon),
            "minecraft:projectile_piercing",
            &ctx,
            &mut self.world.random,
            0.0,
        );
        pierce.floor().clamp(0.0, 127.0) as u8
    }

    /// One hand's block use, then its item's, for a click on a block: the block
    /// first (`useItemOn`), then `use_hand` without it (`useItem`).
    pub(super) fn use_target_then_item(&mut self, hit: &Hit) -> Result<bool> {
        self.guest_use = Some(GuestUse::OnBlock(self.hand));
        let result = self.with_block_use_original(self.hand, |g| {
            let result = g.use_hand(Some(hit));
            if matches!(result, Ok(true)) && !g.use_failed {
                g.finish_block_use(hit.pos, g.hand);
            }
            result
        });
        self.guest_use = None;
        if result? {
            return Ok(true);
        }
        self.use_hand(None)
    }

    /// The game mode's item-phase trigger is separate from item-internal triggers.
    fn finish_item_block_use(&mut self, hit: Option<&Hit>, original: &ItemStack) -> Result<bool> {
        if let Some(hit) = hit {
            self.trigger_block("minecraft:item_used_on_block", hit.pos, original);
        }
        Ok(true)
    }

    /// Runs one block use with an isolated original stack, restoring the outer
    /// operation even when a listener unwinds.
    pub(super) fn with_block_use_original<R>(
        &mut self,
        hand: Hand,
        action: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let original = self.with_hand(hand, |g| g.held().cloned());
        let depth = self.block_use_original.len();
        self.block_use_original.push((hand, original));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| action(self)));
        self.block_use_original.truncate(depth);
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    /// Completes a consuming block use with the stack held when it started, so
    /// `ITEM_USED_ON_BLOCK` and the advancements see the stack vanilla passed.
    pub(super) fn finish_block_use(&mut self, pos: [i32; 3], _hand: Hand) {
        let stack = self
            .block_use_original
            .last_mut()
            .and_then(|(_, stack)| stack.take())
            .unwrap_or_else(|| ItemStack::new("minecraft:air", 0));
        myriade_engine::spans::time("use_advancements", || {
            self.trigger_any_block_use(pos, &stack);
        });
    }

    /// `Block.newBlockEntity` for the state a placed item leaves. No constructor from a
    /// state exists in the tree, so none is made.
    pub(in crate::game) fn new_block_entity_for_state(&mut self, _state: horizon_world::registry::BlockStateId) -> Option<BlockEntity> {
        None
    }
}


#[cfg(test)]
#[path = "tests/bow_enchantment_tests.rs"]
mod bow_enchantment_tests;

#[cfg(test)]
#[path = "tests/jigsaw_place_tests.rs"]
mod jigsaw_place_tests;

#[cfg(test)]
#[path = "tests/scaffolding_inside_tests.rs"]
mod scaffolding_inside_tests;
