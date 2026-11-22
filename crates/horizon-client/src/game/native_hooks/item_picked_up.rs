use super::*;
use horizon_client::entities::ItemEntity;
use horizon_core::random::RandomSource;

impl Game<'_> {
    /// `player` touches the ground item entity `item`
    /// (`ItemEntity.playerTouch`), before its pickup delay and target are
    /// checked: it is asked on every tick the two touch, even while the item
    /// can't be taken. Leave `can_pickup` at `Default` for vanilla's rule,
    /// set `True` to take it despite its delay or another player's target
    /// (a full inventory still leaves it), or `False` to leave it there: the
    /// fork sets a vanilla eligibility delay of at least 10 ticks. The hook
    /// still runs on each touch, and a later `True` overrides that delay. The last mod to set it
    /// decides. The handle reads the whole stack on the ground; for the
    /// window's own player of a world it ticks itself, whose pickup pass
    /// lifts the items out of the level, it reads as unavailable. Server-side.
    pub(in crate::game) fn native_item_picked_up(
        &mut self,
        player: u128,
        item: ItemEntityRef,
        can_pickup: &mut TriState,
    ) {
        self.fire_item_picked_up(player, item, can_pickup);
    }

    /// `player` picked up the whole stack of the ground item entity `item`
    /// (`ItemEntity.playerTouch`, right after `Inventory.add` took all of
    /// it), the entity `item_picked_up` named. `original` is a copy of
    /// the stack as it lay before the pickup: changing it does nothing.
    /// `remaining` is what the item entity still holds, so it starts as
    /// `None` (empty). Set a stack there to leave the entity on the ground
    /// holding it: the pickup then counts only `original`'s count less its
    /// count. Neither says how many were picked up; that is `original`'s
    /// count less `remaining`'s. A mod that removes the item entity meanwhile
    /// keeps it removed: no take animation, the stat and criteria still run.
    /// Not called on a partial pickup, where some of the stack went in and
    /// the rest stayed on the ground: `item_picked_up_partial` hears that one,
    /// and a pickup is never told to both. Nor when `item_picked_up` refused
    /// the pickup or the inventory took nothing; a forced `True` from
    /// `item_picked_up` changes none of this. A creative player always takes
    /// the whole stack. Server-side.
    pub(in crate::game) fn native_item_picked_up_post(
        &mut self,
        player: u128,
        item: ItemEntityRef,
        original: &ItemStack,
        remaining: &mut Option<ItemStack>,
    ) {
        self.fire_item_picked_up_post(player, item, original, remaining);
    }

    /// `player` picked up part of a ground stack (`ItemEntity.playerTouch`,
    /// right after `Inventory.add` took some of it and left the rest).
    /// `original` is a copy of the stack as it lay before the pickup:
    /// changing it does nothing. `remaining` is what the item entity still
    /// holds, so it starts as the part that did not fit. Set another stack
    /// there to leave the entity holding it, or `None` to empty it: the
    /// pickup then counts `original`'s count less what is left. The amount
    /// taken is `original`'s count less `remaining`'s. A mod that removes the
    /// item entity meanwhile keeps it removed: no take animation, the stat
    /// and criteria still run. Of the two notices, this one hears a partial
    /// pickup and `item_picked_up_post` a whole one; a pickup is never told
    /// to both. Not called when `item_picked_up` refused the pickup or the
    /// inventory took nothing. A creative player always takes the whole
    /// stack. Server-side; no NeoForge counterpart.
    pub(in crate::game) fn native_item_picked_up_partial(
        &mut self,
        player: u128,
        original: &ItemStack,
        remaining: &mut Option<ItemStack>,
    ) {
        self.fire_item_picked_up_partial(player, original, remaining);
    }

    /// `Inventory.add` of the ground `stack` the acting player picks up: what
    /// did not fit. A player with infinite materials (creative) takes all of it.
    pub(in crate::game) fn native_pickup_insert(&mut self, stack: &ItemStack) -> Option<ItemStack> {
        let rest = self.add_stack_to_inventory(stack.clone());
        rest.filter(|_| !self.creative)
    }

    /// `item_picked_up_post` for the acting player, who took all of
    /// `original` from the item entity `item`: the stack a mod left on it,
    /// `None` when none did or no mod listens.
    pub(in crate::game) fn native_pickup_post(
        &mut self,
        item: u32,
        original: &ItemStack,
    ) -> Option<ItemStack> {
        let mut remaining = None;
        if mod_api::__private::listened(mod_api::HookId::ItemPickedUpPost)
            && let Some(item) = self.native_item_handle(item)
        {
            let player = self.native_acting_id();
            self.native_item_picked_up_post(player, item, original, &mut remaining);
        }
        remaining
    }

    /// `item_picked_up_partial` for the acting player, who took part of
    /// `original` and left `rest`: the stack the item entity then holds,
    /// `None` when a mod emptied it, `rest` when no mod listens.
    pub(in crate::game) fn native_pickup_partial(
        &mut self,
        original: &ItemStack,
        rest: ItemStack,
    ) -> Option<ItemStack> {
        let mut remaining = Some(rest);
        if mod_api::__private::listened(mod_api::HookId::ItemPickedUpPartial) {
            let player = self.native_acting_id();
            self.native_item_picked_up_partial(player, original, &mut remaining);
        }
        remaining
    }

    /// The item entity `id` as the hooks name it. It need not be in the
    /// level: a canceled toss and the host player's pickup hand out one the
    /// handle reads cannot find.
    pub(in crate::game) fn native_item_handle(&self, id: u32) -> Option<ItemEntityRef> {
        let epoch = self.native_lifecycle.epoch()?;
        Some(ItemEntityRef::new(EntityRef::new(epoch, id)))
    }

    /// `ItemEntity.playerTouch`'s rule for `player`: no pickup delay, and no
    /// target or `player` as the target, unless `answer` forces it either
    /// way. A refusal sets a vanilla eligibility delay of at least 10 ticks;
    /// later touches still dispatch the hook, and `True` overrides the delay.
    pub(in crate::game) fn pickup_decision(
        pickup_delay: &mut i64,
        target: Option<[i32; 4]>,
        player: [i32; 4],
        answer: TriState,
    ) -> bool {
        if answer == TriState::False {
            *pickup_delay = (*pickup_delay).max(10);
            return false;
        }
        answer.or(*pickup_delay == 0 && target.is_none_or(|t| t == player))
    }
}
