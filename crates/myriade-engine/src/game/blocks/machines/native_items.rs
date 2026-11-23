//! Hopper helpers shared by the machines.
use super::{Game, View};

impl Game<'_> {
    /// `HopperBlockEntity.inventoryFull`: every slot holds exactly a full item stack.
    pub(super) fn hopper_is_stacked(&self, view: &View) -> bool {
        let c = self.containers();
        (0..view.len()).all(|slot| {
            c.get(view, slot)
                .is_some_and(|stack| stack.count == c.max_stack(stack))
        })
    }
    /// The level in place (`minecraft:overworld`).
    pub(in crate::game) fn dimension(&self) -> &'static str {
        self.dim.id()
    }
}
