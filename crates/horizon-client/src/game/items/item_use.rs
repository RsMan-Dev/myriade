//! `LivingEntity.startUsingItem` and the use over time of items vanilla's
//! `Item` subclasses don't know: what `Item.getUseDuration` answers, a
//! native mod's own item counted down to its `finishUsingItem`, and the hand
//! pose of a `getUseAnimation` (`native_use_duration`, `native_use_animation`,
//! `native_living_entity_use_item_start`, `native_living_entity_use_item_finish`),
//! and each tick and the release of a use (`native_living_entity_use_item_tick`,
//! `native_on_use_tick`, `native_living_entity_use_item_stop`, `native_release_using`).
use super::{Game, shield::OFFHAND_SLOT};
use horizon_client::{items::{ItemInfo, ItemStack}, mobs::spear_use};
use mod_api::client::first_person::Hand;
use horizon_core::use_animation::UseAnimation;

/// Resolve USE_EFFECTS through the stack's component patch.
pub(super) fn interact_vibrations(stack: &ItemStack, default: Option<bool>) -> bool {
    if stack.count == 0 {
        return false;
    }
    let Some(data) = stack.data() else {
        return default.unwrap_or(false);
    };
    if data.other("!minecraft:use_effects").is_some() {
        return false;
    }
    data.other("minecraft:use_effects").map_or_else(
        || default.unwrap_or(false),
        |tag| horizon_client::save::number(tag.get("interact_vibrations")).unwrap_or(1) != 0,
    )
}

impl Game<'_> {
    /// `LivingEntity.startUsingItem` for the acting player's hand: nothing
    /// when a use runs already or the hand is empty. The use lasts
    /// `getUseDuration` ticks, which mods may change or refuse
    /// (`LivingEntityUseItemEvent.Start`), on both sides as in vanilla. True
    /// when the use started.
    pub(super) fn start_using_item(&mut self) -> bool {
        let Some(stack) = self.held().cloned().filter(|_| self.using.is_none()) else {
            return false;
        };
        let own = self.use_duration(&stack);
        let (mut duration, mut canceled) = (own, false);
        let hand = self.hand;
        self.native_living_entity_use_item_start(&stack, None, hand, &mut duration, &mut canceled);
        let Some(ticks_set) = start_of(own, duration, canceled) else {
            return false;
        };
        self.using = Some((self.held_slot(), 0));
        self.use_duration = duration;
        self.use_start = super::using_item::UseStart {
            ticks_set,
            item: Some(stack.clone()),
        };
        // `LivingEntity.startUsingItem`: `causeUseVibration(ITEM_INTERACT_START)`
        // when the use effects of the stack ask for interaction vibrations.
        self.use_vibration(&stack, "minecraft:item_interact_start");
        // A new charge hears the crossbow's loading sounds again
        // (`game/items/crossbow.rs`).
        self.crossbow_sounds = [false; 2];
        true
    }
    /// `ItemStack.getUseDuration`: a consumable's time, 72000 for what is
    /// held until let go, the fixed times of brushes, bundles, spyglasses and
    /// goat horns, 0 for the rest, then a mod's answer. A `consumable` or
    /// `blocks_attacks` the server's patch adds counts as the item's own
    /// (`Item.getUseDuration` reads the stack's components).
    pub(super) fn use_duration(&self, stack: &ItemStack) -> i32 {
        let info = self.items().get(&stack.item);
        let (patched_ticks, patched_blocks) = horizon_client::item_sync::patched_use(stack);
        let mut duration = match &*stack.item {
            "minecraft:bow" | "minecraft:crossbow" | "minecraft:trident" => 72_000,
            "minecraft:spyglass" => 1200,
            "minecraft:brush" => 200,
            _ if info.is_some_and(|i| i.bundle) => 200,
            "minecraft:ender_eye" => 0,
            "minecraft:goat_horn" => self.native_item_instrument_duration(stack),
            _ => {
                if let Some(ticks) = patched_ticks {
                    ticks
                } else if let Some(consumable) = self.consumable(stack) {
                    consumable.ticks()
                } else if patched_blocks
                    || self.stack_carries_blocks_attacks(stack, info)
                    || stack.carries_component(info, "minecraft:kinetic_weapon")
                {
                    72_000
                } else {
                    0
                }
            }
        };
        self.native_use_duration(stack, &mut duration);
        duration
    }
    /// Whether `stack` is a mod's item that only a mod uses over time: no
    /// `consumable`, `blocks_attacks` or kinetic weapon of vanilla's own, and
    /// a `native_use_duration` above 0. Its use is its own `Item.use`.
    pub(super) fn mod_use(&self, stack: &ItemStack) -> bool {
        let info = self.items().get(&stack.item);
        !stack.item.starts_with("minecraft:")
            && info.is_some()
            && self.consumable(stack).is_none()
            && !self.stack_carries_blocks_attacks(stack, info)
            && !stack.carries_component(info, "minecraft:kinetic_weapon")
            && self.use_duration(stack) > 0
    }
    /// A right click with a `mod_use` item: `startUsingItem` (CONSUME).
    pub(super) fn start_mod_use(&mut self, stack: &ItemStack) -> bool {
        if !self.mod_use(stack) {
            return false;
        }
        self.start_using_item();
        true
    }
    /// `LivingEntity.updateUsingItem` for a `mod_use` item used `ticks` so far:
    /// `completeUsingItem` once its duration ran out.
    pub(super) fn tick_mod_use(&mut self, slot: usize, ticks: u32, stack: &ItemStack) {
        if !self.item_on_use_tick(stack, ticks) {
            return;
        }
        if let Some(ticks) = count_down(ticks, self.use_duration) {
            self.using = Some((slot, ticks));
            return;
        }
        let Some(info) = self.items().get(&stack.item).cloned() else {
            self.using = None;
            return;
        };
        self.complete_using_item(slot, stack, &info);
    }
    /// `ServerPlayer.completeUsingItem` of `stack` in `slot`: the client is
    /// told, `ItemStack.finishUsingItem` (a consumable is eaten, a potion
    /// drunk, a spyglass lowered, anything else given back) with its use
    /// cooldown, then `LivingEntityUseItemEvent.Finish`.
    pub(super) fn complete_using_item(&mut self, slot: usize, stack: &ItemStack, info: &ItemInfo) {
        // a potion is drunk (no last tick of its own: a mod's tick ran it out).
        if self.finish_drinking(slot, stack) {
            self.use_item_finished(stack);
            self.use_vibration(stack, "minecraft:item_interact_finish");
            return;
        }
        self.using = None;
        self.send_use_item_complete();
        self.apply_use_cooldown(stack);
        if let Some(consumable) = self.consumable(stack) {
            self.finish_consuming(info, &consumable, stack);
        }
        self.use_item_finished(stack);
        // `LivingEntity.completeUsingItem` ends with `stopUsingItem`.
        self.use_vibration(stack, "minecraft:item_interact_finish");
    }
    /// `LivingEntityUseItemEvent.Finish` after `finishUsingItem` of `stack`
    /// (no ticks left): the hand holds what a mod made the result.
    pub(super) fn use_item_finished(&mut self, stack: &ItemStack) {
        let mut result = self.held().cloned();
        let before = result.clone();
        let hand = self.hand;
        self.native_living_entity_use_item_finish(stack, None, hand, 0, &mut result);
        if result != before {
            *self.held_mut() = result.filter(|s| s.count > 0);
        }
    }
    /// What `Item.finishUsingItem` leaves in the hand on a server's client: a
    /// stack that is not a consumable stays as it is (a spyglass); a
    /// consumable loses one (`ItemStack.consume`), and once that empties the
    /// hand, its `use_remainder` takes the hand
    /// (`UseRemainder.convertIntoRemainder`). Neither in creative mode. A
    /// remainder of a stack that is not emptied goes to the inventory on the
    /// server and arrives with its inventory update.
    /// `LivingEntityUseItemEvent.Tick` for `stack` in use `ticks` so far, as
    /// `updatingUsingItem` starts: a mod may change the ticks left, which
    /// moves the end. False when it ran the use out (`tick_ran_out`): the
    /// item's tick and countdown are skipped.
    pub(super) fn use_item_ticked(&mut self, stack: &ItemStack, ticks: u32) -> bool {
        let before = ticks_left(self.use_duration, ticks);
        let (mut remaining, mut canceled) = (before, false);
        let hand = self.used_hand().unwrap_or(self.hand);
        self.native_living_entity_use_item_tick(stack, None, hand, &mut remaining, &mut canceled);
        let (left, set) = tick_left(before, remaining, canceled, self.use_start.ticks_set);
        self.use_start.ticks_set = set;
        self.use_duration = duration_leaving(ticks, left);
        left > 0 || !set
    }
    /// A mod's `Item.onUseTick` of `stack` used `ticks`, while ticks are left.
    /// False when that mod released the use, which then has nothing left to run.
    pub(super) fn item_on_use_tick(&mut self, stack: &ItemStack, ticks: u32) -> bool {
        let using = self.using;
        let remaining = ticks_left(self.use_duration, ticks);
        if remaining > 0 {
            self.native_on_use_tick(stack, None, remaining);
        }
        self.using == using
    }
    /// How long `stack`, used `ticks`, counts as held: `getUseDuration` less
    /// the ticks left (`BowItem.releaseUsing`, `CrossbowItem.onUseTick`,
    /// `TridentItem.releaseUsing`), `ticks` unless a mod's Start or Tick
    /// moved the ticks left.
    pub(super) fn ticks_held(&self, stack: &ItemStack, ticks: u32) -> u32 {
        held(
            self.use_duration(stack),
            ticks_left(self.use_duration, ticks),
        )
    }
    /// `LivingEntityUseItemEvent.Stop` as `releaseUsingItem` lets go of
    /// `stack` used `ticks`: false when a mod skips its `releaseUsing`.
    pub(super) fn use_item_stopped(&mut self, stack: &ItemStack, ticks: u32) -> bool {
        let mut canceled = false;
        let remaining = ticks_left(self.use_duration, ticks);
        let hand = self.hand;
        self.native_living_entity_use_item_stop(stack, None, hand, remaining, &mut canceled);
        !canceled
    }
    /// A mod's `Item.releaseUsing` of `stack` used `ticks`: when it says it
    /// was used, `ItemStack.releaseUsing` applies the use cooldown.
    pub(super) fn mod_release_using(&mut self, stack: &ItemStack, ticks: u32) {
        let mut used = false;
        let remaining = ticks_left(self.use_duration, ticks);
        self.native_release_using(stack, None, remaining, &mut used);
        if used {
            self.apply_use_cooldown(stack);
        }
    }
    /// Whether a use in `slot` lost its item: switching hotbar slots only ends
    /// a main-hand use.
    pub(super) fn use_hand_changed(&self, slot: usize) -> bool {
        slot != self.selected && slot != OFFHAND_SLOT
    }
    /// `LivingEntity.releaseUsingItem` of the acting player, the hand of the
    /// item in use acting: the use is over before any hook runs, so a mod
    /// that releases again from one finds nothing. False when none runs.
    pub(super) fn release_using_item(&mut self) -> bool {
        let Some(hand) = self.used_hand() else {
            return false;
        };
        self.with_hand(hand, |game| {
            let Some((slot, ticks)) = game.using.take() else {
                return false;
            };
            let Some(stack) = game.stack_in(slot).cloned() else {
                game.use_start.item = None;
                return true;
            };
            // `releaseUsing` only for the same item; `stopUsingItem` always,
            // from the stack the use began with.
            let (stack, hand_changed) = match game.use_start.item.take() {
                Some(item) if item.item != stack.item => (item, true),
                _ => (stack, game.use_hand_changed(slot)),
            };
            let player = (!mod_api::__private::mods().is_empty()).then(|| {
                let name = game.acting_player_name().to_owned();
                (game.native_acting_id(), name)
            });
            if let Some((uuid, name)) = &player
                && !hand_changed
                && &*stack.item == "minecraft:bow"
                && let Some(arrow) = game
                    .projectile_slot("minecraft:bow")
                    .and_then(|slot| game.stack_in(slot).map(|s| s.item.clone()))
            {
                game.native_player_ready_arrow(*uuid, name, &arrow);
            }
            game.release_used_item(&stack, ticks, hand_changed);
            if let Some((uuid, name)) = &player {
                game.native_player_stop_using_item(*uuid, name);
            }
            true
        })
    }
    /// The stack in the hand of mob `id`.
    fn mob_hand_stack(&self, id: u32, off_hand: bool) -> Option<ItemStack> {
        use horizon_items::equipment::{MAINHAND, OFFHAND};
        let slot = if off_hand { OFFHAND } else { MAINHAND };
        let mob = self.remote_entities.iter().find(|m| m.id == id)?;
        mob.extra.slot_stack(slot).cloned()
    }
    /// `ItemStack.causeUseVibration` for `stack` used by mob `id`.
    fn mob_use_vibration(&mut self, id: u32, stack: &ItemStack, event: &'static str) {
        if !self.use_vibrates(stack) {
            return;
        }
        let Some(p) = self.remote_entities.iter().find(|m| m.id == id).map(|m| m.position) else {
            return;
        };
        let ctx = self.mob_context(id, None);
        self.post_game_event(event, [p.x, p.y, p.z], ctx);
    }
    /// `LivingEntity.startUsingItem` of mob `id`, `native_living_entity_use_item_start`
    /// as for a player. True when the use started. A mob's use is its
    /// `using_item` counter: its goals run that, and nothing counts a use down
    /// to `finishUsingItem` for it.
    pub(super) fn start_mob_using_item(&mut self, id: u32, hand: Hand) -> bool {
        let off_hand = hand == Hand::Off;
        let free = |m: &horizon_client::mobs::Mob| {
            !spear_use::is_using_item(m) && !spear_use::counts_own_timer(m)
        };
        if !self.remote_entities.iter().any(|m| m.id == id && free(m)) {
            return false;
        }
        let Some(stack) = self.mob_hand_stack(id, off_hand) else {
            return false;
        };
        let own = self.use_duration(&stack);
        let (mut duration, mut canceled) = (own, false);
        let entity = Some(id as i32);
        self.native_living_entity_use_item_start(
            &stack,
            entity,
            hand,
            &mut duration,
            &mut canceled,
        );
        if start_of(own, duration, canceled).is_none() {
            return false;
        }
        // The hook may have changed the mob: the same checks again.
        let started = self
            .remote_entities
            .iter_mut()
            .find(|m| m.id == id)
            .is_some_and(|m| free(m) && spear_use::start_using_item(m, off_hand));
        if started {
            self.mob_use_vibration(id, &stack, "minecraft:item_interact_start");
        }
        started
    }
    /// `LivingEntity.getUseItemRemainingTicks` of mob `id`: the use's duration
    /// less the ticks its counter has run, 0 when it uses nothing (an admiring
    /// piglin and a drinking witch use none). None for a mob that is gone.
    pub(super) fn mob_item_use_remaining(&self, id: u32) -> Option<i32> {
        let mob = self.remote_entities.iter().find(|m| m.id == id)?;
        if !spear_use::is_using_item(mob) {
            return Some(0);
        }
        let stack = self.mob_hand_stack(id, mob.extra.using_off_hand);
        Some(stack.map_or(0, |stack| {
            ticks_left(
                self.use_duration(&stack),
                mob.extra.using_item.unsigned_abs() - 1,
            )
        }))
    }
    /// `LivingEntity.useItemRemaining` of mob `id` set to `remaining`: its use
    /// counter moves to where that many ticks are left. A mob's use never
    /// runs out, so nothing else changes. False when it uses nothing.
    pub(super) fn mob_set_item_use_remaining(&mut self, id: u32, remaining: i32) -> bool {
        let Some(mob) = self.remote_entities.iter().find(|m| m.id == id) else {
            return false;
        };
        if !spear_use::is_using_item(mob) {
            return false;
        }
        let Some(stack) = self.mob_hand_stack(id, mob.extra.using_off_hand) else {
            return false;
        };
        let counter = use_counter_leaving(self.use_duration(&stack), remaining);
        if let Some(mob) = self.remote_entities.iter_mut().find(|m| m.id == id) {
            mob.extra.using_item = counter;
        }
        true
    }
    /// `LivingEntity.releaseUsingItem` of mob `id`: a mod's `Stop` and
    /// `Item.releaseUsing`, the use over before them as for a player. A bow's
    /// and a trident's own release ask for a player, and a use cooldown too
    /// (`UseCooldown.apply`). False when no use runs.
    pub(super) fn release_mob_using_item(&mut self, id: u32) -> bool {
        let Some(mob) = self.remote_entities.iter_mut().find(|m| m.id == id) else {
            return false;
        };
        if !spear_use::is_using_item(mob) {
            return false;
        }
        let off_hand = mob.extra.using_off_hand;
        let ticks = mob.extra.using_item.unsigned_abs() - 1;
        spear_use::stop_using_item(mob);
        let Some(stack) = self.mob_hand_stack(id, off_hand) else {
            return true;
        };
        let remaining = ticks_left(self.use_duration(&stack), ticks);
        let hand = if off_hand { Hand::Off } else { Hand::Main };
        let entity = Some(id as i32);
        let mut canceled = false;
        self.native_living_entity_use_item_stop(&stack, entity, hand, remaining, &mut canceled);
        if !canceled {
            self.native_release_using(&stack, entity, remaining, &mut false);
        }
        self.mob_use_vibration(id, &stack, "minecraft:item_interact_finish");
        true
    }
    /// `mob_hand_stack` for the ECS mob `id`: the hand stack the bridge holds (`ecs_hand_stack`, not yet on the bridge).
    fn ecs_mob_hand_stack(&self, id: u32, off_hand: bool) -> Option<ItemStack> {
        self.ecs_hand_stack(id, off_hand)
    }
    /// `mob_use_vibration` for the ECS mob `id`.
    fn ecs_mob_use_vibration(&mut self, id: u32, stack: &ItemStack, event: &'static str) {
        if !self.use_vibrates(stack) {
            return;
        }
        let Some(p) = self.ecs.position(id) else { return };
        let ctx = self.mob_context(id, None);
        self.post_game_event(event, p, ctx);
    }
    /// `start_mob_using_item` for the ECS mob `id`: the use counter is the bridge's (`ecs_start_using`, not yet on the bridge).
    pub(in crate::game) fn start_mob_using_item_ecs(&mut self, id: u32, hand: Hand) -> bool {
        let off_hand = hand == Hand::Off;
        if !self.ecs_mob_free(id) {
            return false;
        }
        let Some(stack) = self.ecs_mob_hand_stack(id, off_hand) else {
            return false;
        };
        let own = self.use_duration(&stack);
        let (mut duration, mut canceled) = (own, false);
        let entity = Some(id as i32);
        self.native_living_entity_use_item_start(&stack, entity, hand, &mut duration, &mut canceled);
        if start_of(own, duration, canceled).is_none() {
            return false;
        }
        let started = self.ecs_mob_free(id) && self.ecs_start_using(id, off_hand);
        if started {
            self.ecs_mob_use_vibration(id, &stack, "minecraft:item_interact_start");
        }
        started
    }
    /// `mob_item_use_remaining` for the ECS mob `id`.
    pub(super) fn mob_item_use_remaining_ecs(&self, id: u32) -> Option<i32> {
        let (off_hand, counter) = self.ecs_using(id)?;
        if counter == 0 {
            return Some(0);
        }
        let stack = self.ecs_mob_hand_stack(id, off_hand);
        Some(stack.map_or(0, |stack| ticks_left(self.use_duration(&stack), counter.unsigned_abs() - 1)))
    }
    /// `mob_set_item_use_remaining` for the ECS mob `id`.
    pub(super) fn mob_set_item_use_remaining_ecs(&mut self, id: u32, remaining: i32) -> bool {
        let Some((off_hand, counter)) = self.ecs_using(id) else { return false };
        if counter == 0 {
            return false;
        }
        let Some(stack) = self.ecs_mob_hand_stack(id, off_hand) else { return false };
        let counter = use_counter_leaving(self.use_duration(&stack), remaining);
        self.ecs_set_using(id, counter);
        true
    }
    /// `release_mob_using_item` for the ECS mob `id`.
    pub(super) fn release_mob_using_item_ecs(&mut self, id: u32) -> bool {
        let Some((off_hand, counter)) = self.ecs_using(id) else { return false };
        if counter == 0 {
            return false;
        }
        let ticks = counter.unsigned_abs() - 1;
        self.ecs_stop_using(id);
        let Some(stack) = self.ecs_mob_hand_stack(id, off_hand) else {
            return true;
        };
        let remaining = ticks_left(self.use_duration(&stack), ticks);
        let hand = if off_hand { Hand::Off } else { Hand::Main };
        let entity = Some(id as i32);
        let mut canceled = false;
        self.native_living_entity_use_item_stop(&stack, entity, hand, remaining, &mut canceled);
        if !canceled {
            self.native_release_using(&stack, entity, remaining, &mut false);
        }
        self.ecs_mob_use_vibration(id, &stack, "minecraft:item_interact_finish");
        true
    }
    /// `Player.canEat(canAlwaysEat)` for `stack`, food that is eaten only
    /// when hungry, then a mod's answer.
    pub(super) fn can_eat(&self, stack: Option<&ItemStack>, can_always_eat: bool) -> bool {
        let vanilla = can_always_eat
            || self.creative
            || self
                .player
                .as_ref()
                .is_some_and(|p| p.health.food.food < 20);
        let mut answer = None;
        self.native_can_eat(stack, vanilla, &mut answer);
        answer.unwrap_or(vanilla)
    }
}

/// The ticks left after a mod's `Tick`, a cancel leaving -1, and
/// whether a mod ever set this use's: once one did, the use goes on only
/// while some are left, else it ran out. A use no mod touched goes on as
/// vanilla's own do (an eye of ender's has none from the start).
fn tick_left(before: i32, remaining: i32, canceled: bool, set: bool) -> (i32, bool) {
    let left = if canceled { -1 } else { remaining };
    (left, set || canceled || remaining != before)
}

/// What `releaseUsingItem` runs of a release: the item's own and a mod's
/// `releaseUsing` (not when a mod's `Stop` canceled, nor when the hand
/// changed item, which stops the use without one), and the spyglass's
/// sound (not when canceled).
pub(super) fn release_parts(canceled: bool, hand_changed: bool) -> (bool, bool) {
    (!canceled && !hand_changed, !canceled)
}

pub(super) fn bow_release_power(has_ammo: bool, charge: i32, canceled: bool) -> Option<f32> {
    if canceled || charge < 0 || !has_ammo {
        return None;
    }
    let f = charge as f32 / 20.0;
    let power = ((f * f + f * 2.0) / 3.0).min(1.0);
    (power >= 0.1).then_some(power)
}

/// `useItemRemaining` of a use of `duration` ticks, `ticks` old.
pub(super) fn ticks_left(duration: i32, ticks: u32) -> i32 {
    duration.saturating_sub_unsigned(ticks)
}

/// `LivingEntity.getUsedItemHand` of the use `using` (its slot and ticks so
/// far), none when nothing is in use.
pub(super) fn hand_of(using: Option<(usize, u32)>) -> Option<Hand> {
    using.map(|(slot, _)| {
        if slot == OFFHAND_SLOT {
            Hand::Off
        } else {
            Hand::Main
        }
    })
}

/// `LivingEntity.useItemRemaining` of the use `using` (its slot and ticks so
/// far) of `duration` ticks, 0 when nothing is in use.
pub(super) fn remaining_of(using: Option<(usize, u32)>, duration: i32) -> i32 {
    using.map_or(0, |(_, ticks)| ticks_left(duration, ticks))
}

/// Sets `useItemRemaining` of the use `using` by moving its `duration`, as a
/// mod's tick does; false when nothing is in use.
pub(super) fn set_remaining(
    using: Option<(usize, u32)>,
    duration: &mut i32,
    remaining: i32,
) -> bool {
    let Some((_, ticks)) = using else {
        return false;
    };
    *duration = duration_leaving(ticks, remaining);
    true
}

/// `getUseDuration(stack) - remainingTime` of a use of `own` ticks with
/// `remaining` left, none below 0.
fn held(own: i32, remaining: i32) -> u32 {
    u32::try_from(i64::from(own) - i64::from(remaining)).unwrap_or(0)
}

/// The duration that leaves `remaining` ticks of a use `ticks` old, as a
/// mod's `Tick` set them.
pub(super) fn duration_leaving(ticks: u32, remaining: i32) -> i32 {
    remaining.saturating_add_unsigned(ticks)
}

/// The use counter of a mob (1 on the tick its use starts) that leaves
/// `remaining` ticks of a use of `own`, none past the use's whole duration.
fn use_counter_leaving(own: i32, remaining: i32) -> i32 {
    i32::try_from(held(own, remaining).saturating_add(1)).unwrap_or(i32::MAX)
}

/// What a `Start` left of a use of `own` ticks: None refused (a cancel, or
/// a time below 0), else whether a
/// mod set the ticks left to none. Such a use runs out on its first tick
/// (`tick_left`), while
/// an item's own 0 (an eye of ender) is held until let go, as vanilla's.
fn start_of(own: i32, duration: i32, canceled: bool) -> Option<bool> {
    (!canceled && duration >= 0).then_some(duration == 0 && own != 0)
}

/// `Consumable.startConsuming`: a time of 0 or less is no use, the item is
/// consumed at once (`onConsume`).
pub(super) fn consumes_at_once(consume_ticks: i32) -> bool {
    consume_ticks <= 0
}

/// `updateUsingItem`'s `--useItemRemaining == 0` for a use `ticks` old of
/// `duration` ticks: the ticks used after this one, none on the tick it runs
/// out. A time of 0 or less, or one a mod cut below what was used, never
/// does: the use lasts until let go.
pub(super) fn count_down(ticks: u32, duration: i32) -> Option<u32> {
    let ticks = ticks + 1;
    (i64::from(duration) - i64::from(ticks) != 0).then_some(ticks)
}

#[cfg(test)]
mod tests {
    use super::{
        bow_release_power, consumes_at_once, count_down, duration_leaving, held, release_parts,
        start_of, tick_left, ticks_left, use_counter_leaving,
    };
    use horizon_core::use_animation::UseAnimation;

    /// A mob's use counter is 1 on the tick it starts and a bow's use lasts
    /// 72000 ticks: 71995 left is 5 ticks in, counter 6 (what releasing
    /// reports back as 71995), and more than the whole use is no ticks in.
    #[test]
    fn a_mobs_use_counter_leaves_the_remaining_ticks() {
        for (own, remaining, counter) in [
            (72_000, 71_995, 6),
            (72_000, 72_000, 1),
            (72_000, 1, 72_000),
            (72_000, 0, 72_001),
            (32, 40, 1),
            (32, -3, 36),
            (i32::MAX, i32::MIN, i32::MAX),
            // The extremes a mod may set on a 72000 tick use, which
            // `spear_use`'s tick test runs on.
            (72_000, i32::MIN, i32::MAX),
            (72_000, i32::MAX, 1),
            (72_000, -5, 72_006),
            (i32::MIN, i32::MIN, 1),
            (i32::MIN, i32::MAX, 1),
        ] {
            let got = use_counter_leaving(own, remaining);
            assert_eq!(got, counter, "{own} {remaining}");
            if remaining <= own && got < i32::MAX {
                // Read back the way a use reads its ticks left.
                assert_eq!(ticks_left(own, got.unsigned_abs() - 1), remaining);
            }
            // Reading the ticks left back never overflows either.
            ticks_left(own, got.unsigned_abs() - 1);
        }
    }

    #[test]
    fn a_use_runs_out_on_its_last_tick() {
        // a 32 tick food: ticks 0 to 30 go on, the 32nd tick finishes it.
        assert_eq!(count_down(0, 32), Some(1));
        assert_eq!(count_down(30, 32), Some(31));
        assert_eq!(count_down(31, 32), None);
        // a one tick use finishes on its first tick.
        assert_eq!(count_down(0, 1), None);
        // no time, or one below what was used, waits for the release.
        assert_eq!(count_down(0, 0), Some(1));
        assert_eq!(count_down(10, 4), Some(11));
        assert_eq!(count_down(5, 72_000), Some(6));
    }

    #[test]
    fn a_started_use_finishes_after_its_duration() {
        // start at 0 ticks, count down each tick, finish when it runs out.
        for duration in [1, 16, 32, 40] {
            let (mut ticks, mut ticked) = (0, 0);
            while let Some(next) = count_down(ticks, duration) {
                ticks = next;
                ticked += 1;
            }
            assert_eq!(ticked + 1, duration);
        }
    }

    #[test]
    fn a_start_below_0_refuses_the_use_and_a_mods_0_runs_out() {
        // a cancel or a time below 0 refuses it.
        assert_eq!(start_of(32, 32, true), None);
        assert_eq!(start_of(32, -1, false), None);
        assert_eq!(start_of(0, -1, false), None);
        // a mod's 0 starts it, and its first tick runs it out.
        assert_eq!(start_of(32, 0, false), Some(true));
        let (left, set) = tick_left(0, 0, false, true);
        assert!(!(left > 0 || !set));
        // an item's own 0 (an eye of ender) goes on, as a changed time does.
        assert_eq!(start_of(0, 0, false), Some(false));
        let (left, set) = tick_left(0, 0, false, false);
        assert!(left > 0 || !set);
        assert_eq!(start_of(32, 64, false), Some(false));
    }

    #[test]
    fn a_tick_moves_the_end_of_a_use() {
        // a 32 tick use, 10 ticks in: 22 left.
        assert_eq!(ticks_left(32, 10), 22);
        // a mod leaves 1 tick: the next tick finishes it.
        let duration = duration_leaving(10, 1);
        assert_eq!(count_down(10, duration), None);
        // or 30 more: it goes on to tick 40.
        let duration = duration_leaving(10, 30);
        assert_eq!(duration, 40);
        assert_eq!(count_down(38, duration), Some(39));
        assert_eq!(count_down(39, duration), None);
        // left as it was, nothing moves.
        assert_eq!(duration_leaving(10, ticks_left(32, 10)), 32);
    }

    #[test]
    fn a_tick_of_no_time_left_runs_the_use_out() {
        let goes_on = |(left, set): (i32, bool)| left > 0 || !set;
        // untouched, a use goes on, even one of no time (an eye of ender).
        assert_eq!(tick_left(20, 20, false, false), (20, false));
        assert!(goes_on(tick_left(-3, -3, false, false)));
        // a mod's ticks left go on while above 0.
        assert_eq!(tick_left(20, 5, false, false), (5, true));
        assert!(goes_on(tick_left(20, 5, false, false)));
        // a cancel is -1 left, and it or 0 or less set runs the use out.
        assert_eq!(tick_left(20, 20, true, false), (-1, true));
        assert!(!goes_on(tick_left(20, 0, false, false)));
        assert!(!goes_on(tick_left(20, -1, false, false)));
        // a use a mod ran out stays out on the next ticks (a crossbow frozen).
        assert!(!goes_on(tick_left(-1, -1, false, true)));
        // and goes on again if a mod gives it ticks back.
        assert!(goes_on(tick_left(-1, 10, false, true)));
    }

    #[test]
    fn a_release_runs_what_vanilla_runs() {
        // let go: the item's release and the spyglass's sound.
        assert_eq!(release_parts(false, false), (true, true));
        // a mod's Stop canceled: neither.
        assert_eq!(release_parts(true, false), (false, false));
        // another item in the hand: no release (Horizon still lowers a spyglass).
        assert_eq!(release_parts(false, true), (false, true));
    }

    #[test]
    fn bow_release_respects_charge_cancel_and_ammo() {
        assert_eq!(bow_release_power(true, 20, false), Some(1.0));
        assert_eq!(bow_release_power(true, 10, false), Some(5.0 / 12.0));
        assert_eq!(bow_release_power(true, 0, false), None);
        assert_eq!(bow_release_power(true, -1, false), None);
        assert_eq!(bow_release_power(true, 20, true), None);
        assert_eq!(bow_release_power(false, 20, false), None);
    }

    #[test]
    fn a_moved_end_moves_the_charge() {
        // a bow held 10 ticks of 72000: 10, as vanilla's.
        assert_eq!(held(72_000, ticks_left(72_000, 10)), 10);
        // a mod's Tick leaves 71000 at tick 10: it counts as held 1000.
        assert_eq!(
            held(72_000, ticks_left(duration_leaving(10, 71_000), 10)),
            1000
        );
        // more ticks left than the item's own time: not held at all.
        assert_eq!(held(72_000, 80_000), 0);
    }

    #[test]
    fn a_release_tells_how_long_the_item_was_held() {
        // a bow-like item held 25 ticks of 72000: the time left gives the charge back.
        let left = ticks_left(72_000, 25);
        assert_eq!(left, 71_975);
        assert_eq!(72_000 - left, 25);
        // released at once.
        assert_eq!(ticks_left(72_000, 0), 72_000);
    }

    #[test]
    fn a_food_of_no_time_is_eaten_at_once() {
        assert!(consumes_at_once(0));
        assert!(consumes_at_once(-3));
        assert!(!consumes_at_once(1));
        assert!(!consumes_at_once(32));
    }

}

#[cfg(test)]
pub(super) mod interact_vibration_tests {
    use super::*;

    #[test]
    fn stack_use_effects_patch_controls_vibrations() {
        let mut rod = ItemStack::new("minecraft:fishing_rod", 1);
        assert!(interact_vibrations(&rod, Some(true)));
        rod.data_mut().set_loot_component(
            "minecraft:use_effects",
            &Some(
                horizon_core::json_nbt::json_to_tag(
                    &serde_json::json!({"interact_vibrations": false}),
                )
                .unwrap(),
            ),
        );
        assert!(!interact_vibrations(&rod, Some(true)));
        rod.data_mut().set_loot_component(
            "minecraft:use_effects",
            &Some(horizon_core::nbt::Tag::Compound(Default::default())),
        );
        assert!(interact_vibrations(&rod, Some(false)));
        rod.data_mut().set_other("minecraft:use_effects", None);
        rod.data_mut().set_other(
            "!minecraft:use_effects",
            Some(horizon_core::nbt::Tag::Compound(Default::default())),
        );
        assert!(!interact_vibrations(&rod, Some(true)));
    }
}
