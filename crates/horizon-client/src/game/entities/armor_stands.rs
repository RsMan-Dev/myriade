//! Armor stands: `ArmorStandItem.useOn` placement (snapped to 45 degrees),
//! falling, `interactAt` equipment swaps by clicked height, the two-hit break
//! of `hurtServer` (creative breaks at once), and `ArmorStandRenderer` with
//! the default pose, the hit wobble and humanoid armor layers.
use super::entity::Entity;
use super::{Game, InteractTarget};
use horizon_core::random::RandomSource;
use horizon_client::{
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
pub(super) const TYPE_ID: &str = "minecraft:armor_stand";
/// Build the living-drops input at the loot-source boundary, before equipment pops.
fn armor_stand_dying_drops<'a>(
    stand: &ArmorStand,
    mob_drops: bool,
    dying: super::creatures::DyingDrops<'a>,
    roll: impl FnOnce(&str) -> Vec<horizon_world::loot::LootStack>,
) -> (super::creatures::DyingDrops<'a>, Vec<(ItemStack, [f64; 3])>) {
    let stacks = if stand.small || !mob_drops {
        Vec::new()
    } else {
        roll("minecraft:entities/armor_stand")
            .iter()
            .filter(|stack| !stack.is_empty())
            .map(|stack| {
                (
                    super::creatures::from_loot(stack),
                    [stand.position.x, stand.position.y, stand.position.z],
                )
            })
            .collect()
    };
    (dying, stacks)
}

#[cfg(test)]
mod death_loot_tests {
    use super::*;

    #[test]
    fn loot_source_stacks_reach_dying_drops_and_obey_gates() {
        for (small, mob_drops) in [(false, true), (true, true), (false, false), (true, false)] {
            let mut stand = ArmorStand::new(42, Vec3::new(1.5, 64.0, -2.5), 0.0);
            stand.small = small;
            let dying = super::super::creatures::DyingDrops {
                entity: stand.id,
                kind: "minecraft:armor_stand",
                player: None,
                source: None,
                recently_hit: false,
            };
            let mut called = false;
            let (dying, stacks) = armor_stand_dying_drops(&stand, mob_drops, dying, |table| {
                assert_eq!(table, "minecraft:entities/armor_stand");
                called = true;
                // A fake table source; no prepared vanilla resources or Game required.
                let stack = horizon_world::loot::LootStack {
                    item: "minecraft:stick".into(),
                    count: 3,
                    damage: 0,
                    enchantments: Vec::new(),
                    potion: None,
                    map: false,
                    exploration: None,
                    ominous_amplifier: None,
                    stew_effects: Vec::new(),
                    names: Vec::new(),
                    components: Vec::new(),
                };
                let mut empty = stack.clone();
                empty.count = 0;
                vec![stack, empty]
            });
            assert_eq!(dying.entity, 42);
            assert_eq!(dying.kind, "minecraft:armor_stand");
            assert_eq!(called, !small && mob_drops);
            if !small && mob_drops {
                assert_eq!(stacks.len(), 1);
                assert_eq!(stacks[0].0.item.as_ref(), "minecraft:stick");
                assert_eq!(stacks[0].0.count, 3);
                assert_eq!(stacks[0].1, [1.5, 64.0, -2.5]);
            } else {
                assert!(stacks.is_empty());
            }
        }
    }
}

/// Velocity `AbstractMinecart.push` adds: the cart, and a quarter on the stand.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CartShove {
    cart_x: f64,
    cart_z: f64,
    stand_x: f64,
    stand_z: f64,
}
/// `ArmorStand.pushEntities` for one cart. `None` when the cart is not the
/// plain rideable kind, the boxes miss, `distanceToSqr` is past 0.2, the
/// stand is a passenger of the cart, or the horizontal gap is under
/// `1.0E-4F` (where `AbstractMinecart.push` does nothing).
fn armor_stand_cart_shove(
    stand_pos: Vec3,
    stand_box: Aabb,
    cart_pos: Vec3,
    cart_box: Aabb,
    kind: super::minecarts::CartKind,
    cart_carries_stand: bool,
) -> Option<CartShove> {
    if kind != super::minecarts::CartKind::Rideable
        || cart_carries_stand
        || !stand_box.intersects(cart_box)
    {
        return None;
    }
    let (dx, dy, dz) = (
        stand_pos.x - cart_pos.x,
        stand_pos.y - cart_pos.y,
        stand_pos.z - cart_pos.z,
    );
    if dx * dx + dy * dy + dz * dz > 0.2 {
        return None;
    }
    let (xa, za) = super::minecarts::cart_push_from(stand_pos, cart_pos)?;
    Some(CartShove {
        cart_x: -xa,
        cart_z: -za,
        stand_x: xa / 4.0,
        stand_z: za / 4.0,
    })
}

impl Game<'_> {
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
            stand.custom_name = Some(horizon_client::text::Text::literal(name));
        }
        let b = stand.bounds();
        // Touching the floor is not a collision.
        let (lo, hi) = (b.min_position(), b.max_position());
        let e = 1.0e-7;
        let bounds = Aabb::new(
            Vec3::new(lo.x + e, lo.y + e, lo.z + e),
            Vec3::new(hi.x - e, hi.y - e, hi.z - e),
        );
        let mut scene = horizon_client::collisions::SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        if !scene.unobstructed(bounds).unwrap_or(false)
            || self
                .armor_stands
                .iter()
                .any(|o| overlaps(o.bounds(), bounds))
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
        let own = horizon_client::entity_nbt::armor_stand_to_nbt(&stand, [0; 4], None);
        let mut raw = horizon_client::block_entity_item::stack_configured_entity(
            stack,
            "minecraft:armor_stand",
            &own,
        );
        if let Some(mut loaded) = horizon_client::entity_nbt::armor_stand_from_nbt(&raw, stand.id) {
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
        self.persist.raw.insert(stand.id, raw);
        let foot = stand.position;
        // EntityType.create's spawn yaw, `level.getRandom().nextFloat() * 360`, before
        // ArmorStandItem.useOn turns the stand to face the player.
        self.world.random.next_float();
        self.next_entity += 1;
        let (_, uuid) = horizon_client::mobs::living_random();
        self.persist.uuids.insert(stand.id, uuid);
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
        self.armor_stands.push(stand);
        if !self.creative {
            self.consume_held();
        }
        // ItemStack.useOn counts the use.
        self.award_stat("used", &stack.item, 1);
        // ArmorStandItem.useOn returns SUCCESS: the hand swings.
        self.swing_arm();
        true
    }
    /// The armor stand under the crosshair, its distance and the hit point.
    pub(super) fn armor_stand_hit(&self) -> Option<(usize, f64, [f64; 3])> {
        let (eye, dir) = self.aim()?;
        let reach: f64 = self.entity_reach;
        let mut best: Option<(usize, f64, [f64; 3])> = None;
        for (i, a) in self.armor_stands.iter().enumerate() {
            // Minecraft.pick never hits the camera entity it looks from.
            if a.marker {
                continue;
            }
            let b = a.bounds();
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
                best = Some((i, t0, std::array::from_fn(|k| eye[k] + dir[k] * t0)));
            }
        }
        best
    }
    fn equipment_slot(&self, stack: &ItemStack) -> usize {
        stand_slot(self.res.items(), stack)
    }
    /// The ECS stand under the crosshair: the nearest the ray meets within reach, by the box P keeps (`Bounds`), among the stands
    /// the spatial index holds near the eye. Markers and the camera are no target (`armor_stand_hit` on the level).
    fn ecs_stand_hit(&self) -> Option<(u32, f64, [f64; 3])> {
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
    fn interact_armor_stand_ecs(&mut self, target: Option<InteractTarget>) -> bool {
        let (id, point) = match target {
            Some(t) => {
                let Ok(tid) = u32::try_from(t.id) else { return false };
                let Some(p) = self.ecs.position(tid) else { return false };
                (tid, [p[0] + t.location[0], p[1] + t.location[1], p[2] + t.location[2]])
            }
            None => {
                let Some((id, d, point)) = self.ecs_stand_hit() else { return false };
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
    /// `attack_armor_stand` on the ECS level: the pick is `ecs_stand_hit`, the hit is `hit_armor_stand_ecs`.
    fn attack_armor_stand_ecs(&mut self) -> bool {
        let Some((id, d, _)) = self.ecs_stand_hit() else { return false };
        if !self.before_block(d) || self.mob_hit().is_some_and(|m| m.1 < d) {
            return false;
        }
        let Some(at) = self.ecs.position(id) else { return false };
        if !self.may_interact_at(BlockPos::new(at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32)) {
            return true;
        }
        let ((_, scale, crit, magic), admitted) = self.melee_damage_with_admission("minecraft:armor_stand", true, Some(id));
        let sweep = self.sweep_before_hit("minecraft:armor_stand", crit);
        let hurt = self.hit_armor_stand_ecs(id, |g, area, id| {
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
    fn hit_armor_stand_ecs(&mut self, id: u32, before_weapon: impl FnOnce(&mut Self, Aabb, u32)) -> bool {
        let Some(e) = self.ecs.entity_of(id) else { return false };
        let Some(w) = self.ecs.world() else { return false };
        let (Some(area), Some(center), Some(on_ground)) = (
            w.get::<Bounds>(e).map(|b| b.0),
            w.get::<Pos>(e).map(|p| p.0),
            w.get::<Contact>(e).map(|c| c.on_ground),
        ) else {
            return false;
        };
        let hurt = self.punch_armor_stand_ecs(id);
        if hurt {
            before_weapon(self, area, id);
            self.weapon_hit_stand(id, center, on_ground);
        }
        hurt
    }
    /// `punch_armor_stand` on the ECS level: the invulnerability check as the host does, then P's `stand::blow` decides the
    /// wobble or the break, and the host plays its sounds and posts its game events from that answer.
    fn punch_armor_stand_ecs(&mut self, id: u32) -> bool {
        let Some(e) = self.ecs.entity_of(id) else { return false };
        let attacker = self.own_entity_id();
        let vanilla = self.saved_refuses_damage(id, "minecraft:player_attack", self.creative);
        let source = horizon_client::health::HurtSource {
            damage_type: "minecraft:player_attack",
            causing: attacker,
            direct: attacker,
            position: None,
        };
        if super::entity::damage_level_of!(self, scoped).invulnerability_check(
            horizon_client::health::Hurting::ArmorStand(id as i32, source),
            vanilla,
        ) {
            return false;
        }
        let (creative, by) = (self.creative, Some(super::commands::Target::Player));
        self.break_armor_stand_by_hit_ecs(e, creative, false, by)
    }
    /// `ArmorStand.hurtServer` for a blow on an ECS stand: P's `stand::blow` says wobble or break; the host plays the sounds and
    /// posts the game events from that answer. True: the blow was taken.
    fn break_armor_stand_by_hit_ecs(&mut self, e: EcsEntity, creative: bool, always_kills: bool, by: Option<super::commands::Target>) -> bool {
        let now = self.world.game_time.max(0) as u64;
        let Some(at) = self.ecs.world().and_then(|w| w.get::<Pos>(e)).map(|p| [p.0.x, p.0.y, p.0.z]) else { return false };
        let Some(w) = self.ecs.world_mut() else { return false };
        match ecs_stand::blow(w, e, now, creative, always_kills) {
            Blow::Wobble => {
                self.audio.play("minecraft:entity.armor_stand.hit", Some(at), 0.3, 1.0);
                self.stand_game_event("minecraft:entity_damage", at, by);
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
            self.show_breaking_particles(at, f64::from(width), f64::from(height));
            self.stand_game_event("minecraft:entity_die", at, by);
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
        self.show_breaking_particles(at, f64::from(width), f64::from(height));
        self.stand_game_event("minecraft:entity_die", at, by);
        self.ecs.despawn(e);
    }
    /// `ArmorStand.interact` for the stand under the crosshair, or for the
    /// stand a guest's interact packet named (its client's click point).
    pub(super) fn interact_armor_stand(&mut self, target: Option<InteractTarget>) -> bool {
        if self.ecs.is_active() {
            return self.interact_armor_stand_ecs(target);
        }
        let Some((mut i, d, point)) = (match target {
            // interactAt gets the packet's location, measured from the stand's
            // position, so getClickedSlot reads the height the client clicked.
            Some(t) => Self::named_entity(&self.armor_stands, t.id, |a| a.id).map(|i| {
                let p = self.armor_stands[i].position;
                (
                    i,
                    0.0,
                    [
                        p.x + t.location[0],
                        p.y + t.location[1],
                        p.z + t.location[2],
                    ],
                )
            }),
            None => self.armor_stand_hit(),
        }) else {
            return false;
        };
        // Only a client's own pick is stopped by a block: the range check passed.
        if target.is_none() && !self.before_block(d) {
            return false;
        }
        let held = self.held().cloned();
        if let Some(stack) = held.as_ref().filter(|h| &*h.item == "minecraft:name_tag") {
            let stand = &mut self.armor_stands[i];
            let alive = stand.health > 0.0;
            if !stand.name_with_tag(stack) {
                return false;
            }
            let id = stand.id;
            let at = [stand.position.x, stand.position.y, stand.position.z];
            let name = stand.custom_name.as_ref().map(|name| name.plain());
            if alive {
                // `Player.interactOn` gives a creative player the count back.
                self.consume_held();
                if let (Some(player), Some(player_name), Some(name)) =
                    (self.lan_acting_uuid, self.lan_acting_name.clone(), name)
                {
                    self.native_player_name_entity(player, &player_name, id, &name);
                }
            }
            // `Player.interactOn`: the tag consumed the use, so the stand
            // posts `gameEvent(GameEvent.ENTITY_INTERACT)` from the player,
            // alive or not (`NameTagItem` succeeds either way).
            let ctx = self.who_context(Some(Who::Player), None);
            self.post_game_event("minecraft:entity_interact", at, ctx);
            self.swing_arm();
            return true;
        }
        let a = &self.armor_stands[i];
        let slot = match &held {
            Some(h) => {
                let slot = self.equipment_slot(h);
                // ArmorStand.interact: a disabled slot, or a hand without
                // arms, is FAIL.
                if a.is_disabled(slot) {
                    return true;
                }
                slot
            }
            None => {
                // getClickedSlot by the height of the click.
                let y = (point[1] - a.position.y) / if a.small { 0.5 } else { 1.0 };
                let has = |s: usize| a.equipment[s].is_some();
                let small = a.small;
                if y >= 0.1 && y < 0.1 + if small { 0.8 } else { 0.45 } && has(2) {
                    2
                } else if y >= 0.9 + if small { 0.3 } else { 0.0 }
                    && y < 0.9 + if small { 1.0 } else { 0.7 }
                    && has(4)
                {
                    4
                } else if y >= 0.4 && y < 0.4 + if small { 1.0 } else { 0.8 } && has(3) {
                    3
                } else if y >= 1.6 && has(5) {
                    5
                } else if !has(0) && has(1) {
                    1
                } else {
                    0
                }
            }
        };
        // An empty hand takes from the main hand when the clicked slot is
        // disabled (`isDisabled(clickedSlot) ? slot : clickedSlot`).
        let slot = if held.is_none() && a.is_disabled(slot) {
            0
        } else {
            slot
        };
        // ArmorStand.swapItem: taking from a filled slot or putting into an
        // empty one can be disabled; the click then passes.
        if a.swap_locked(slot) {
            return false;
        }
        let current = self.armor_stands[i].equipment[slot].clone();
        if held.is_none() && current.is_none() {
            return false;
        }
        // A server's mods hear the swap before it happens, and may stop it.
        // A stack of several on an occupied slot goes nowhere (below).
        let moves = held
            .as_ref()
            .is_none_or(|h| h.count <= 1 || current.is_none());
        if moves
            && target.is_some()
            && let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone())
        {
            let stand = self.armor_stands[i].id as i32;
            let slot_name = ["mainhand", "offhand", "feet", "legs", "chest", "head"][slot];
            let mut cancel = false;
            self.native_armor_stand_manipulate(player, &name, stand, slot_name, &mut cancel);
            if cancel {
                return true;
            }
            // The hook had `&mut self`: find the stand again by id and check
            // the slot and the held stack are still what the swap was worked
            // out from.
            let Some(now) = self.armor_stands.iter().position(|a| a.id as i32 == stand) else {
                return true;
            };
            i = now;
            let key = |s: &Option<ItemStack>| s.as_ref().map(|s| (s.item.clone(), s.count));
            if key(&self.armor_stands[i].equipment[slot]) != key(&current)
                || key(&self.held().cloned()) != key(&held)
                || self.armor_stands[i].swap_locked(slot)
            {
                return true;
            }
        }
        // ArmorStand.swapItem.
        match held {
            // A player with infinite materials puts a copy of one on an empty
            // slot and keeps the stack.
            Some(h) if self.creative && current.is_none() => {
                self.armor_stands[i].equipment[slot] = Some(ItemStack { count: 1, ..h });
            }
            Some(h) if h.count > 1 => {
                if current.is_some() {
                    return true;
                }
                self.armor_stands[i].equipment[slot] = Some(ItemStack {
                    count: 1,
                    ..h.clone()
                });
                *self.held_mut() = Some(ItemStack {
                    count: h.count - 1,
                    ..h
                });
            }
            h => {
                self.armor_stands[i].equipment[slot] = h;
                *self.held_mut() = current;
            }
        }
        let p = self.armor_stands[i].position;
        self.world
            .modified
            .insert(((p.x.floor() as i32) >> 4, (p.z.floor() as i32) >> 4));
        self.swing_arm();
        true
    }
    /// `ArmorStand.hurtServer` from a player attack.
    pub(super) fn attack_armor_stand(&mut self) -> bool {
        if self.ecs.is_active() {
            return self.attack_armor_stand_ecs();
        }
        let Some((i, d, _)) = self.armor_stand_hit() else {
            return false;
        };
        if !self.before_block(d) || self.mob_hit().is_some_and(|m| m.1 < d) {
            return false;
        }
        // ArmorStand.skipAttackInteraction: `mayInteract` at its block refuses
        // the hit before any damage is worked out (so before its RNG draws).
        let at = self.armor_stands[i].position;
        if !self.may_interact_at(BlockPos::new(
            at.x.floor() as i32,
            at.y.floor() as i32,
            at.z.floor() as i32,
        )) {
            return true;
        }
        let ((_, scale, crit, magic), admitted) = self.melee_damage_with_admission(
            "minecraft:armor_stand",
            true,
            Some(self.armor_stands[i].id),
        );
        let sweep = self.sweep_before_hit("minecraft:armor_stand", crit);
        let hurt = self.hit_armor_stand(i, |g, area, id| {
            let knockback = if admitted {
                g.extra_knockback("minecraft:armor_stand", scale).0
            } else {
                0.0
            };
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
        // Player.onAttack: the hit starts the attack strength again, hurt or
        // not (skipAttackInteraction only asks mayInteract).
        self.attack_ticks = 0;
        if hurt {
            if admitted {
                self.special_attack_exhaustion();
            }
            self.swing_arm();
        }
        hurt
    }
    /// A player's attack on the stand `i` once it is picked: the stand's own
    /// hurt, `before_weapon` (the attack's sounds and particles, given the
    /// stand's box and id) and `Player.itemAttackInteraction`, which treats
    /// the stand as a living target: the held weapon's hit and post-attack
    /// effects.
    pub(super) fn hit_armor_stand(
        &mut self,
        i: usize,
        before_weapon: impl FnOnce(&mut Self, Aabb, u32),
    ) -> bool {
        // The stand's box, id and place, before the hit may remove it.
        let (area, id) = (self.armor_stands[i].bounds(), self.armor_stands[i].id);
        let (center, on_ground) = (
            self.armor_stands[i].position,
            self.armor_stands[i].on_ground,
        );
        let hurt = self.punch_armor_stand(i);
        if hurt {
            before_weapon(self, area, id);
            self.weapon_hit_stand(id, center, on_ground);
        }
        hurt
    }
    /// `ArmorStand.hurtServer` for a player's attack (also a sweep's): a
    /// creative player breaks it, otherwise the first hit wobbles it and a
    /// second within five ticks breaks it.
    pub(super) fn punch_armor_stand(&mut self, i: usize) -> bool {
        // `Entity.isInvulnerableTo` before ArmorStand's own branches (arismoko/horizon#916):
        // the saved flag or window refuses the hit — past only for
        // #bypasses_invulnerability damage and a creative player's hit
        // (`DamageSource.isCreativePlayer`).
        let id = self.armor_stands[i].id;
        let attacker = self.own_entity_id();
        let vanilla = self.saved_refuses_damage(id, "minecraft:player_attack", self.creative);
        let source = horizon_client::health::HurtSource {
            damage_type: "minecraft:player_attack",
            causing: attacker,
            direct: attacker,
            position: None,
        };
        if super::entity::damage_level_of!(self, scoped).invulnerability_check(
            horizon_client::health::Hurting::ArmorStand(id as i32, source),
            vanilla,
        ) {
            return false;
        }

        self.break_armor_stand_by_hit(
            i,
            self.creative,
            false,
            Some(super::commands::Target::Player),
        )
    }
    /// The end of `ArmorStand.hurtServer` for a source in
    /// `#can_break_armor_stand` or `#always_kills_armor_stands`, which returns
    /// true: a creative causing player breaks the stand at once; otherwise the
    /// first hit (`always_kills` skips it) wobbles it and posts `entity_damage`,
    /// and a second hit within five ticks breaks it (`brokenByPlayer`). `by` is
    /// the causing entity the game events are attributed to.
    fn break_armor_stand_by_hit(
        &mut self,
        i: usize,
        creative: bool,
        always_kills: bool,
        by: Option<super::commands::Target>,
    ) -> bool {
        let time = self.world.game_time;
        let at = self.armor_stands[i].position;
        let at = [at.x, at.y, at.z];
        if creative {
            let (width, height) = stand_box(&self.armor_stands[i]);
            self.armor_stands.remove(i);
            self.audio.play("minecraft:entity.armor_stand.break", Some(at), 1.0, 1.0);
            self.show_breaking_particles(at, f64::from(width), f64::from(height));
            // ArmorStand.kill(level, the causing entity).
            self.stand_game_event("minecraft:entity_die", at, by);
            return true;
        }
        if time - self.armor_stands[i].last_hit > 5 && !always_kills {
            self.audio.play("minecraft:entity.armor_stand.hit", Some(at), 0.3, 1.0);
            // First hit: entity event 32, the wobble and the hit sound, then
            // gameEvent(ENTITY_DAMAGE, the causing entity).
            self.stand_game_event("minecraft:entity_damage", at, by);
            self.armor_stands[i].last_hit = time;
            return true;
        }
        // brokenByPlayer: the stand item, with the stand's name, pops at its
        // block, then brokenByAnything: the sound, the loot table and the
        // equipment above its block; then the kill.
        let a = self.armor_stands.remove(i);
        let (width, height) = stand_box(&a);
        let mut stand = ItemStack::new("minecraft:armor_stand", 1);
        if let Some(name) = &a.custom_name {
            stand.data_mut().custom_name =
                Some(horizon_client::scoreboard::component_tag(&name.component()));
        }
        let block = [
            a.position.x.floor() as i32,
            a.position.y.floor() as i32,
            a.position.z.floor() as i32,
        ];
        self.pop_resource(block, stand);
        self.audio.play("minecraft:entity.armor_stand.break", Some(at), 1.0, 1.0);
        self.armor_stand_death_drops(&a, "minecraft:player_attack", self.own_entity_id() as u32);
        let (above, drops) = broken_armor_stand_equipment(a);
        for stack in drops {
            self.pop_resource(above, stack);
        }
        self.show_breaking_particles(at, f64::from(width), f64::from(height));
        self.stand_game_event("minecraft:entity_die", at, by);
        true
    }
    /// `Entity.gameEvent(event, attributedTo)` for a stand at `at`: the
    /// causing player (a LAN guest too) or mob, else the stand itself (which
    /// no listener tells apart from none).
    fn show_breaking_particles(&mut self, at: [f64; 3], width: f64, height: f64) {
        let Some(state) = self
            .pack
            .assets
            .registry
            .default_state("minecraft:oak_planks")
        else {
            return;
        };
        let options = horizon_client::particles::ParticleOptions::simple(
            horizon_client::particles::ParticleType::Block,
        );
        // Entity.getY(2/3): the box's height is scaled by 2/3 from the feet.
        let centre = [at[0], at[1] + height * (2.0 / 3.0), at[2]];
        let spread = [width / 4.0, height / 4.0, width / 4.0];
        self.lan_level_particles(&options, centre, spread, 0.05, 10, Some(state));
    }
    fn stand_game_event(
        &mut self,
        event: &'static str,
        at: [f64; 3],
        by: Option<super::commands::Target>,
    ) {
        self.entity_event_by(event, at, by);
    }
    pub(super) fn tick_armor_stands(&mut self) -> Result<()> {
        let mut dead = Vec::new();
        // ServerLevel.tick: only entities in entity-ticking chunks tick.
        let ticking = self.entity_ticking_area();
        // EntityGetter.getEntityCollisions: the boxes that stop a stand's move
        // (a living shulker, a boat, a happy ghast held still), gathered when
        // the first stand travels. A server's stand rides nothing (`vehicle`
        // is set on a server's client only), but mobs can ride it (`/ride`):
        // `isPassengerOfSameVehicle` leaves those riders out.
        let mut nearby = Vec::new();
        for i in 0..self.armor_stands.len() {
            let at = self.armor_stands[i].position;
            if !ticking(at.x.floor() as i32, at.z.floor() as i32) {
                self.armor_stands[i].previous = at;
                self.armor_stands[i].previous_yaw = self.armor_stands[i].yaw;
                continue;
            }
            self.armor_stands[i].first_tick = false;
            // Entity.baseTick: the saved invulnerability window counts down
            // (arismoko/horizon#916).
            self.tick_saved_invulnerable_time(self.armor_stands[i].id);
            self.armor_stand_location_effects(i, false);
            // ArmorStand.travel: a marker or a NoGravity stand does not move.
            let travels = !self.armor_stands[i].marker && !self.armor_stands[i].no_gravity;
            // LivingEntity.baseTick: air below the max refills by 4 a tick
            // (increaseAirSupply), in water too, as armor_stand is in the
            // can_breathe_under_water tag. A stand's air is its loaded tag's.
            let id = self.armor_stands[i].id;
            if let Some(horizon_core::nbt::Tag::Compound(map)) = self.persist.raw.get_mut(&id)
                && let Some(air) = map
                    .get(&"Air".into())
                    .and_then(horizon_client::nbt_path::int_value)
                && air < 300
            {
                let air = (air + 4).min(300) as i16;
                map.insert("Air".into(), horizon_core::nbt::Tag::Short(air));
            }
            let a = &mut self.armor_stands[i];
            a.previous = a.position;
            a.previous_yaw = a.yaw;
            // aiStep's freezing step (removeFrost) makes the speed instance.
            a.speed_instance = true;
            // LivingEntity.aiStep -> EntityEquipment.tick, before movement.
            let a = &mut self.armor_stands[i];
            super::inventory_tick::equipment_inventory_tick(
                &self.world,
                self.res.facts(),
                self.dim.id(),
                &mut a.equipment,
            );
            let (water_height, lava_height) =
                a.push_by_fluid(&self.world, &self.res.motion().fluids, &self.loop_context);
            let in_water = water_height > 0.0;
            let medium =
                horizon_client::entities::StandMedium::from_heights(water_height, lava_height);
            // Entity.collide asks for the entities meeting the box stretched
            // by this tick's movement (the fluid push is already in it).
            nearby.clear();
            if travels {
                // The ECS leaves out the stand itself and what shares its vehicle (`collides_with`).
                let bounds = a.bounds().inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
                nearby = self.ecs.solid_colliders(a.id, bounds, [a.velocity.x, a.velocity.y, a.velocity.z]);
            }
            let (world, registry, facts, collision) = (
                &self.world,
                self.pack.assets.registry,
                self.res.facts(),
                self.collision,
            );
            let mut scene = horizon_client::collisions::SceneCollisions {
                world,
                binding: collision,
                subject: None,
            };
            let mut tops: Vec<BlockPos> = Vec::new();
            if let Some(step) = a.tick_move(
                &mut scene,
                facts,
                &self.res.motion().climbing,
                in_water,
                Some(self.border.geometry()),
                &nearby,
                &self.loop_context,
            ) {
                // LivingEntity.aiStep: `travel` first, then the blocks' push.
                a.tick_settle(medium, step);
                a.push_by_bubble_column_over(world, registry, facts, collision, |pos| {
                    tops.push(pos)
                });
            }
            let section_step = (a.id, a.previous, a.position);
            for pos in tops {
                self.bubble_column_particles(pos);
            }
            let a = &mut self.armor_stands[i];
            if in_water {
                a.enchantment_fall_distance = 0.0;
            } else {
                // Entity.checkFallDamage narrows each movement before adding to the double.
                a.enchantment_fall_distance +=
                    f64::from((a.previous.y - a.position.y).max(0.0) as f32);
            }
            let landed = a.on_ground && a.enchantment_fall_distance > 0.0;
            if a.on_ground {
                a.enchantment_fall_distance = 0.0;
            }
            // LivingEntity.checkFallDamage: onChangedBlock before the block's
            // fallOn, then aiStep's applyEffectsFromBlocks (stepOn, then the
            // blocks it is inside).
            if landed {
                self.armor_stand_location_effects(i, true);
            }
            self.armor_stand_turtle_eggs(i, landed)?;
            if landed {
                let stand = &self.armor_stands[i];
                let (position, bounds) = (stand.position, stand.bounds());
                self.post_landing_event(position, bounds, true);
            }
            self.armor_stands[i].apply_honey_slide(&self.world, self.res.facts());
            let a = &self.armor_stands[i];
            if !a.marker && !a.no_gravity {
                self.entity_powder_snow_particles(
                    a.previous,
                    a.position,
                    a.bounds(),
                    a.velocity,
                    true,
                );
            }
            // LivingEntity.aiStep calls pushEntities after travel. The stand's
            // own doPush is empty; only this shove runs.
            self.push_rideable_minecarts(i);
            let id = self.armor_stands[i].id;
            let saved = ["minecraft:on_fire", "minecraft:in_fire"]
                .map(|damage| self.saved_refuses_damage(id, damage, false));
            let tags = world_items!(self);
            let registry = self.pack.assets.registry;
            let block_registries = &self.pack.block_registries;
            let hooks = &self.damage_hooks;
            if let Some(damage_type) = self.armor_stands[i].environment_tick_with_hook(
                &mut self.world,
                self.res.facts(),
                registry,
                |world, damage| {
                    let vanilla = if damage == "minecraft:on_fire" {
                        saved[0]
                    } else {
                        saved[1]
                    };
                    let source = horizon_client::health::HurtSource {
                        damage_type: damage,
                        causing: 0,
                        direct: 0,
                        position: None,
                    };
                    horizon_client::health::DamageLevel::of(
                        world,
                        registry,
                        Some(tags),
                        Some(block_registries),
                        hooks,
                    )
                    .invulnerability_check(
                        horizon_client::health::Hurting::ArmorStand(id as i32, source),
                        vanilla,
                    )
                },
            ) {
                dead.push((i, damage_type));
            }
            let (id, from, to) = section_step;
            self.entity_section_step(id, [from.x, from.y, from.z], [to.x, to.y, to.z]);
        }
        for (i, damage_type) in dead.into_iter().rev() {
            self.break_armor_stand_by_anything(i, damage_type, 0);
        }
        Ok(())
    }
    /// A lid's `Entity.move` for armor stand `i`, one with physics (vanilla's
    /// `!noPhysics`: not a marker, not `NoGravity`), from the game's one lid
    /// push: the stand's own `shove` by `delta`. Stands tick
    /// before mobs here, while vanilla ticks a stand that joined after the
    /// pushing `shulker` (one placed on it) after the shulker: `shove` makes
    /// up for the order. A shulker box's lid moves it after every entity's tick.
    pub(super) fn lid_shoves_armor_stand(
        &mut self,
        i: usize,
        shulker: Option<u32>,
        delta: Vec3,
    ) {
        // `join` (the shulker's grip on a mob) has no component in P yet: an ECS mob is not read here (see the report).
        let joined = shulker.and_then(|id| self.remote_entities.iter().find(|m| m.id == id).map(|m| m.join));
        let a = &mut self.armor_stands[i];
        // Entity.collide: the entities meeting the box stretched by the push.
        let reach = a.bounds().expand_towards(delta);
        let reach = reach.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        let nearby = self.ecs.solid_colliders(a.id, reach, [0.0; 3]);
        let scene = horizon_client::collisions::SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        let travels_after = joined.is_some_and(|joined| a.join > joined);
        a.shove(
            scene,
            delta,
            Some(self.border.geometry()),
            &nearby,
            travels_after,
            if shulker.is_some() {
                horizon_world::entity_move::MoverType::Shulker
            } else {
                horizon_world::entity_move::MoverType::ShulkerBox
            },
            &self.loop_context,
        );
    }
    /// `Ravager.roar` with `mob_griefing` on: every armor stand in `area` (the
    /// ravager's box grown by 4) takes `strongKnockback` from the ravager at
    /// `from`. Its `mobAttack` leaves the stand whole (`ArmorStand.hurtServer`:
    /// not a damage type that breaks one).
    pub(in crate::game) fn roar_pushes_armor_stands(&mut self, area: Aabb, from: [f64; 3]) {
        for a in self.armor_stands.iter_mut() {
            let (width, height) = stand_box(a);
            let half = f64::from(width) / 2.0;
            let p = a.position;
            let bounds = Aabb::new(
                Vec3::new(p.x - half, p.y, p.z - half),
                Vec3::new(p.x + half, p.y + f64::from(height), p.z + half),
            );
            if a.health <= 0.0 || !bounds.intersects(area) {
                continue;
            }
            let (xd, zd) = (p.x - from[0], p.z - from[2]);
            let dd = (xd * xd + zd * zd).max(0.001);
            // Entity.push.
            a.velocity = a
                .velocity
                .add_vector(Vec3::new(xd / dd * 4.0, 0.2, zd / dd * 4.0));
        }
    }
    /// `ArmorStand.pushEntities`: a plain minecart (`RIDABLE_MINECARTS`,
    /// `AbstractMinecart.isRideable`) whose box meets the stand's is shoved
    /// when `distanceToSqr` is at most 0.2. `AbstractMinecart.push` of a
    /// non-cart adds the push to the cart and a quarter of it to the stand
    /// (`Entity.push(double)` does not consult `isPushable`). A stand with no
    /// physics (`Marker` or `NoGravity`, `ArmorStand.hasPhysics` / `noPhysics`)
    /// or one riding that cart does not shove it.
    fn push_rideable_minecarts(&mut self, i: usize) {
        let stand = &self.armor_stands[i];
        if stand.marker || stand.no_gravity {
            return;
        }
        let stand_pos = stand.position;
        let stand_box = stand.bounds();
        let vehicle = stand.vehicle;
        let mut applied = Vec::new();
        for (j, cart) in self.carts.iter().enumerate() {
            let Some(shove) = armor_stand_cart_shove(
                stand_pos,
                stand_box,
                cart.position,
                cart.bounds(),
                cart.kind,
                vehicle == Some(cart.id),
            ) else {
                continue;
            };
            applied.push((j, shove));
        }
        let mut stand_x = 0.0;
        let mut stand_z = 0.0;
        for (j, shove) in applied {
            self.carts.update(j, |cart| {
                cart.velocity.x += shove.cart_x;
                cart.velocity.z += shove.cart_z;
            });
            stand_x += shove.stand_x;
            stand_z += shove.stand_z;
        }
        if stand_x != 0.0 || stand_z != 0.0 {
            let stand = &mut self.armor_stands[i];
            stand.velocity.x += stand_x;
            stand.velocity.z += stand_z;
        }
    }
    /// An armor stand is a `LivingEntity`, so the egg under it
    /// (`getOnPosLegacy`) cracks like under a mob: `TurtleEggBlock.fallOn` one
    /// landing in 3 (`Entity.move`'s `checkFallDamage`), then `stepOn` one tick
    /// in 100 while it is on the ground (`LivingEntity.aiStep`'s
    /// `applyEffectsFromBlocks`, after `travel`, so a marker that has landed
    /// still steps). `canDestroyEgg` wants mob griefing before the level's roll.
    fn armor_stand_turtle_eggs(&mut self, i: usize, landed: bool) -> Result<()> {
        let stand = &self.armor_stands[i];
        if !stand.on_ground {
            return Ok(());
        }
        let p = stand.on_pos_legacy(|p| {
            let name = &self.res.facts().get(self.world.block(p.x, p.y, p.z)).name;
            let tags = &self.res.items().block_tags;
            [
                "minecraft:fences",
                "minecraft:walls",
                "minecraft:fence_gates",
            ]
            .iter()
            .any(|t| tags.get(*t).is_some_and(|s| s.contains(&**name)))
        });
        for chance in [landed.then_some(3), Some(100)].into_iter().flatten() {
            let egg = self
                .pack
                .assets
                .registry
                .state(self.world.block(p.x, p.y, p.z))
                .is_some_and(|s| s.name == "minecraft:turtle_egg");
            if egg
                && self
                    .session
                    .rules
                    .bool_at(horizon_client::gamerules::vanilla::MOB_GRIEFING)
                && self.world.random.next_int_bound(chance).unwrap_or(1) == 0
            {
                self.crack_turtle_egg([p.x, p.y, p.z])?;
            }
        }
        Ok(())
    }
    /// `ArmorStand.brokenByAnything` rolls the stand's entity loot table before
    /// the equipment. The resulting list crosses the existing living-drops hook;
    /// the stand item and equipment pop outside it, as in vanilla.
    fn armor_stand_death_drops(&mut self, stand: &ArmorStand, damage_type: &str, causing: u32) {
        let source = (!mod_api::__private::mods().is_empty())
            .then(|| super::creatures::death_hurt_source(damage_type, causing));
        let dying = super::creatures::DyingDrops {
            entity: stand.id,
            kind: "minecraft:armor_stand",
            player: None,
            source,
            recently_hit: false,
        };
        let (dying, stacks) = armor_stand_dying_drops(
            stand,
            self.session
                .rules
                .bool_at(horizon_client::gamerules::vanilla::MOB_DROPS),
            dying,
            |table| {
                let Some(loot) = horizon_client::structure_blocks::loot_data(
                    &self.options.resources,
                    &self.pack_files(),
                    self.res.item_defaults(),
                ) else {
                    return Vec::new();
                };
                if !loot.has_table(table) {
                    return Vec::new();
                }
                let params = horizon_world::loot::EntityLootParams {
                    this: horizon_world::loot::LootEntity {
                        on_fire: stand.fire_ticks > 0,
                        baby: false,
                        ..horizon_world::loot::LootEntity::new("minecraft:armor_stand")
                    },
                    ..Default::default()
                };
                match loot.random_sequence(table) {
                    Some(sequence) => loot.roll_entity(
                        table,
                        self.rand_sequences.get(sequence, self.level.seed),
                        &params,
                    ),
                    None => loot.roll_entity(table, &mut self.world.random, &params),
                }
            },
        );
        self.drop_death_items(
            &dying,
            stacks
                .into_iter()
                .map(|(stack, at)| (stack, at, 0))
                .collect(),
            10,
            |_| None,
        );
    }
    /// `ArmorStand.brokenByAnything`, then `kill`, as burning out or a blast
    /// breaks a stand: its equipment pops above its block, the stand item does
    /// not drop.
    pub(super) fn break_armor_stand_by_anything(
        &mut self,
        i: usize,
        damage_type: &str,
        causing: u32,
    ) {
        let a = self.armor_stands.remove(i);
        let at = [a.position.x, a.position.y, a.position.z];
        self.armor_stand_death_drops(&a, damage_type, causing);
        let source = self
            .level_entity(causing as i32)
            .or(Some(super::commands::Target::Stand(a.id)));
        let (pos, drops) = broken_armor_stand_equipment(a);
        for stack in drops {
            self.pop_resource(pos, stack);
        }
        self.audio.play("minecraft:entity.armor_stand.break", Some(at), 1.0, 1.0);
        self.entity_killed_event_by(at, source);
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    }
    /// `break_armor_stand_by_anything` on the ECS level: the equipment above the cell pops (P's `take_slot`), the sound, the
    /// killed event, then the entity is despawned. The loot roll and the vanishing filter are not in this branch yet.
    fn break_armor_stand_by_anything_ecs(&mut self, e: EcsEntity, id: u32, causing: u32) {
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
    /// `command_damage_stand` on the ECS level: the gates are the host's; the stand's flags, life and fire are P's.
    fn command_damage_stand_ecs(&mut self, id: u32, damage_type: &str, causing: Option<super::commands::Target>) -> bool {
        use super::commands::Target;
        let Some(e) = self.ecs.entity_of(id) else { return false };
        let registry = &self.health_resources.damage;
        let Some(kind) = registry.id(damage_type) else { return false };
        if !self.session.rules.bool_at(horizon_client::gamerules::vanilla::MOB_GRIEFING) && matches!(causing, Some(Target::Mob(_))) {
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
            .is_some_and(|c| horizon_client::entity_nbt::flag(&horizon_core::nbt::Tag::Compound(c), "Invulnerable"));
        let hidden = self
            .ecs
            .world()
            .and_then(|w| w.get::<EcsStand>(e))
            .is_some_and(|s| s.flags & (ecs_stand::INVISIBLE | ecs_stand::MARKER) != 0);
        let equipment = (0..6).map(|slot| self.ecs.equipment_stack(id, slot)).collect();
        let immune = self.stand_immune_to_damage(equipment, damage_type, causing);
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
            self.break_armor_stand_by_anything_ecs(e, id, causing_id);
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
        self.break_armor_stand_by_hit_ecs(e, creative, always_kills, by)
    }
    /// `ArmorStand.hurtServer` for a command's damage of `damage_type` from the
    /// causing entity `causing`, up to its sources that act and still return
    /// false: a blast breaks the stand (`brokenByAnything`, `kill`), fire that
    /// ignites stands lights it (or burns a burning one 0.15), and burning
    /// takes 4 health, all returning false. A source in
    /// `#can_break_armor_stand` or `#always_kills_armor_stands` hurts the
    /// stand as a player's hit does and returns true; every other source is
    /// refused (false), as `/damage` fails for it in vanilla.
    pub(super) fn command_damage_stand(
        &mut self,
        id: u32,
        damage_type: &str,
        causing: Option<super::commands::Target>,
    ) -> bool {
        use super::commands::Target;
        if self.ecs.is_active() {
            return self.command_damage_stand_ecs(id, damage_type, causing);
        }
        let Some(i) = self.armor_stands.iter().position(|a| a.id == id) else {
            return false;
        };
        let registry = &self.health_resources.damage;
        let Some(kind) = registry.id(damage_type) else {
            return false;
        };
        if !self
            .session
            .rules
            .bool_at(horizon_client::gamerules::vanilla::MOB_GRIEFING)
            && matches!(causing, Some(Target::Mob(_)))
        {
            return false;
        }
        if registry.in_tag(kind, "minecraft:bypasses_invulnerability") {
            self.kill_target(Target::Stand(id));
            return false;
        }
        // Entity.isInvulnerableToBase: the flag, unless a creative player hits.
        let creative = match causing {
            Some(Target::Player) => self.creative,
            Some(Target::Guest(conn)) => self.command_guest(conn).is_some_and(|g| g.creative()),
            _ => false,
        };
        let invulnerable = self
            .persist
            .raw
            .get(&id)
            .is_some_and(|tag| horizon_client::entity_nbt::flag(tag, "Invulnerable"));
        // Entity.isInvulnerableToBase: the flag, then EnchantmentHelper.isImmuneToDamage.
        let a = &self.armor_stands[i];
        if (invulnerable && !creative)
            || a.invisible
            || a.marker
            || self.stand_immune_to_damage(self.armor_stands[i].equipment.iter().cloned().collect(), damage_type, causing)
        {
            return false;
        }
        // ArmorStand.hurtServer attributes its game events to the source
        // entity: the player, a LAN guest or the mob.
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
        let causing_id = causing
            .and_then(|target| self.network_entity_id(target))
            .unwrap_or(0) as u32;
        let (breaks, always_kills) = (
            tagged("minecraft:can_break_armor_stand"),
            tagged("minecraft:always_kills_armor_stands"),
        );
        if explosion {
            self.break_armor_stand_by_anything(i, damage_type, causing_id);
            return false;
        }
        if ignites {
            if self.armor_stands[i].fire_ticks > 0 {
                self.cause_stand_damage(i, 0.15, damage_type, causing_id);
            } else {
                // igniteForSeconds(5.0f): at least 100 ticks, and
                // Entity.igniteForTicks calls clearFreeze.
                let a = &mut self.armor_stands[i];
                a.fire_ticks = a.fire_ticks.max(100);
                if let Some(horizon_core::nbt::Tag::Compound(map)) = self.persist.raw.get_mut(&id) {
                    map.remove(&"TicksFrozen".into());
                }
            }
            return false;
        }
        if burns && self.armor_stands[i].health > 0.5 {
            self.cause_stand_damage(i, 4.0, damage_type, causing_id);
            return false;
        }
        if !breaks && !always_kills {
            return false;
        }
        // A causing player who may not build (adventure, spectator) cannot.
        let may_build = match causing {
            Some(Target::Player) => self.may_build(),
            Some(Target::Guest(conn)) => self
                .command_guest(conn)
                .is_none_or(|g| g.abilities.may_build),
            _ => true,
        };
        if !may_build {
            return false;
        }
        self.break_armor_stand_by_hit(i, creative, always_kills, by)
    }
    /// `EnchantmentHelper.isImmuneToDamage` for the stand at `i`: whether an
    /// enchantment on an active equipment slot has a `minecraft:damage_immunity`
    /// effect whose requirements hold for `damage_type` and `causing`.
    fn stand_immune_to_damage(
        &mut self,
        equipment: Vec<Option<ItemStack>>,
        damage_type: &str,
        causing: Option<super::commands::Target>,
    ) -> bool {
        use super::commands::Target;
        let mut checks = Vec::new();
        for (stack, slot) in equipment.iter().zip(ArmorStand::SLOTS) {
            let Some(stack) = stack else { continue };
            let armor = matches!(slot, "head" | "chest" | "legs" | "feet");
            let hand = matches!(slot, "mainhand" | "offhand");
            for (id, level) in stack.enchantments() {
                let Some(def) = self.res.stations().enchantments.get(id) else {
                    continue;
                };
                if !def.slots.iter().any(|s| {
                    s == slot || s == "any" || (s == "armor" && armor) || (s == "hand" && hand)
                }) {
                    continue;
                }
                let immunity = &def.effects["minecraft:damage_immunity"];
                for entry in immunity.as_array().into_iter().flatten() {
                    checks.push((entry.get("requirements").cloned(), *level));
                }
            }
        }
        if checks.is_empty() {
            return false;
        }
        let attacker = match causing {
            Some(Target::Player | Target::Guest(_)) => Some("minecraft:player".to_owned()),
            Some(Target::Mob(m_id)) => {
                if self.ecs.is_active() {
                    self.fixed_kind(m_id)
                } else {
                    self.remote_entities
                        .iter()
                        .find(|m| m.id == m_id)
                        .map(|m| m.kind.id.clone())
                }
            }
            Some(Target::Stand(_)) => Some("minecraft:armor_stand".to_owned()),
            _ => None,
        };
        let canonical = if damage_type.contains(':') {
            damage_type.to_owned()
        } else {
            format!("minecraft:{damage_type}")
        };
        let ctx = horizon_client::enchant::EffectContext {
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
        for (req, level) in checks {
            if req.is_none_or(|r| ench.condition(&r, level, &ctx, rng)) {
                return true;
            }
        }
        false
    }
    /// `ArmorStand.causeDamage`: the health less `amount`, broken by anything
    /// at 0.5 or less.
    fn cause_stand_damage(&mut self, i: usize, amount: f32, damage_type: &str, causing: u32) {
        let health = self.armor_stands[i].health - amount;
        if health <= 0.5 {
            self.break_armor_stand_by_anything(i, damage_type, causing);
        } else {
            let at = self.armor_stands[i].position;
            self.armor_stands[i].health = health;
            self.stand_game_event(
                "minecraft:entity_damage",
                [at.x, at.y, at.z],
                self.level_entity(causing as i32),
            );
        }
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
    let equippable = horizon_client::item_sync::equippable(stack, items)?;
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
                horizon_client::item_data::component_bool(flag).is_some()
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

/// The `ArmorStand.equipment` index `ArmorStand.interact` puts `stack` in:
/// the stack's own `equippable` slot, else the main hand.
fn stand_slot(items: &crate::items::Items, stack: &ItemStack) -> usize {
    match horizon_client::item_sync::equippable_slot(stack, items).as_deref() {
        Some("feet") => 2,
        Some("legs") => 3,
        Some("chest") => 4,
        Some("head") => 5,
        Some("offhand") => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod stack_equip_tests {
    use super::{hand_equip_slot, stand_slot};
    use horizon_client::items::{ItemInfo, ItemStack, Items};
    use horizon_core::nbt::Tag;

    fn with_equippable(fields: Vec<(&str, Tag)>) -> ItemStack {
        let mut stack = ItemStack::new("minecraft:stick", 1);
        stack
            .data_mut()
            .set_other("minecraft:equippable", Some(Tag::compound(fields)));
        stack
    }

    /// A stack's own `equippable` slot, not its item type's, decides where a
    /// right-click wears it; a removed component wears nowhere.
    #[test]
    fn the_hands_armour_slot_comes_from_the_stacks_equippable() {
        let items = Items::for_tests(Default::default());
        let helmet = with_equippable(vec![("slot", Tag::String("head".into()))]);
        assert_eq!(hand_equip_slot(&helmet, &items), Some((0, "head")));
        assert_eq!(
            hand_equip_slot(
                &with_equippable(vec![("slot", Tag::String("chest".into()))]),
                &items
            ),
            Some((1, "chest"))
        );
        let unsupported = with_equippable(vec![("slot", Tag::String("mainhand".into()))]);
        assert_eq!(hand_equip_slot(&unsupported, &items), None);
        let unswappable = with_equippable(vec![
            ("slot", Tag::String("head".into())),
            ("swappable", Tag::Byte(0)),
        ]);
        assert_eq!(hand_equip_slot(&unswappable, &items), None);

        // The item's own component answers while the stack removes it.
        let mut items = Items::for_tests(Default::default());
        items.items.insert(
            "minecraft:iron_helmet".into(),
            ItemInfo {
                equip_slot: Some("head".into()),
                ..Default::default()
            },
        );
        let mut removed = ItemStack::new("minecraft:iron_helmet", 1);
        removed.data_mut().set_other(
            "!minecraft:equippable",
            Some(Tag::compound(Vec::<(&str, Tag)>::new())),
        );
        assert_eq!(hand_equip_slot(&removed, &items), None);
    }

    /// `ArmorStand.interact` puts a stack in the slot its `equippable` names,
    /// not the one its item type names.
    #[test]
    fn an_armor_stands_slot_comes_from_the_stacks_equippable() {
        let items = Items::for_tests(Default::default());
        assert_eq!(
            stand_slot(
                &items,
                &with_equippable(vec![("slot", Tag::String("feet".into()))])
            ),
            2
        );
        assert_eq!(
            stand_slot(
                &items,
                &with_equippable(vec![("slot", Tag::String("offhand".into()))])
            ),
            1
        );
        assert_eq!(stand_slot(&items, &ItemStack::new("minecraft:stick", 1)), 0);
        let mut removed = ItemStack::new("minecraft:stick", 1);
        removed.data_mut().set_other(
            "!minecraft:equippable",
            Some(Tag::compound(Vec::<(&str, Tag)>::new())),
        );
        assert_eq!(stand_slot(&items, &removed), 0);
    }
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
impl Game<'_> {
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
mod swap_tests {
    use super::*;
    fn stack(item: &str, count: u32) -> ItemStack {
        ItemStack::new(item, count)
    }
    #[test]
    fn disabled_slots_lock_by_filter_flag() {
        let mut a = ArmorStand::new(1, horizon_core::geometry::Vec3::ZERO, 0.0);
        // Hands need arms.
        assert!(a.is_disabled(0) && a.is_disabled(1));
        a.show_arms = true;
        assert!(!a.is_disabled(0) && !a.is_disabled(1));
        // Bit 4 locks the head (equipment index 5), bit 5 the off hand (1).
        a.disabled_slots = (1 << 4) | (1 << 5);
        assert!(a.is_disabled(5) && a.is_disabled(1));
        assert!(!a.is_disabled(4) && !a.is_disabled(0));
        // Taking (+8) from a filled slot and putting (+16) into an empty one.
        a.disabled_slots = (1 << (3 + 8)) | (1 << (1 + 16));
        assert!(!a.swap_locked(4), "an empty chest slot is not take-locked");
        a.equipment[4] = Some(stack("minecraft:iron_chestplate", 1));
        assert!(a.swap_locked(4));
        assert!(a.swap_locked(2), "an empty feet slot is put-locked");
        a.equipment[2] = Some(stack("minecraft:iron_boots", 1));
        assert!(!a.swap_locked(2));
    }
    #[test]
    fn creative_swap_gives_the_worn_piece_to_the_hand() {
        let swap = swap_with_equipment_slot(
            &stack("minecraft:diamond_chestplate", 1),
            Some(stack("minecraft:iron_chestplate", 1)),
            true,
        );
        assert_eq!(swap.worn, stack("minecraft:diamond_chestplate", 1));
        assert_eq!(swap.hand, Some(stack("minecraft:iron_chestplate", 1)));
        assert_eq!(swap.to_inventory, None);
    }
    #[test]
    fn creative_equip_with_nothing_worn_keeps_the_hand() {
        let swap = swap_with_equipment_slot(&stack("minecraft:diamond_helmet", 1), None, true);
        assert_eq!(swap.worn, stack("minecraft:diamond_helmet", 1));
        assert_eq!(swap.hand, Some(stack("minecraft:diamond_helmet", 1)));
    }
    #[test]
    fn survival_swap_gives_the_worn_piece_or_empties_the_hand() {
        let worn = Some(stack("minecraft:iron_boots", 1));
        let swap = swap_with_equipment_slot(&stack("minecraft:diamond_boots", 1), worn, false);
        assert_eq!(swap.hand, Some(stack("minecraft:iron_boots", 1)));
        let swap = swap_with_equipment_slot(&stack("minecraft:diamond_boots", 1), None, false);
        assert_eq!(swap.hand, None);
    }
    #[test]
    fn a_larger_stack_wears_one_and_sends_the_old_piece_to_the_inventory() {
        // Armour stacks past one only with a max_stack_size component.
        let worn = Some(stack("minecraft:leather_helmet", 1));
        let swap =
            swap_with_equipment_slot(&stack("minecraft:golden_helmet", 3), worn.clone(), false);
        assert_eq!(swap.worn, stack("minecraft:golden_helmet", 1));
        assert_eq!(swap.hand, Some(stack("minecraft:golden_helmet", 2)));
        assert_eq!(swap.to_inventory, worn);
        let swap =
            swap_with_equipment_slot(&stack("minecraft:golden_helmet", 3), worn.clone(), true);
        assert_eq!(swap.hand, Some(stack("minecraft:golden_helmet", 3)));
        assert_eq!(swap.to_inventory, worn);
    }
}
#[cfg(test)]
mod burn_drop_tests {
    use super::*;
    #[test]
    fn only_visible_non_marker_stands_are_broken_by_blasts() {
        let mut stand = ArmorStand::new(1, Vec3::new(0.5, 64.0, 0.5), 0.0);
        assert!(!spared_by_blast(&stand));
        stand.invisible = true;
        assert!(spared_by_blast(&stand));
        stand.invisible = false;
        stand.marker = true;
        assert!(spared_by_blast(&stand));
    }
    #[test]
    fn burnt_equipment_drops_above_the_block_and_vanishing_items_do_not_drop() {
        let mut stand = ArmorStand::new(1, Vec3::new(-0.1, 4.8, 0.0), 0.0);
        let kept = ItemStack::new("minecraft:diamond_boots", 1);
        let mut vanished = ItemStack::new("minecraft:diamond_helmet", 1);
        vanished
            .data_mut()
            .enchantments
            .push(("minecraft:vanishing_curse".into(), 1));
        stand.equipment[0] = Some(kept.clone());
        stand.equipment[1] = Some(vanished);
        let (pos, drops) = broken_armor_stand_equipment(stand);
        assert_eq!(pos, [-1, 5, 0]);
        assert_eq!(drops, vec![kept]);
    }
    #[test]
    fn a_broken_stand_pops_body_and_saddle_after_the_other_six_slots() {
        use horizon_items::equipment::{BODY, HEAD, MAINHAND, SADDLE};
        let mut stand = ArmorStand::new(1, Vec3::new(0.5, 4.0, 0.5), 0.0);
        let stacks: Vec<_> = [SADDLE, BODY, HEAD, MAINHAND]
            .into_iter()
            .map(|slot| {
                let mut stack = ItemStack::new("minecraft:stone", 1 + slot as u32);
                stack.damage = slot as u32;
                stand.equipment[slot] = Some(stack.clone());
                (slot, stack)
            })
            .collect();
        let (_, drops) = broken_armor_stand_equipment(stand);
        let order = |slots: [usize; 4]| {
            slots
                .map(|s| {
                    stacks
                        .iter()
                        .find(|(slot, _)| *slot == s)
                        .unwrap()
                        .1
                        .clone()
                })
                .to_vec()
        };
        assert_eq!(drops, order([MAINHAND, HEAD, BODY, SADDLE]));
    }
}
#[cfg(test)]
mod push_tests {
    use super::super::minecarts::{CartKind, Minecart};
    use super::*;

    fn boxes(kind: CartKind, cart_at: Vec3) -> (Aabb, Aabb) {
        let stand = ArmorStand::new(1, Vec3::ZERO, 0.0);
        let cart = Minecart::new(2, kind, cart_at);
        (stand.bounds(), cart.bounds())
    }

    #[test]
    fn a_rideable_minecart_inside_the_0_2_gate_is_shoved() {
        // 0.25² is inside the 0.2 gate; 0.5² is not. The function's Some/None is the check.
        let near = Vec3::new(0.25, 0.0, 0.0);
        let far = Vec3::new(0.5, 0.0, 0.0);
        let (stand_box, cart_box) = boxes(CartKind::Rideable, near);
        let shove = armor_stand_cart_shove(
            Vec3::ZERO,
            stand_box,
            near,
            cart_box,
            CartKind::Rideable,
            false,
        )
        .expect("a rideable cart 0.25 away is shoved");
        // AbstractMinecart.push: gap 0.25, length 0.25, pow capped at 1, then 0.1F * 0.5.
        // The cart moves east, away from the stand; the stand takes a quarter west.
        let scale = f64::from(0.1_f32) * 0.5;
        assert_eq!(shove.cart_x, scale);
        assert_eq!(shove.cart_z, 0.0);
        assert_eq!(shove.stand_x, -scale / 4.0);
        assert_eq!(shove.stand_z, 0.0);

        let (stand_box, cart_box) = boxes(CartKind::Rideable, far);
        assert!(
            armor_stand_cart_shove(
                Vec3::ZERO,
                stand_box,
                far,
                cart_box,
                CartKind::Rideable,
                false
            )
            .is_none(),
            "past 0.2 the cart stays"
        );
        for kind in [
            CartKind::Chest,
            CartKind::Hopper,
            CartKind::Furnace,
            CartKind::Tnt,
            CartKind::CommandBlock,
            CartKind::Spawner,
        ] {
            let (stand_box, cart_box) = boxes(kind, near);
            assert!(
                armor_stand_cart_shove(Vec3::ZERO, stand_box, near, cart_box, kind, false)
                    .is_none(),
                "{} is not a rideable minecart",
                kind.id()
            );
        }
        let (stand_box, cart_box) = boxes(CartKind::Rideable, near);
        assert!(
            armor_stand_cart_shove(
                Vec3::ZERO,
                stand_box,
                near,
                cart_box,
                CartKind::Rideable,
                true
            )
            .is_none(),
            "a cart does not shove the stand riding it"
        );
    }
}
#[cfg(test)]
mod pumpkin_equippable_use_tests {
    use super::*;
    use horizon_client::lan::{Joined, NetConfig, PlayerSender, guest::Guest, hub::HubConfig};
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
                std::sync::Arc::new(horizon_client::chunk::Chunk::empty(0, 0)),
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
                    integrated: false,
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
                    assert_eq!(actual.get("swappable").and_then(horizon_client::item_data::component_bool), if label == "false" { Some(false) } else { None });
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

#[cfg(test)]
mod stand_lodestone_inventory_tick_tests {
    use horizon_client::motion::PlayerMotion;
    use horizon_core::{geometry::Vec3, position::BlockPos};

    #[test]
    #[ignore = "daily"]
    fn actual_stand_equipment_compasses_tick_after_loaded_lodestone_removal() {
        let mut resources = None;
        let mut game = crate::game::game_tests::tests::server_game(&mut resources);
        game.build_registries();
        game.set_allow_commands(true);
        game.level.health = 20.0;
        game.player = Some(PlayerMotion::new(game.motion, Vec3::new(14.5, 200.0, 14.5)).unwrap());
        game.block_ticks
            .add_empty_chunk(BlockPos::new(0, 0, 0).chunk())
            .unwrap();
        assert!(game.run_command("forceload add 0 0"));
        assert!(game.run_command("fill 0 199 0 15 199 15 stone"));
        assert!(game.run_command("setblock 12 200 12 lodestone"));
        assert!(game.world.loaded(12, 12));
        let mut ids = Vec::new();
        for (n, dim, tracked, target) in [
            (1, "minecraft:overworld", true, true),
            (2, "minecraft:overworld", false, true),
            (3, "minecraft:the_nether", true, true),
            (4, "minecraft:overworld", true, false),
        ] {
            let who = format!("0000a551-0000-0000-0000-{n:012x}");
            let x = f64::from(n * 2) + 0.5;
            assert!(game.run_command(&format!("summon armor_stand {x} 200.0 4.5 {{UUID:[I;42321,0,0,{n}],Health:20f,NoGravity:1b}}")));
            let id = game
                .armor_stands
                .iter()
                .find(|a| game.persist.uuid_of(a.id) == Some([42321, 0, 0, n]))
                .unwrap()
                .id;
            let tracker = format!(
                "{}tracked:{tracked}",
                if target {
                    format!("target:{{dimension:\"{dim}\",pos:[I;12,200,12]}},")
                } else {
                    String::new()
                }
            );
            for hand in ["mainhand", "offhand"] {
                assert!(game.run_command(&format!("item replace entity {who} weapon.{hand} with compass[minecraft:lodestone_tracker={{{tracker}}},minecraft:custom_data={{marker:{n}}}] 1")));
            }
            let a = game.armor_stands.iter().find(|a| a.id == id).unwrap();
            assert_eq!(a.position, Vec3::new(x, 200.0, 4.5));
            assert!(a.no_gravity);
            assert!(a.first_tick, "fresh actual stand has not ticked yet");
            assert_eq!(a.health, 20.0);
            for stack in a.equipment.iter().take(2).flatten() {
                assert_eq!(&*stack.item, "minecraft:compass");
                assert_eq!(stack.count, 1);
                assert_eq!(
                    stack
                        .data()
                        .unwrap()
                        .other("minecraft:lodestone_tracker")
                        .unwrap()
                        .get("target")
                        .is_some(),
                    target
                );
            }
            assert!(game.persist.raw.remove(&id).is_some());
            ids.push((id, n, target));
        }
        for hand in ["mainhand", "offhand"] {
            assert!(game.run_command(&format!("item replace entity @s weapon.{hand} with compass[minecraft:lodestone_tracker={{target:{{dimension:\"minecraft:overworld\",pos:[I;12,200,12]}},tracked:true}}] 1")));
        }
        assert!(game.run_command("setblock 12 200 12 air"));
        assert_eq!(
            &*game.res.facts().get(game.world.block(12, 200, 12)).name,
            "minecraft:air"
        );
        game.world.game_time += 1;
        game.tick_lodestone_compasses();
        for slot in [0, crate::game::shield::OFFHAND_SLOT] {
            assert!(
                game.stack_in(slot)
                    .unwrap()
                    .data()
                    .unwrap()
                    .other("minecraft:lodestone_tracker")
                    .unwrap()
                    .get("target")
                    .is_none(),
                "actual player tick positive control"
            );
        }
        assert!(
            game.entity_ticking_area()(2, 4),
            "actual entity-ticking chunk admission"
        );
        game.tick_armor_stands().unwrap();
        let mut mismatches = Vec::new();
        for (id, n, target) in ids {
            let a = game.armor_stands.iter().find(|a| a.id == id).unwrap();
            assert_eq!(a.position, Vec3::new(f64::from(n * 2) + 0.5, 200.0, 4.5));
            assert_eq!(a.health, 20.0);
            assert!(a.no_gravity);
            for (slot, stack) in a.equipment.iter().take(2).enumerate() {
                let stack = stack.as_ref().unwrap();
                assert_eq!(stack.count, 1);
                let data = stack.data().unwrap();
                assert_eq!(
                    data.other("minecraft:custom_data").unwrap().get("marker"),
                    Some(&horizon_core::nbt::Tag::Int(n))
                );
                let present = data
                    .other("minecraft:lodestone_tracker")
                    .unwrap()
                    .get("target")
                    .is_some();
                if present != (target && n != 1) {
                    mismatches.push((n, slot, present));
                }
            }
            let saved = horizon_client::entity_nbt::armor_stand_to_nbt(a, [42321, 0, 0, n], None);
            assert!(
                saved.get("equipment").is_some(),
                "actual no-raw model writer"
            );
            for slot in 0..2 {
                let key = horizon_client::entities::ArmorStand::SLOTS[slot];
                let saved_target = saved
                    .get("equipment")
                    .unwrap()
                    .get(key)
                    .unwrap()
                    .get("components")
                    .unwrap()
                    .get("minecraft:lodestone_tracker")
                    .unwrap()
                    .get("target")
                    .is_some();
                let modeled_target = a.equipment[slot]
                    .as_ref()
                    .unwrap()
                    .data()
                    .unwrap()
                    .other("minecraft:lodestone_tracker")
                    .unwrap()
                    .get("target")
                    .is_some();
                assert_eq!(
                    saved_target, modeled_target,
                    "no-raw writer preserves actual tracker state"
                );
            }
        }
        assert!(
            mismatches.is_empty(),
            "actual stand equipment inventory tick mismatches: {mismatches:?}"
        );
    }
}

#[cfg(test)]
mod postbaseline_lodestone_controls {
    //! Added after baseline: source-backed slots and preservation, not new runtime breadth.
    use super::Game;
    use horizon_client::{chunk::Chunk, entities::ArmorStand, items::ItemStack};
    use horizon_core::position::BlockPos;
    use std::sync::Arc;

    fn stack<'g>(game: &'g Game<'_>, id: u32, slot: usize) -> &'g ItemStack {
        game.armor_stands
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .equipment[slot]
            .as_ref()
            .unwrap()
    }
    fn has_target(stack: &ItemStack) -> bool {
        stack
            .data()
            .unwrap()
            .other("minecraft:lodestone_tracker")
            .unwrap()
            .get("target")
            .is_some()
    }
    fn item(id: &str, tracker: &str, marker: usize, count: usize) -> String {
        format!(
            "{{id:\"{id}\",count:{count},components:{{\"minecraft:lodestone_tracker\":{{{tracker}}},\"minecraft:custom_data\":{{marker:{marker}}}}}}}"
        )
    }

    #[test]
    #[ignore = "daily"]
    fn postbaseline_six_slots_noncompass_guard_and_dimension_preservation() {
        let mut resources = None;
        let mut game = crate::game::game_tests::tests::server_game(&mut resources);
        game.build_registries();
        game.set_allow_commands(true);
        game.block_ticks
            .add_empty_chunk(BlockPos::new(0, 0, 0).chunk())
            .unwrap();
        assert!(game.run_command("forceload add 0 0"));
        assert!(game.run_command("fill 0 199 0 15 199 15 stone"));
        assert!(game.run_command("setblock 12 200 12 lodestone"));
        let tracked = "target:{dimension:\"minecraft:overworld\",pos:[I;12,200,12]},tracked:true";
        let slots = ArmorStand::SLOTS
            .iter()
            .enumerate()
            .map(|(slot, key)| format!("{key}:{}", item("minecraft:compass", tracked, slot, 1)))
            .collect::<Vec<_>>()
            .join(",");
        assert!(game.run_command(&format!("summon armor_stand 4.5 200.0 4.5 {{UUID:[I;42321,0,0,100],Health:20f,NoGravity:1b,equipment:{{{slots}}}}}")));
        let mixed = format!(
            "mainhand:{},offhand:{},head:{},feet:{}",
            item("minecraft:stone", tracked, 66, 3),
            item(
                "minecraft:compass",
                "target:{dimension:\"minecraft:the_nether\",pos:[I;12,200,12]},tracked:true",
                77,
                1
            ),
            item(
                "minecraft:compass",
                "target:{dimension:\"minecraft:overworld\",pos:[I;12,200,12]},tracked:false",
                88,
                1
            ),
            item("minecraft:compass", "tracked:true", 99, 1)
        );
        assert!(game.run_command(&format!("summon armor_stand 8.5 200.0 8.5 {{UUID:[I;42321,0,0,101],Health:20f,NoGravity:1b,equipment:{{{mixed}}}}}")));
        game.world
            .chunks
            .insert((4, 4), Arc::new(Chunk::empty(4, 4)));
        game.block_ticks
            .add_empty_chunk(BlockPos::new(64, 0, 64).chunk())
            .unwrap();
        game.load_ticking_neighbourhood(0, 0);
        assert!(game.run_command("fill 64 199 64 79 199 79 stone"));
        assert!(game.run_command(&format!("summon armor_stand 65.5 200.0 65.5 {{UUID:[I;42321,0,0,102],Health:20f,NoGravity:1b,equipment:{{mainhand:{}}}}}",item("minecraft:compass",tracked,100,1))));
        let id = |n| {
            game.armor_stands
                .iter()
                .find(|a| game.persist.uuid_of(a.id) == Some([42321, 0, 0, n]))
                .unwrap()
                .id
        };
        let (all, mixed, inactive) = (id(100), id(101), id(102));
        // Direct class ticks do not refresh the headless fixture's tickets.
        // Use the existing ticket model: only chunk zero is forced to tick.
        game.tickets.update(Some(crate::ticket_levels::Sources {
            forced: vec![(0, 0)],
            ..Default::default()
        }));
        assert!(game.entity_ticking_area()(4, 4));
        assert!(!game.entity_ticking_area()(65, 65));
        assert!(game.world.loaded(65, 65));
        let before: Vec<_> = (0..6)
            .map(|slot| {
                game.armor_stands
                    .iter()
                    .find(|a| a.id == mixed)
                    .unwrap()
                    .equipment[slot]
                    .as_ref()
                    .map(horizon_items::item_nbt::item_to_nbt)
            })
            .collect();
        assert!(has_target(stack(&game, mixed, 0)));
        assert!(has_target(stack(&game, mixed, 1)));
        assert!(has_target(stack(&game, mixed, 5)));
        assert!(!has_target(stack(&game, mixed, 2)));
        assert!(has_target(stack(&game, inactive, 0)));
        for slot in 0..6 {
            assert!(has_target(stack(&game, all, slot)));
        }
        let dimension = game.dim;
        let chunks: std::collections::HashSet<_> = game.world.chunks.keys().copied().collect();
        assert!(game.run_command("setblock 12 200 12 air"));
        game.world.game_time += 1;
        game.tick_armor_stands().unwrap();
        assert!(
            !game
                .armor_stands
                .iter()
                .find(|a| a.id == all)
                .unwrap()
                .first_tick,
            "active stand really ticked"
        );
        assert!(
            game.armor_stands
                .iter()
                .find(|a| a.id == inactive)
                .unwrap()
                .first_tick,
            "non-ticking guard unchanged"
        );
        for slot in 0..6 {
            let s = stack(&game, all, slot);
            assert!(!has_target(s), "source-backed equipment slot {slot}");
            assert_eq!(s.count, 1);
            assert_eq!(
                s.data()
                    .unwrap()
                    .other("minecraft:custom_data")
                    .unwrap()
                    .get("marker"),
                Some(&horizon_core::nbt::Tag::Int(slot as i32))
            );
        }
        let after: Vec<_> = (0..6)
            .map(|slot| {
                game.armor_stands
                    .iter()
                    .find(|a| a.id == mixed)
                    .unwrap()
                    .equipment[slot]
                    .as_ref()
                    .map(horizon_items::item_nbt::item_to_nbt)
            })
            .collect();
        assert_eq!(
            before, after,
            "actual non-compass/untracked/other-dimension/targetless stacks preserved"
        );
        assert!(has_target(stack(&game, inactive, 0)));
        assert_eq!(game.dim, dimension);
        assert_eq!(
            game.world
                .chunks
                .keys()
                .copied()
                .collect::<std::collections::HashSet<_>>(),
            chunks
        );
    }
}

