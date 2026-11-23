//! Armor stands: `ArmorStandItem.useOn` placement (snapped to 45 degrees),
//! falling, `interactAt` equipment swaps by clicked height, the two-hit break
//! of `hurtServer` (creative breaks at once), and `ArmorStandRenderer` with
//! the default pose, the hit wobble and humanoid armor layers.
use super::entity::Entity;
use super::{Game, InteractTarget};
use horizon_core::random::RandomSource;
use myriade_engine::{
    Result,
    entities::ArmorStand,
    items::ItemStack,
    mobs::{Who, anim::Poses},
};
use horizon_core::{
    geometry::{Aabb, Vec3},
    position::BlockPos,
};
use bevy_ecs::entity::Entity as EcsEntity;
use horizon_ecs_proto::{
    bridge::{host::Select, ids::id_of},
    cow::{Bounds, Cause, Contact, Health, Pos},
    equipment as ecs_equipment, goals,
    items::{item_id, item_name},
    stand::{self as ecs_stand, Blow, Stand as EcsStand},
};
pub(in crate::game) const TYPE_ID: &str = "minecraft:armor_stand";

/// Velocity `AbstractMinecart.push` adds: the cart, and a quarter on the stand.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CartShove {
    cart_x: f64,
    cart_z: f64,
    stand_x: f64,
    stand_z: f64,
}

impl Game<'_> {
    /// The network ids of the armor stands the ECS holds.
    pub(in crate::game) fn armor_stand_ids(&self) -> Vec<u32> {
        self.ecs.species_ids(TYPE_ID)
    }
    /// The armor stands the ECS holds, read from their saved compounds.
    pub(in crate::game) fn armor_stand_list(&self) -> Vec<ArmorStand> {
        self.armor_stand_ids().into_iter().filter_map(|id| self.armor_stand_saved(id)).collect()
    }
    /// The armor stand `id` as its saved NBT gives it.
    pub(in crate::game) fn armor_stand_saved(&self, id: u32) -> Option<ArmorStand> {
        let e = self.ecs.entity_of(id)?;
        let saved = self.ecs.entity_nbt(e)?;
        // Any compound reads as a stand: only one whose `id` says so is one.
        if !matches!(saved.get(&"id".into()), Some(horizon_core::nbt::Tag::String(k)) if *k == TYPE_ID.into()) {
            return None;
        }
        myriade_engine::entity_nbt::armor_stand_from_nbt(&horizon_core::nbt::Tag::Compound(saved), id)
    }
    /// `f` changes the armor stand `id` as its saved state reads, and the result is loaded back onto the ECS entity.
    pub(in crate::game) fn armor_stand_edit(&mut self, id: u32, f: impl FnOnce(&mut ArmorStand)) -> bool {
        use horizon_core::nbt::Tag;
        let Some(e) = self.ecs.entity_of(id) else { return false };
        let Some(saved) = self.ecs.entity_nbt(e) else { return false };
        if !matches!(saved.get(&"id".into()), Some(Tag::String(k)) if *k == TYPE_ID.into()) {
            return false;
        }
        let Some(mut a) = myriade_engine::entity_nbt::armor_stand_from_nbt(&Tag::Compound(saved.clone()), id) else { return false };
        f(&mut a);
        let uuid = myriade_engine::entity_nbt::uuid(&Tag::Compound(saved.clone())).unwrap_or([0; 4]);
        let Tag::Compound(patch) = myriade_engine::entity_nbt::armor_stand_to_nbt(&a, uuid, Some(&Tag::Compound(saved))) else { return false };
        self.ecs.merge_nbt(e, &patch)
    }
    /// An armor stand leaves the level without drops (`Entity.discard`).
    pub(in crate::game) fn armor_stand_remove(&mut self, id: u32) -> bool {
        let Some(e) = self.ecs.entity_of(id) else { return false };
        self.ecs.despawn(e);
        self.native_entity_left(id);
        true
    }
    /// `ArmorStandItem.useOn`.
    /// `ArmorStandItem.useOn` up to its `FAIL`s, which a client checks too:
    /// not on a bottom face, and nothing in the new stand's box. The block it
    /// stands in and the stand.
    pub(super) fn armor_stand_spawn(
        &self,
        hit: &super::Hit,
        stack: &ItemStack,
    ) -> Option<([i32; 3], ArmorStand)> {
        if hit.face == [0, -1, 0] {
            return None;
        }
        // BlockPlaceContext: a replaceable clicked block is replaced.
        let clicked = self
            .res
            .facts()
            .get(self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]));
        let pos = if clicked.replaceable {
            hit.pos
        } else {
            std::array::from_fn(|i| hit.pos[i] + hit.face[i])
        };
        let foot = Vec3::new(
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]),
            f64::from(pos[2]) + 0.5,
        );
        let yaw =
            (((self.yaw - 180.0 + 180.0).rem_euclid(360.0) - 180.0 + 22.5) / 45.0).floor() * 45.0;
        let mut stand = ArmorStand::new(self.next_entity + 1, foot, yaw);
        // Entity.applyImplicitComponents: a name from an anvil names the stand.
        if let Some(name) = stack.custom_name() {
            stand.custom_name = Some(myriade_engine::text::Text::literal(name));
        }
        let b = stand.bounds();
        // Touching the floor is not a collision.
        let (lo, hi) = (b.min_position(), b.max_position());
        let e = 1.0e-7;
        let bounds = Aabb::new(
            Vec3::new(lo.x + e, lo.y + e, lo.z + e),
            Vec3::new(hi.x - e, hi.y - e, hi.z - e),
        );
        let mut scene = myriade_engine::collisions::SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        if !scene.unobstructed(bounds).unwrap_or(false)
            || self.ecs.species_bounds(TYPE_ID).into_iter().any(|o| overlaps(o, bounds))
        {
            return None;
        }
        Some((pos, stand))
    }
    pub(super) fn place_armor_stand(&mut self, hit: &super::Hit, stack: &ItemStack) -> bool {
        if &*stack.item != "minecraft:armor_stand" {
            return false;
        }
        let Some((pos, mut stand)) = self.armor_stand_spawn(hit, stack) else {
            return false;
        };
        // EntityType.create(.., tryMoveDown, movedUp): the new stand falls onto what is under it,
        // up to two blocks (a bottom slab, the block under replaced grass).
        let drop = self.drop_offset_from(0.5, 1.975, (pos[0], pos[1], pos[2]), true);
        stand.position.y = f64::from(pos[1]) + drop;
        stand.previous = stand.position;
        // `EntityType.createDefaultStackConfig`: the item's `custom_data`, then its
        // `entity_data` merged over the stand; `ArmorStandItem.useOn` then snaps it
        // to the placement yaw with pitch 0.
        let own = myriade_engine::entity_nbt::armor_stand_to_nbt(&stand, [0; 4], None);
        let mut raw = myriade_engine::block_entity_item::stack_configured_entity(
            stack,
            "minecraft:armor_stand",
            &own,
        );
        if let Some(mut loaded) = myriade_engine::entity_nbt::armor_stand_from_nbt(&raw, stand.id) {
            loaded.first_tick = stand.first_tick;
            loaded.yaw = stand.yaw;
            loaded.previous_yaw = stand.previous_yaw;
            loaded.pitch = 0.0;
            loaded.previous = loaded.position;
            stand = loaded;
        }
        if let horizon_core::nbt::Tag::Compound(map) = &mut raw {
            map.remove(&"UUID".into());
        }
        let horizon_core::nbt::Tag::Compound(saved) = &raw else {
            return false;
        };
        let Some(id) = self.ecs_load(saved) else {
            return false;
        };
        let foot = stand.position;
        // EntityType.create's spawn yaw, `level.getRandom().nextFloat() * 360`, before
        // ArmorStandItem.useOn turns the stand to face the player.
        self.world.random.next_float();
        let (_, uuid) = myriade_engine::mobs::living_random();
        self.persist.uuids.insert(id, uuid);
        self.audio.play(
            "minecraft:entity.armor_stand.place",
            Some([foot.x, foot.y, foot.z]),
            0.75,
            0.8,
        );
        // ArmorStandItem.useOn: the stand's ENTITY_PLACE, by the player.
        let ctx = self.player_context(None);
        self.post_game_event("minecraft:entity_place", [foot.x, foot.y, foot.z], ctx);
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        if !self.creative {
            self.consume_held();
        }
        // ItemStack.useOn counts the use.
        self.award_stat("used", &stack.item, 1);
        // ArmorStandItem.useOn returns SUCCESS: the hand swings.
        self.swing_arm();
        true
    }
    fn equipment_slot(&self, stack: &ItemStack) -> usize {
        stand_slot(self.res.items(), stack)
    }
    /// The ECS stand under the crosshair: the nearest the ray meets within reach, by the box P keeps (`Bounds`), among the stands
    /// the spatial index holds near the eye. Markers and the camera are no target (`armor_stand_hit` on the level).
    pub(in crate::game) fn armor_stand_hit(&self) -> Option<(u32, f64, [f64; 3])> {
        let (eye, dir) = self.aim()?;
        let reach: f64 = self.entity_reach;
        let w = self.ecs.world()?;
        let q = Select { kind: Some(TYPE_ID.to_owned()), tag: None, near: Some((eye, reach)), limit: 64 };
        let mut best: Option<(u32, f64, [f64; 3])> = None;
        for e in self.ecs.select(&q) {
            let Some(id) = id_of(w, e) else { continue };
            if w.get::<EcsStand>(e).is_some_and(|s| s.flags & ecs_stand::MARKER != 0) {
                continue;
            }
            let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { continue };
            let (lo, hi) = (b.min_position(), b.max_position());
            let (lo, hi) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
            let (mut t0, mut t1) = (0.0f64, reach);
            let mut hit = true;
            for k in 0..3 {
                if dir[k].abs() < 1e-9 {
                    hit &= eye[k] >= lo[k] && eye[k] <= hi[k];
                    continue;
                }
                let (mut ta, mut tb) = ((lo[k] - eye[k]) / dir[k], (hi[k] - eye[k]) / dir[k]);
                if ta > tb {
                    std::mem::swap(&mut ta, &mut tb);
                }
                t0 = t0.max(ta);
                t1 = t1.min(tb);
            }
            if hit && t0 <= t1 && best.is_none_or(|b| t0 < b.1) {
                best = Some((id, t0, std::array::from_fn(|k| eye[k] + dir[k] * t0)));
            }
        }
        best
    }
    /// `interact_armor_stand` on the ECS level: the slot and the swap are P's (`click_target`, `equipment::set_slot`); the host keeps
    /// the held stack, the game events, the swing and the chunk's mark. A name tag needs P's `stand::name_with_tag` (not in P yet).
    pub(in crate::game) fn interact_armor_stand(&mut self, target: Option<InteractTarget>) -> bool {
        let (id, point) = match target {
            Some(t) => {
                let Ok(tid) = u32::try_from(t.id) else { return false };
                let Some(p) = self.ecs.position(tid) else { return false };
                (tid, [p[0] + t.location[0], p[1] + t.location[1], p[2] + t.location[2]])
            }
            None => {
                let Some((id, d, point)) = self.armor_stand_hit() else { return false };
                if !self.before_block(d) {
                    return false;
                }
                (id, point)
            }
        };
        let (Some(e), Some(pos)) = (self.ecs.entity_of(id), self.ecs.position(id)) else { return false };
        let at = pos;
        let held = self.held().cloned();
        if let Some(stack) = held.as_ref().filter(|h| &*h.item == "minecraft:name_tag") {
            let name = stack.custom_name();
            let Some(name) = name else { return false };
            let alive = self.ecs.world().and_then(|w| w.get::<Health>(e)).is_some_and(|h| h.0 > 0.0);
            if alive {
                if let Some(w) = self.ecs.world_mut() {
                    horizon_ecs_proto::custom_name::set(w, e, Some(name.clone()));
                }
                self.consume_held();
                if let (Some(player), Some(player_name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) {
                    self.native_player_name_entity(player, &player_name, id, &name);
                }
            }
            let ctx = self.who_context(Some(Who::Player), None);
            self.post_game_event("minecraft:entity_interact", at, ctx);
            self.swing_arm();
            return true;
        }
        let small = self.ecs.world().and_then(|w| w.get::<EcsStand>(e)).is_some_and(|s| s.flags & ecs_stand::SMALL != 0);
        let held_slot = held.as_ref().map(|h| self.equipment_slot(h));
        let rel_y = ((point[1] - pos[1]) / if small { 0.5 } else { 1.0 }) as f32;
        let Some(w) = self.ecs.world() else { return false };
        let slot = match ecs_stand::click_target(w, e, held.is_some(), held_slot, rel_y) {
            Some(s) => s,
            None if held_slot.is_some_and(|s| ecs_stand::is_disabled(w, e, s)) => return true,
            None => return false,
        };
        let Some(slot_enum) = ecs_stand::slot_of_index(slot) else { return false };
        let current = goals::equipped(w, e, slot_enum);
        let held_id = held.as_ref().map_or(0, |h| item_id(&h.item));
        let creative = self.creative;
        if held.as_ref().is_some_and(|h| h.count > 1) && current != 0 {
            return true;
        }
        // A server's mods hear the swap before it happens, and may stop it; the stands are found again by id after the hook.
        if held.as_ref().is_none_or(|h| h.count <= 1 || current == 0)
            && target.is_some()
            && let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone())
        {
            let slot_name = ["mainhand", "offhand", "feet", "legs", "chest", "head"][slot];
            let mut cancel = false;
            self.native_armor_stand_manipulate(player, &name, id as i32, slot_name, &mut cancel);
            if cancel {
                return true;
            }
            let key = |s: &Option<ItemStack>| s.as_ref().map(|s| (s.item.clone(), s.count));
            let Some(w) = self.ecs.world() else { return true };
            if goals::equipped(w, e, slot_enum) != current
                || key(&self.held().cloned()) != key(&held)
                || ecs_stand::swap_locked(w, e, slot, current != 0)
            {
                return true;
            }
        }
        let Some(w) = self.ecs.world_mut() else { return false };
        ecs_equipment::set_slot(w, e, slot_enum, held_id);
        let new_held = match held {
            Some(h) if creative && current == 0 => Some(h),
            Some(h) if h.count > 1 => Some(ItemStack { count: h.count - 1, ..h }),
            _ => item_name(current).map(|n| ItemStack::new(n, 1)),
        };
        *self.held_mut() = new_held;
        self.world.modified.insert(((at[0].floor() as i32) >> 4, (at[2].floor() as i32) >> 4));
        self.swing_arm();
        true
    }
    /// `attack_armor_stand` on the ECS level: the pick is `armor_stand_hit`, the hit is `hit_armor_stand`.
    pub(in crate::game) fn attack_armor_stand(&mut self) -> bool {
        let Some((id, d, _)) = self.armor_stand_hit() else { return false };
        if !self.before_block(d) || self.mob_hit().is_some_and(|m| m.1 < d) {
            return false;
        }
        let Some(at) = self.ecs.position(id) else { return false };
        if !self.may_interact_at(BlockPos::new(at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32)) {
            return true;
        }
        let ((_, scale, crit, magic), admitted) = self.melee_damage_with_admission("minecraft:armor_stand", true);
        let sweep = self.sweep_before_hit("minecraft:armor_stand", crit);
        let hurt = self.hit_armor_stand(id, |g, area, id| {
            let knockback = if admitted { g.extra_knockback("minecraft:armor_stand", scale).0 } else { 0.0 };
            g.attack_effects(
                area,
                sweep,
                Some(id as i32),
                crit,
                super::mace::Spared::Stand(id),
                magic.then_some(id as i32),
            );
            g.special_attack_slowdown(knockback);
        });
        self.attack_ticks = 0;
        if hurt {
            if admitted {
                self.special_attack_exhaustion();
            }
            self.swing_arm();
        }
        hurt
    }
    /// `hit_armor_stand` on the ECS level: the box, the centre and the ground are read before the blow may remove the stand.
    pub(in crate::game) fn hit_armor_stand(&mut self, id: u32, before_weapon: impl FnOnce(&mut Self, Aabb, u32)) -> bool {
        let Some(e) = self.ecs.entity_of(id) else { return false };
        let Some(w) = self.ecs.world() else { return false };
        let (Some(area), Some(center), Some(on_ground)) = (
            w.get::<Bounds>(e).map(|b| b.0),
            w.get::<Pos>(e).map(|p| p.0),
            w.get::<Contact>(e).map(|c| c.on_ground),
        ) else {
            return false;
        };
        let hurt = self.punch_armor_stand(id);
        if hurt {
            before_weapon(self, area, id);
            self.weapon_hit_stand(id, center, on_ground);
        }
        hurt
    }
    /// `punch_armor_stand` on the ECS level: the invulnerability check as the host does, then P's `stand::blow` decides the
    /// wobble or the break, and the host plays its sounds and posts its game events from that answer.
    pub(in crate::game) fn punch_armor_stand(&mut self, id: u32) -> bool {
        let Some(e) = self.ecs.entity_of(id) else { return false };
        let attacker = self.own_entity_id();
        let vanilla = self.saved_refuses_damage(id, "minecraft:player_attack", self.creative);
        let source = myriade_engine::health::HurtSource {
            damage_type: "minecraft:player_attack",
            causing: attacker,
            direct: attacker,
            position: None,
        };
        if super::entity::damage_level_of!(self, scoped).invulnerability_check(
            myriade_engine::health::Hurting::ArmorStand(id as i32, source),
            vanilla,
        ) {
            return false;
        }
        let (creative, by) = (self.creative, Some(super::commands::Target::Player));
        self.break_armor_stand_by_hit(e, creative, false, by)
    }
    /// `ArmorStand.hurtServer` for a blow on an ECS stand: P's `stand::blow` says wobble or break; the host plays the sounds and
    /// posts the game events from that answer. True: the blow was taken.
    pub(in crate::game) fn break_armor_stand_by_hit(&mut self, e: EcsEntity, creative: bool, always_kills: bool, by: Option<super::commands::Target>) -> bool {
        let now = self.world.game_time.max(0) as u64;
        let Some(at) = self.ecs.world().and_then(|w| w.get::<Pos>(e)).map(|p| [p.0.x, p.0.y, p.0.z]) else { return false };
        let Some(w) = self.ecs.world_mut() else { return false };
        match ecs_stand::blow(w, e, now, creative, always_kills) {
            Blow::Wobble => {
                self.audio.play("minecraft:entity.armor_stand.hit", Some(at), 0.3, 1.0);
            }
            Blow::Break => self.break_armor_stand_ecs(e, at, creative, by),
        }
        true
    }
    /// The break of an ECS stand by a blow (`brokenByPlayer`, or at once when creative): the host pops the stand item and the
    /// equipment above the cell and plays the sounds, then the entity is despawned. The loot roll is not in this branch yet.
    fn break_armor_stand_ecs(&mut self, e: EcsEntity, at: [f64; 3], creative: bool, by: Option<super::commands::Target>) {
        let (width, height) = self
            .ecs
            .world()
            .and_then(|w| w.get::<EcsStand>(e).copied())
            .map_or((0.5, 1.975), ecs_stand::stand_size);
        if creative {
            self.audio.play("minecraft:entity.armor_stand.break", Some(at), 1.0, 1.0);
            self.ecs.despawn(e);
            return;
        }
        let block = [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32];
        let mut worn = Vec::new();
        if let Some(w) = self.ecs.world_mut() {
            for slot in (0..8).filter_map(ecs_stand::slot_of_index) {
                if let Some(name) = item_name(ecs_equipment::take_slot(w, e, slot)) {
                    worn.push(ItemStack::new(name, 1));
                }
            }
        }
        self.pop_resource(block, ItemStack::new(TYPE_ID, 1));
        self.audio.play("minecraft:entity.armor_stand.break", Some(at), 1.0, 1.0);
        for stack in worn {
            self.pop_resource([block[0], block[1] + 1, block[2]], stack);
        }
        self.ecs.despawn(e);
    }
    /// `break_armor_stand_by_anything` on the ECS level: the equipment above the cell pops (P's `take_slot`), the sound, the
    /// killed event, then the entity is despawned. The loot roll and the vanishing filter are not in this branch yet.
    pub(in crate::game) fn break_armor_stand_by_anything(&mut self, e: EcsEntity, id: u32, causing: u32) {
        let Some(at) = self.ecs.world().and_then(|w| w.get::<Pos>(e)).map(|p| [p.0.x, p.0.y, p.0.z]) else { return };
        let pos = [at[0].floor() as i32, at[1].floor() as i32 + 1, at[2].floor() as i32];
        let mut worn = Vec::new();
        if let Some(w) = self.ecs.world_mut() {
            for slot in (0..8).filter_map(ecs_stand::slot_of_index) {
                if let Some(name) = item_name(ecs_equipment::take_slot(w, e, slot)) {
                    worn.push(ItemStack::new(name, 1));
                }
            }
        }
        for stack in worn {
            self.pop_resource(pos, stack);
        }
        self.audio.play("minecraft:entity.armor_stand.break", Some(at), 1.0, 1.0);
        let source =self.level_entity(causing as i32).or(Some(super::commands::Target::Stand(id)));
        self.entity_killed_event_by(at, source);
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        self.ecs.despawn(e);
    }
    /// `EnchantmentHelper.isImmuneToDamage` for the stand `id`: whether an
    /// enchantment on an active equipment slot has a `minecraft:damage_immunity`
    /// effect whose requirements hold for `damage_type` and `causing`.
    fn stand_immune_to_damage(&mut self, id: u32, damage_type: &str, causing: Option<super::commands::Target>) -> bool {
        use super::commands::Target;
        let mut checks = Vec::new();
        for (slot, name) in ArmorStand::SLOTS.iter().enumerate().take(6) {
            let Some(stack) = self.ecs.equipment_stack(id, slot) else { continue };
            let armor = matches!(*name, "head" | "chest" | "legs" | "feet");
            let hand = matches!(*name, "mainhand" | "offhand");
            for (ench, level) in stack.enchantments() {
                let Some(def) = self.res.stations().enchantments.get(ench) else { continue };
                if !def.slots.iter().any(|s| s == name || s == "any" || (s == "armor" && armor) || (s == "hand" && hand)) {
                    continue;
                }
                for entry in def.effects["minecraft:damage_immunity"].as_array().into_iter().flatten() {
                    checks.push((entry.get("requirements").cloned(), *level));
                }
            }
        }
        if checks.is_empty() {
            return false;
        }
        let attacker = match causing {
            Some(Target::Player | Target::Guest(_)) => Some("minecraft:player".to_owned()),
            Some(Target::Mob(m_id)) if self.ecs.is_active() => self.fixed_kind(m_id),
            Some(Target::Stand(_)) => Some("minecraft:armor_stand".to_owned()),
            _ => None,
        };
        let canonical = if damage_type.contains(':') { damage_type.to_owned() } else { format!("minecraft:{damage_type}") };
        let ctx = myriade_engine::enchant::EffectContext {
            items: Some(self.res.items()),
            damage_type: Some(&canonical),
            is_direct: attacker.is_some(),
            this: Some("minecraft:armor_stand"),
            attacker: attacker.as_deref(),
            direct_attacker: attacker.as_deref(),
            ..Default::default()
        };
        let ench = &self.res.stations().enchantments;
        let rng = &mut self.world.random;
        checks.into_iter().any(|(req, level)| req.is_none_or(|r| ench.condition(&r, level, &ctx, rng)))
    }
    /// `command_damage_stand` on the ECS level: the gates are the host's; the stand's flags, life and fire are P's.
    pub(in crate::game) fn command_damage_stand(&mut self, id: u32, damage_type: &str, causing: Option<super::commands::Target>) -> bool {
        use super::commands::Target;
        let Some(e) = self.ecs.entity_of(id) else { return false };
        let registry = &self.health_resources.damage;
        let Some(kind) = registry.id(damage_type) else { return false };
        if !self.session.rules.bool_at(myriade_engine::gamerules::vanilla::MOB_GRIEFING) && matches!(causing, Some(Target::Mob(_))) {
            return false;
        }
        if registry.in_tag(kind, "minecraft:bypasses_invulnerability") {
            self.kill_target(Target::Stand(id));
            return false;
        }
        let creative = match causing {
            Some(Target::Player) => self.creative,
            Some(Target::Guest(conn)) => self.command_guest(conn).is_some_and(|g| g.creative()),
            _ => false,
        };
        let invulnerable = self
            .ecs
            .entity_nbt(e)
            .is_some_and(|c| myriade_engine::entity_nbt::flag(&horizon_core::nbt::Tag::Compound(c), "Invulnerable"));
        let hidden = self
            .ecs
            .world()
            .and_then(|w| w.get::<EcsStand>(e))
            .is_some_and(|s| s.flags & (ecs_stand::INVISIBLE | ecs_stand::MARKER) != 0);
        let immune = self.stand_immune_to_damage(id, damage_type, causing);
        if (invulnerable && !creative) || hidden || immune {
            return false;
        }
        let by = match causing {
            Some(Target::Player | Target::Guest(_) | Target::Mob(_)) => causing,
            _ => None,
        };
        let registry = &self.health_resources.damage;
        let tagged = |tag: &str| registry.in_tag(kind, tag);
        let (explosion, ignites, burns) = (
            tagged("minecraft:is_explosion"),
            tagged("minecraft:ignites_armor_stands"),
            tagged("minecraft:burns_armor_stands"),
        );
        let causing_id = causing.and_then(|target| self.network_entity_id(target)).unwrap_or(0) as u32;
        let (breaks, always_kills) = (
            tagged("minecraft:can_break_armor_stand"),
            tagged("minecraft:always_kills_armor_stands"),
        );
        if explosion {
            self.break_armor_stand_by_anything(e, id, causing_id);
            return false;
        }
        // P's fire intake: an ignition for 100 ticks, or the burn's hurt on a burning stand; the host reads no more of it.
        if ignites {
            if let Some(w) = self.ecs.world_mut() {
                ecs_stand::fire_intake(w, e, Cause::InFire);
            }
            return false;
        }
        if burns && self.ecs.world().and_then(|w| w.get::<Health>(e)).is_some_and(|h| h.0 > 0.5) {
            if let Some(w) = self.ecs.world_mut() {
                ecs_stand::fire_intake(w, e, Cause::OnFire);
            }
            return false;
        }
        if !breaks && !always_kills {
            return false;
        }
        let may_build = match causing {
            Some(Target::Player) => self.may_build(),
            Some(Target::Guest(conn)) => self.command_guest(conn).is_none_or(|g| g.abilities.may_build),
            _ => true,
        };
        if !may_build {
            return false;
        }
        self.break_armor_stand_by_hit(e, creative, always_kills, by)
    }
}
fn overlaps(a: Aabb, b: Aabb) -> bool {
    let (a0, a1, b0, b1) = (
        a.min_position(),
        a.max_position(),
        b.min_position(),
        b.max_position(),
    );
    a0.x < b1.x && b0.x < a1.x && a0.y < b1.y && b0.y < a1.y && a0.z < b1.z && b0.z < a1.z
}
/// What `Equippable.swapWithEquipmentSlot` leaves once it has passed its
/// checks: the piece worn in the slot, the hand, and the old piece bound for
/// the inventory.
#[derive(Debug, PartialEq)]
struct ArmorSwap {
    worn: ItemStack,
    hand: Option<ItemStack>,
    to_inventory: Option<ItemStack>,
}
/// `Equippable.swapWithEquipmentSlot` after its checks. A single item swaps
/// with the worn piece: the hand gets the worn piece (`copyAndClear`), or
/// keeps the item in creative when nothing was worn; creative wears a copy
/// (`copy` instead of `copyAndClear`). From a larger stack one item is worn
/// (`consumeAndReturn`, which takes none in creative) and the worn piece goes
/// to the inventory.
fn swap_with_equipment_slot(
    stack: &ItemStack,
    current: Option<ItemStack>,
    creative: bool,
) -> ArmorSwap {
    let one = ItemStack {
        count: 1,
        ..stack.clone()
    };
    if stack.count <= 1 {
        let hand = match current {
            Some(old) => Some(old),
            None if creative => Some(stack.clone()),
            None => None,
        };
        return ArmorSwap {
            worn: one,
            hand,
            to_inventory: None,
        };
    }
    let mut held = stack.clone();
    if !creative {
        held.count -= 1;
    }
    ArmorSwap {
        worn: one,
        hand: Some(held),
        to_inventory: current,
    }
}
/// `Equippable.swapWithEquipmentSlot`'s armour slot for `stack`: the stack's
/// own `equippable` component (a saved one, or the server's patch), else its
/// item's, and only while it is swappable.
pub(super) fn hand_equip_slot(
    stack: &ItemStack,
    items: &crate::items::Items,
) -> Option<(usize, &'static str)> {
    let equippable = myriade_engine::item_sync::equippable(stack, items)?;
    let actual = stack
        .data()
        .and_then(|data| data.other("minecraft:equippable"));
    // Only adopt the default-allowed shape here. Restricted or otherwise
    // shaped overrides keep their existing admission until supported.
    let supported = actual.is_none_or(|tag| {
        matches!(tag, horizon_core::nbt::Tag::Compound(map)
            if map.keys().all(|key| matches!(key.to_string_lossy().as_str(), "slot" | "swappable")))
            && matches!(tag.get("slot"), Some(horizon_core::nbt::Tag::String(_)))
            && tag.get("swappable").is_none_or(|flag| {
                myriade_engine::item_data::component_bool(flag).is_some()
            })
    });
    let (slot, swappable) = if supported {
        (equippable.slot.as_str(), equippable.swappable)
    } else {
        let info = items.get(&stack.item)?;
        (info.equip_slot.as_deref()?, info.swappable)
    };
    if !swappable {
        return None;
    }
    Some(match slot {
        "head" => (0, "head"),
        "chest" => (1, "chest"),
        "legs" => (2, "legs"),
        "feet" => (3, "feet"),
        _ => return None,
    })
}

impl Game<'_> {
    /// `Equippable.swapWithEquipmentSlot`: right-clicking armor swaps it into
    /// its slot (the worn piece comes back to the hand). It runs on a client
    /// too (`Item.use`), as the prediction the server's slot packets confirm.
    pub(super) fn equip_from_hand(&mut self, stack: &ItemStack) -> bool {
        let Some((index, slot)) = hand_equip_slot(stack, self.items()) else {
            return false;
        };
        let current = self.armor[index].clone();
        // PREVENT_ARMOR_CHANGE (Curse of Binding) outside creative, or the same item.
        if current.as_ref().is_some_and(|c| {
            (world_stations!(self).enchantments.prevents_armor_change(c) && !self.creative)
                || c.same_item_same_components(stack)
        }) {
            return true;
        }
        // Counted on the server only, before the swap.
        {
            self.award_stat("used", &stack.item, 1);
        }
        let swap = swap_with_equipment_slot(stack, current, self.creative);
        self.armor[index] = Some(swap.worn.clone());
        // Player.setItemSlot: onEquipItem plays the equip sound.
        self.player_equipped(slot, Some(&swap.worn));
        *self.held_mut() = swap.hand;
        // Inventory.add, else Player.drop (in creative a full inventory
        // takes it anyway: hasInfiniteMaterials empties the stack).
        if let Some(old) = swap.to_inventory
            && let Some(rest) = self.add_stack_to_inventory(old)
            && !self.creative
            // Player.drop is the server's on a client: it spawns the item.

            && let Some(eye) = self.eye_position()
        {
            // Player.drop(stack, false): thrown from the eyes, no thrower.
            self.throw_from_eyes(rest, eye, false, false);
        }
        // The attribute modifiers of a worn piece come from the server's
        // ClientboundUpdateAttributesPacket on a client.
        {
            self.apply_equipment();
        }
        self.swing_arm();
        true
    }
}

/// The `ArmorStand.equipment` index `ArmorStand.interact` puts `stack` in:
/// the stack's own `equippable` slot, else the main hand.
fn stand_slot(items: &crate::items::Items, stack: &ItemStack) -> usize {
    match myriade_engine::item_sync::equippable_slot(stack, items).as_deref() {
        Some("feet") => 2,
        Some("legs") => 3,
        Some("chest") => 4,
        Some("head") => 5,
        Some("offhand") => 1,
        _ => 0,
    }
}
/// `ArmorStand.getBbWidth` and `getBbHeight`: a marker has no box, a small stand is half size.
fn stand_box(a: &ArmorStand) -> (f32, f32) {
    match (a.marker, a.small) {
        (true, _) => (0.0, 0.0),
        (false, true) => (0.25, 0.9875),
        (false, false) => (0.5, 1.975),
    }
}
/// `ArmorStand.ignoreExplosion` (an invisible stand), and `hurtServer`
/// refusing an invisible or marker stand: such a stand outlasts any blast.
pub(super) fn spared_by_blast(a: &ArmorStand) -> bool {
    a.invisible || a.marker
}

/// `ArmorStand.brokenByAnything`: the equipment, less what curse of vanishing
/// keeps (`PREVENT_EQUIPMENT_DROP`), popped at `blockPosition().above()`.
fn broken_armor_stand_equipment(a: ArmorStand) -> ([i32; 3], Vec<ItemStack>) {
    let pos = [
        a.position.x.floor() as i32,
        a.position.y.floor() as i32 + 1,
        a.position.z.floor() as i32,
    ];
    let drops = a
        .equipment
        .into_iter()
        .flatten()
        .filter(|stack| stack.enchantment_level("minecraft:vanishing_curse") == 0)
        .collect();
    (pos, drops)
}
#[cfg(test)]
mod pumpkin_equippable_use_tests {
    use super::*;
    use myriade_engine::lan::{Joined, NetConfig, PlayerSender, guest::Guest, hub::HubConfig};
    use horizon_core::position::ChunkPos;
    use horizon_protocol::packets::{
        configuration::ClientInformation,
        play::{Serverbound, UseItem},
    };
    use std::sync::{Arc, atomic::AtomicI32, mpsc};

    #[test]
    #[ignore = "daily"]
    fn h207_connected_use_item_adopts_actual_pumpkin_equippable_and_preserves_controls() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.build_registries();
            game.set_allow_commands(true);
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(myriade_engine::chunk::Chunk::empty(0, 0)),
            );
            game.block_ticks
                .add_empty_chunk(ChunkPos::new(0, 0))
                .unwrap();
            assert!(game.run_command("fill 0 199 0 15 199 15 stone"));
            assert!(game.world.loaded(8, 8));
            game.lan_host(
                crate::game::lan::LanSettings {
                    port: 0,
                    guest_commands: true,
                    force_game_mode: false,
                },
                NetConfig {
                    announce: false,
                    ..NetConfig::loopback(0, "pumpkin-use-test".into(), 2)
                },
                HubConfig {
                    default_game_mode: 0,
                    guest_commands: true,
                    force_game_mode: false,
                    hardcore: false,
                    view_distance: 2,
                    simulation_distance: 2,
                    dimension: "minecraft:overworld".into(),
                    allow_flight: false,
                    seed: 0,
                    max_players: 2,
                    levels: Default::default(),
                    level_seeds: Default::default(),
                },
            )
            .unwrap();
            let (tx, _rx) = mpsc::channel();
            let mut guest = Guest::new(
                Joined {
                    conn: 2,
                    uuid: 207,
                    name: "PumpkinGuest".into(),
                    properties: Vec::new(),
                    info: ClientInformation::new(2),
                    brand: "test".into(),
                    sender: PlayerSender::new(tx, Arc::default()),
                    latency: Arc::new(AtomicI32::new(0)),
                    address: "127.0.0.1:25565".parse().unwrap(),
                    expected: false,
                },
                2,
                [8.5, 200.0, 8.5],
                2,
            );
            guest.dimension = "minecraft:overworld".into();
            guest.placed = true;
            guest.loaded = true;
            guest.selected = 0;
            guest.permission = 2;
            game.lan.as_mut().unwrap().hub.guests.push(guest);
            let clock = game.world.game_time;
            let eq = "minecraft:equippable={slot:\"head\"}";
            let marker = "minecraft:custom_data={h207:207}";
            let helmet = "minecraft:diamond_helmet[minecraft:custom_data={old:1}]";
            let mut failures = Vec::new();
            for (i, (label, mode, component, count, old, changed)) in [
            ("empty", "survival", Some(eq), 3, None, true),
            ("swap", "survival", Some(eq), 1, Some(helmet), true),
            ("creative", "creative", Some(eq), 3, None, true),
            ("prototype", "survival", None, 3, None, false),
            ("false", "survival", Some("minecraft:equippable={slot:\"head\",swappable:0b}"), 3, None, false),
            ("binding", "survival", Some(eq), 3, Some("minecraft:diamond_helmet[minecraft:enchantments={\"minecraft:binding_curse\":1},minecraft:custom_data={old:1}]"), false),
            ("same", "survival", Some(eq), 3, Some("minecraft:carved_pumpkin[minecraft:equippable={slot:\"head\"},minecraft:custom_data={h207:207}]"), false),
        ].into_iter().enumerate() {
            let held = format!("minecraft:carved_pumpkin[{}{}{}]", component.unwrap_or(""), if component.is_some() { "," } else { "" }, marker);
            let before = game.as_guest(2, |g| {
                assert_eq!(g.session.registries.permission, 2);
                assert!(g.run_command("clear @s") || g.inventory.iter().all(Option::is_none));
                g.run_command("item replace entity @s armor.head with air");
                g.run_command(&format!("gamemode {mode}"));
                assert!(g.run_command(&format!("item replace entity @s weapon.mainhand with {held} {count}")));
                if let Some(old) = old { assert!(g.run_command(&format!("item replace entity @s armor.head with {old}"))); }
                assert_eq!(g.player.as_ref().unwrap().position, Vec3::new(8.5, 200.0, 8.5));
                let hand = g.held().unwrap().clone();
                assert_eq!(hand.item.as_ref(), "minecraft:carved_pumpkin");
                assert_eq!(hand.count, count);
                assert!(hand.data().unwrap().other("minecraft:custom_data").is_some());
                let actual = hand.data().unwrap().other("minecraft:equippable");
                assert_eq!(actual.is_some(), component.is_some());
                if let Some(actual) = actual {
                    assert_eq!(actual.get("slot"), Some(&horizon_core::nbt::Tag::String("head".into())));
                    assert_eq!(actual.get("swappable").and_then(myriade_engine::item_data::component_bool), if label == "false" { Some(false) } else { None });
                }
                if label == "binding" { assert_eq!(g.armor[0].as_ref().unwrap().enchantment_level("minecraft:binding_curse"), 1); }
                if label == "same" { assert!(g.armor[0].as_ref().unwrap().same_item_same_components(&hand)); }
                (g.inventory.clone(), g.armor.clone(), hand)
            }).unwrap();
            let sequence = 207 + i as i32;
            game.lan_guest_packet(2, Serverbound::UseItem(UseItem { hand: 0, yaw: 0.0, pitch: 0.0, sequence }));
            assert!(game.lan.as_ref().unwrap().hub.guest(2).unwrap().ack_up_to.get() >= sequence);
            game.as_guest(2, |g| {
                let mut expected_inventory = before.0.clone();
                let mut expected_armor = before.1.clone();
                if changed {
                    let mut worn = before.2.clone();
                    worn.count = 1;
                    expected_armor[0] = Some(worn);
                    if label == "swap" { expected_inventory[0] = before.1[0].clone(); }
                    else if label == "empty" { expected_inventory[0].as_mut().unwrap().count = 2; }
                }
                if g.inventory != expected_inventory || g.armor != expected_armor {
                    failures.push((label, g.inventory.clone(), g.armor.clone()));
                }
                assert_eq!(g.world.game_time, clock);
            }).unwrap();
        }
            assert!(
                failures.is_empty(),
                "actual connected UseItem Equippable adoption failures: {failures:?}"
            );
        });
    }
}

