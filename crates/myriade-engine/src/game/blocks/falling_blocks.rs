//! Falling block entities of the host: the item entity a block leaves as a drop
//! (`drop_falling_block`, `Entity.spawnAtLocation`). Their ticks, landings and breaks are P's (`FallingEvent`, game/ecs/falling.rs).
use super::*;

impl Game<'_> {
    /// `Entity.spawnAtLocation`: an item entity where the block was.
    pub(in crate::game) fn drop_falling_block(&mut self, state: BlockStateId, at: Vec3) {
        // Entity.spawnAtLocation: an item entity where the block was.
        let item = self.res.facts().get(state).item.clone();
        if &*item != "minecraft:air" {
            // new ItemEntity(level, x, y, z, stack): the motion is from the
            // item's own random, not the level's.
            let stack = ItemStack {
                item,
                count: 1,
                damage: 0,
                extra: None,
            };
            self.spawn_item_own(stack, [at.x, at.y, at.z], 10);
        }
    }
}
