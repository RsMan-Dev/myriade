impl Game<'_> {
    /// Sets item entity `id`'s stack, keeping it as it was if a transaction is open and hasn't
    /// yet. Emptied in one, it stays, empty, until the root commit removes it, so an abort can
    /// put its stack back. False where no such item entity is.
    pub(super) fn journal_set_item_entity(&mut self, id: u32, stack: ItemStack) -> bool {
        let Some(level) = self.transactions.last_mut() else {
            return self.native_set_item_entity(id, stack);
        };
        let Some(item) = self.ecs.item(id) else {
            return false;
        };
        level.item_entities.entry(id).or_insert_with(|| item.stack.clone());
        if stack.count == 0 {
            level.after.push(After::Emptied(id));
        }
        self.ecs.set_item_stack(id, stack)
    }

    /// Sets the stack of item entity `id`, as a hopper's `addItem` leaves what it couldn't take
    /// (`ItemEntity.setItem`); an empty stack removes the entity (`setItem(EMPTY)`, then
    /// `discard`). False where the level has no such item entity.
    pub(super) fn native_set_item_entity(&mut self, id: u32, stack: ItemStack) -> bool {
        if self.ecs.item(id).is_none() {
            return false;
        }
        if stack.count == 0 {
            self.ecs_remove_item(id);
        } else {
            self.ecs.set_item_stack(id, stack);
        }
        true
    }
}
