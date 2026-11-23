//! The local player's hand: clicks and held attack through
//! `myriade_engine::first_person` (`Minecraft.startAttack` /
//! `continueAttack`, `MultiPlayerGameMode` block breaking), the arm swing
//! (`LivingEntity.SwingState`) and the first-person equip heights
//! (`FirstPersonHandsAndItems`) with the item swap ticker they read.
use super::Game;
use myriade_engine::{attack_strength::HeldStack, items::ItemStack};
use mod_api::client::first_person::Hand;
use crate::hand_use::SwingAnimation;

use horizon_core::geometry::{Aabb, Vec3};

/// The point a `ServerboundInteractPacket` carries: `EntityHitResult.getLocation()
/// - entity.position()`, measured from the entity's feet, as
/// `MultiPlayerGameMode.interact` computes it.
pub(super) fn interact_location(point: Vec3, feet: Vec3) -> [f64; 3] {
    [point.x - feet.x, point.y - feet.y, point.z - feet.z]
}

/// `AttackRange.effectiveMinRange`, `effectiveMaxRange` (for a player) and
/// `hitboxMargin` of an `attack_range` component.
fn attack_reach(range: &serde_json::Value, creative: bool) -> (f32, f32, f32) {
    let value = |key: &str, default: f32| range[key].as_f64().map_or(default, |v| v as f32);
    let (min, max) = if creative {
        (
            value("min_creative_reach", 0.0),
            value("max_creative_reach", 5.0),
        )
    } else {
        (value("min_reach", 0.0), value("max_reach", 3.0))
    };
    (min, max, value("hitbox_margin", 0.3))
}

fn v3(v: [f64; 3]) -> Vec3 {
    Vec3::new(v[0], v[1], v[2])
}

/// `AttackRange.isInRange`: float bounds, then inclusive double comparison.
fn attack_range_contains(range: &serde_json::Value, distance: f64, creative: bool) -> bool {
    let (min, max, margin) = attack_reach(range, creative);
    in_attack_range(distance, (min, max, margin), 0.0)
}

/// `AttackRange.isInRange(attacker, box, buffer)` with the reach of
/// `attack_reach` (or `AttackRange.defaultFor`'s, from the entity interaction
/// range): the eye's distance to the box, inclusive.
fn in_attack_range(distance: f64, (min, max, margin): (f32, f32, f32), buffer: f64) -> bool {
    mod_api::ItemAttackRange {
        min,
        max,
        hitbox_margin: margin,
    }
    .contains(distance, buffer)
}

impl Game<'_> {
    /// `getAttributeValue` of an interaction range of the player in place (a
    /// guest swapped in on a server): creative's modifiers are on it
    /// (`update_player_attributes`), as are `/attribute`'s and the items'.
    pub(super) fn interaction_range(
        &mut self,
        attribute: &str,
        survival: f64,
        creative: f64,
    ) -> f64 {
        let otherwise = if self.creative { creative } else { survival };
        self.player
            .as_mut()
            .and_then(|p| p.attribute(attribute).ok())
            .unwrap_or(otherwise)
    }

    /// `Player.entityInteractionRange`/`blockInteractionRange` for the tick's picks:
    /// the attribute carries creative's and a server's modifiers.
    pub(super) fn update_interaction_ranges(&mut self) {
        self.entity_reach = self.interaction_range("minecraft:entity_interaction_range", 3.0, 5.0);
        self.block_reach = self.interaction_range("minecraft:block_interaction_range", 4.5, 5.0);
    }

    /// A stack's `minecraft:attack_range` component, falling back to the spear's
    /// default reach (`spear::REACH`) for spear items unless explicitly removed.
    fn attack_range_component(&self, stack: &ItemStack) -> Option<serde_json::Value> {
        if stack
            .data()
            .is_some_and(|d| d.other("!minecraft:attack_range").is_some())
        {
            return None;
        }
        myriade_engine::spear::Kinetic::of(&stack.item).map(|_| {
            serde_json::json!({
                "min_reach": myriade_engine::spear::REACH.min,
                "max_reach": myriade_engine::spear::REACH.max,
                "min_creative_reach": myriade_engine::spear::REACH.min_creative,
                "max_creative_reach": myriade_engine::spear::REACH.max_creative,
                "hitbox_margin": myriade_engine::spear::REACH.hitbox_margin,
            })
        })
    }

    /// ServerGamePacketListenerImpl.handleAttack's
    /// `Player.isWithinAttackRange(mainHandItem, box, 3.0)` for the player in
    /// place: the held item's `attack_range`, else `AttackRange.defaultFor`
    /// (0 to the entity interaction range, no margin). Out of range, vanilla
    /// drops the attack without a word.
    pub(super) fn within_attack_range(&mut self, target: &Aabb) -> bool {
        let Some(eye) = self.eye_position() else {
            return false;
        };
        let stack = self.held().cloned();
        let default = self.interaction_range("minecraft:entity_interaction_range", 3.0, 5.0);
        let reach = self.stack_attack_range(stack.as_ref(), true, self.creative, default);
        let distance = target.distance_squared_to_point(v3(eye)).sqrt();
        reach.contains(distance, 3.0)
    }

    /// The same effective component view for a detached stack and normal held-stack attacks.
    pub(super) fn stack_attack_range(
        &self,
        stack: Option<&ItemStack>,
        player: bool,
        creative: bool,
        default: f64,
    ) -> mod_api::ItemAttackRange {
        let range = stack.and_then(|stack| self.attack_range_component(stack));
        let (mut min, mut max, hitbox_margin) = range
            .as_ref()
            .map_or((0.0, default as f32, 0.0), |r| attack_reach(r, creative));
        if !player && let Some(range) = &range {
            let factor = range["mob_factor"].as_f64().unwrap_or(1.0) as f32;
            min *= factor;
            max *= factor;
        }
        mod_api::ItemAttackRange {
            min,
            max,
            hitbox_margin,
        }
    }

    /// `Player.isWithinEntityInteractionRange(box, buffer)` for the player in
    /// place (ServerGamePacketListenerImpl.handleInteract, with 3).
    pub(super) fn within_entity_interaction_range(&mut self, target: &Aabb, buffer: f64) -> bool {
        let Some(eye) = self.eye_position() else {
            return false;
        };
        let range = self.interaction_range("minecraft:entity_interaction_range", 3.0, 5.0) + buffer;
        target.distance_squared_to_point(v3(eye)) < range * range
    }

    /// Opening a screen stops breaking before the next game tick (which may pause).
    pub(super) fn stop_mining(&mut self) {
        self.mining = None;
    }
    /// `LivingEntity.swing` with `getModifiedSwingDuration`: the item's
    /// `on_entity_swing` Part first, which may keep the swing from starting.
    pub(super) fn swing_hand(&mut self, hand: Hand, animation: SwingAnimation) {
        self.try_swing_hand(hand, animation);
    }
    /// Returns false when the interaction policy or item vetoes the swing and its packet.
    pub(super) fn try_swing_hand(&mut self, hand: Hand, _animation: SwingAnimation) -> bool {
        if self.native_entity_swing_event(hand) {
            return false;
        }
        self.swings_started += 1;
        true
    }
    /// `swing(hand)` for the hand that acts, with the default animation.
    pub(super) fn swing_arm(&mut self) {
        self.swing_hand(self.hand, SwingAnimation::DEFAULT);
    }
    /// `swing_arm` for a use whose result is `SwingSource.PREDICTED`: the
    /// player's client swings by itself, so a server keeps it to the others.
    pub(super) fn swing_arm_predicted(&mut self) {
        let before = self.swings_started;
        self.swing_arm();
        if self.swings_started != before {
            self.predicted_swings += 1;
        }
    }
    /// `Player.resetAttackStrengthTicker`.
    pub(super) fn reset_attack_strength(&mut self) {
        self.attack_ticks = 0;
        self.hand_strength.reset();
    }
    /// `Minecraft.startAttack` on an entity: a main-hand item with an
    /// `attack_range` attacks only a hit inside it.
    pub(super) fn held_range_allows(&self, distance: f64) -> bool {
        let range = self
            .held()
            .and_then(|stack| self.attack_range_component(stack));
        range.is_none_or(|r| attack_range_contains(&r, distance, self.creative))
    }
    /// Whether the player's tool harvests the block `state` at `pos`, once vanilla's rule
    /// answered `harvest`: the block's parts (`can_harvest_block`, NeoForge's
    /// `IBlockExtension.canHarvestBlock`) answer first, then the global `player_harvest_check`
    /// (`PlayerEvent.HarvestCheck`) hears the result.
    pub(in crate::game) fn mod_harvest_check(
        &mut self,
        pos: [i32; 3],
        state: horizon_world::registry::BlockStateId,
        harvest: &mut bool,
    ) {
        let player = self.own_entity_id() as u32;
        self.native_can_harvest_block(player, pos, state, harvest);
    }
    /// `BlockBehaviour.getDestroyProgress` with `Player.getDestroySpeed`, whose
    /// end asks the mods' `native_player_break_speed`. Every dig's progress comes from
    /// here: single player's, a host's for each guest, a remote client's own.
    pub(in crate::game) fn destroy_progress(&mut self, pos: [i32; 3]) -> f32 {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let facts = self.res.facts().get(state);
        if facts.hardness == -1.0 {
            return 0.0;
        }
        let mut harvest =
            !facts.requires_tool || self.items().correct_for_drops(self.held(), &facts.name);
        let hardness = facts.hardness;
        self.mod_harvest_check(pos, state, &mut harvest);
        let block_break_speed = self.block_break_speed();
        let facts = self.res.facts().get(state);
        let tool_speed = self.items().mining_speed(self.held(), &facts.name);
        let player = horizon_items::mining::DestroySpeedModifiers {
            // Vanilla reads the attribute (recomputing a dirty one) only then.
            mining_efficiency: if tool_speed > 1.0 {
                self.mining_efficiency()
            } else {
                0.0
            },
            effects: self.potions.effects.mining_speed_factor(),
            block_break_speed,
            submerged_mining_speed: self
                .mining_eyes_in_water()
                .then(|| self.submerged_mining_speed()),
            on_ground: self.player.as_ref().is_none_or(|p| p.ground.on_ground),
        };
        let mut speed = horizon_items::mining::destroy_speed(tool_speed, &player);
        let mut canceled = false;
        if canceled {
            return 0.0;
        }
        horizon_items::mining::destroy_progress(hardness, harvest, speed)
    }
    /// `EntityFluidInteraction`: compare the eyes with the camera fluid surface.
    fn mining_eyes_in_water(&self) -> bool {
        use myriade_engine::blocks::Fluid;
        let Some(eye) = self.eye_position() else {
            return false;
        };
        let [x, y, z] = eye.map(|v| v.floor() as i32);
        let facts = self.res.facts();
        let inside = |y| {
            let fluid = facts.get(self.world.block(x, y, z));
            if fluid.fluid != Fluid::Water {
                return false;
            }
            let above = facts.get(self.world.block(x, y + 1, z));
            let source = fluid.fluid_level == 8 && !fluid.fluid_falling;
            let height = if above.fluid == fluid.fluid || (source && above.sturdy_full & 1 != 0) {
                1.0
            } else {
                f64::from(f32::from(fluid.fluid_level) / 9.0)
            };
            eye[1] <= f64::from(y) + height
        };
        // The scan also includes a capped source below eyes exactly on a cell boundary.
        inside(y) || (eye[1] == f64::from(y) && inside(y - 1))
    }
    /// The main-hand stack a break begins with (`destroyingItem`).
    pub(super) fn main_hand_key(&self) -> Option<ItemStack> {
        self.held().cloned()
    }
    /// `MultiPlayerGameMode.sameDestroyTarget`'s item half for the stack
    /// `began` a break began with: `ItemStack.isSameItemSameComponents` with
    /// the main hand now (count and damage left out, as vanilla's kept
    /// reference sees an in-place change), which the parts of `began`'s item
    /// may overturn (`IItemExtension.shouldCauseBlockBreakReset`).
    pub(super) fn same_destroying_item(&self, began: &Option<ItemStack>) -> bool {
        let held = self.held();
        let same = match (began, held) {
            (None, None) => true,
            (Some(a), Some(b)) => a.item == b.item && a.extra == b.extra,
            _ => false,
        };
        same
    }
    /// Whether an attack would hit an entity rather than the block behind
    /// it: the crosshair's `EntityHitResult`.
    pub(super) fn entity_targeted(&self) -> bool {
        let before = |d: Option<f64>| d.is_some_and(|d| self.before_block(d));
        before(self.aimed_painting().map(|h| h.1))
            || before(self.aimed_frame().map(|h| h.1))
            || before(self.armor_stand_hit().map(|h| h.1))
            || before(self.cushion_hit().map(|h| h.1))
            || before(self.boat_hit().map(|h| h.1))
            || before(self.cart_hit().map(|h| h.1))
            || before(self.interaction_hit().map(|h| h.1))
            || before(self.dragon_hit().map(|h| h.1))
            || before(
                self.mob_hit_within(self.block_reach.max(self.entity_reach))
                    .map(|h| h.1),
            )
            || before(self.redirectable_hit().map(|h| h.1))
    }
    /// `LocalPlayer.isHandsBusy`: paddling a boat this player controls
    /// (`getControlledVehicle`: its first passenger, on a server's client).
    pub(super) fn hands_busy(&self) -> bool {
        self.riding
            .is_some_and(|id| self.ecs.boat_status(id).is_some())
    }
    /// `LocalPlayer.tick`: the swing (`LivingEntity.baseTick`), the attack
    /// strength and item swap tickers (`Player.tick`).
    pub(super) fn tick_hand(&mut self) {
        let main = self.inventory[self.selected].clone();
        // Player.tick: a different item resets the attack strength; any change
        // of the stack (`!ItemStack.matches`, e.g. another Efficiency level on
        // the same tool) is what detectEquipmentUpdates gives the player its
        // modifiers for.
        let held = main.as_ref().map(|s| HeldStack {
            item: s.item.to_string(),
            rest: format!("{} {} {:?}", s.count, s.damage, s.extra),
        });
        let changed = self.hand_strength.main_hand_changed(&held);
        if self.hand_strength.tick(held) {
            self.attack_ticks = 0;
        }
        if changed {
            self.apply_equipment();
        }
        self.attack_item = main.as_ref().map(|s| s.item.clone());
    }

    /// `Attributes.ATTACK_SPEED` with the main-hand item's modifier and the
    /// Haste and Mining Fatigue modifiers (`ADD_MULTIPLIED_TOTAL`), clamped to
    /// the attribute's range.
    pub(super) fn main_hand_attack_speed(&self) -> f64 {
        if let Some(speed) = self.player.as_ref().and_then(|p| {
            p.attributes
                .computed_value_of("minecraft:attack_speed")
                .ok()
        }) && (false
            || !self.applied_modifiers.is_empty()
            || !self.potions.applied.is_empty())
        {
            // The computed value is already inside the attribute's bounds.
            return speed;
        }
        // The base comes from the player's attribute map so `/attribute ... base set` applies.
        let base = self
            .player
            .as_ref()
            .and_then(|player| {
                player
                    .attributes
                    .computed_value_of("minecraft:attack_speed")
                    .ok()
            })
            .unwrap_or(myriade_engine::attack_strength::BASE_ATTACK_SPEED);
        let value = base
            + self
                .attack_item_info()
                .map_or(0.0, |i| f64::from(i.attack_speed));
        let value = self
            .potions
            .effects
            .modifiers()
            .into_iter()
            .filter(|m| m.0 == "minecraft:attack_speed")
            .fold(value, |v, m| v * (1.0 + m.2));
        self.sanitize_attack_speed(value)
    }
    /// `Attributes.ATTACK_SPEED`'s sanitizer, from the attribute catalog: the
    /// attack speed a value computed outside the attribute map takes.
    pub(in crate::game) fn sanitize_attack_speed(&self, value: f64) -> f64 {
        self.motion
            .attributes
            .definition("minecraft:attack_speed")
            .map_or(value, |definition| definition.sanitize(value))
    }
    /// The item whose attack damage and speed modifiers the player has (`attack_item`).
    pub(super) fn attack_item_info(&self) -> Option<&myriade_engine::items::ItemInfo> {
        self.items().get(self.attack_item.as_deref()?)
    }

    /// `Player.attack`: the attack damage attribute, with the held weapon's
    /// and the effects' modifiers on it.
    pub(super) fn attack_damage(&mut self) -> f32 {
        self.player
            .as_mut()
            .and_then(|p| p.attribute("minecraft:attack_damage").ok())
            .unwrap_or(1.0) as f32
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_takes_attacks_within_vanilla_reach_only() {
        // handleAttack: AttackRange.defaultFor (0 to the entity interaction
        // range, no margin) plus 3, inclusive: 6 in survival, 8 in creative.
        let survival = (0.0, 3.0, 0.0);
        assert!(in_attack_range(6.0, survival, 3.0));
        assert!(!in_attack_range(6.01, survival, 3.0));
        assert!(!in_attack_range(50.0, survival, 3.0));
        let creative = (0.0, 5.0, 0.0);
        assert!(in_attack_range(8.0, creative, 3.0));
        assert!(!in_attack_range(8.01, creative, 3.0));
        // A spear's own range, with its margin, from the server's side too.
        let spear = serde_json::json!({"min_reach": 2.0, "max_reach": 4.5, "hitbox_margin": 0.125});
        let reach = attack_reach(&spear, false);
        assert!(in_attack_range(7.625, reach, 3.0));
        assert!(!in_attack_range(7.7, reach, 3.0));
    }

    #[test]
    fn spear_default_reach_and_bounds() {
        // #14092: A spear's default reach matches REACH (min 2.0, max 4.5, creative 6.5, margin 0.125).
        let r = &myriade_engine::spear::REACH;
        let spear = serde_json::json!({
            "min_reach": r.min, "max_reach": r.max,
            "min_creative_reach": r.min_creative, "max_creative_reach": r.max_creative,
            "hitbox_margin": r.hitbox_margin,
        });
        let survival = attack_reach(&spear, false);
        assert_eq!(survival, (2.0, 4.5, 0.125));
        let creative = attack_reach(&spear, true);
        assert_eq!(creative, (2.0, 6.5, 0.125));
        // Serverbound handleAttack allows reach plus 3.0 buffer:
        // Survival: 4.5 + 0.125 + 3.0 = 7.625
        assert!(in_attack_range(7.625, survival, 3.0));
        assert!(!in_attack_range(7.626, survival, 3.0));
        // Minimum reach boundary: 2.0 - 0.125 = 1.875
        assert!(attack_range_contains(&spear, 1.875, false));
        assert!(!attack_range_contains(&spear, 1.874, false));
        assert!(attack_range_contains(&spear, 4.625, false));
        assert!(!attack_range_contains(&spear, 4.626, false));
    }

    #[test]
    fn attack_range_uses_inclusive_float_bounds() {
        let range = serde_json::json!({
            "min_reach": 2.0, "max_reach": 4.5,
            "min_creative_reach": 2.0, "max_creative_reach": 6.5,
            "hitbox_margin": 0.125,
        });
        for (creative, min, max) in [(false, 1.875, 4.625), (true, 1.875, 6.625)] {
            for distance in [min, max] {
                assert!(attack_range_contains(&range, distance, creative));
            }
            for distance in [min - 0.0001, max + 0.0001] {
                assert!(!attack_range_contains(&range, distance, creative));
            }
        }
        let defaults = serde_json::json!({});
        for (creative, reach) in [(false, 3.0f32), (true, 5.0f32)] {
            let max = f64::from(reach + 0.3f32);
            assert!(attack_range_contains(&defaults, max, creative));
            assert!(!attack_range_contains(&defaults, max + 1e-8, creative));
        }
    }
}
