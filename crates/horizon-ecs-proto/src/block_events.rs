//! Block changes as an input of the ECS: the host (or `cow::block_changes`) pushes the position of every block whose state changed;
//! whoever depends on a block (`hanging::attached_wake`) reads them within the tick, and `drain_block_events` empties the list after
//! them. Empty at rest: nothing is visited per tick.
use bevy_ecs::prelude::*;

#[derive(Resource, Default)]
pub struct BlockEvents {
    pub changed: Vec<[i32; 3]>,
}

pub fn drain_block_events(mut b: ResMut<BlockEvents>) {
    if !b.changed.is_empty() {
        b.changed.clear();
    }
}
