//! Container geometry constructed by the screen build seam.
use super::screen_build::ContainerLayout;
use super::{Creative, Game, Screen};
use horizon_client::inventory_effects::EffectsLayout;

#[cfg(test)]
thread_local! {
    pub(in crate::game) static LAYOUT_CONSTRUCTIONS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Selectors read by slot_position; item contents and scroll do not move slots.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SlotInputs {
    count: usize,
    craft_size: usize,
    creative_inventory: bool,
    creative_tab: bool,
    station: Option<(std::mem::Discriminant<super::stations::Kind>, usize)>,
    container: Option<std::mem::Discriminant<horizon_client::containers::BlockEntity>>,
    machine: Option<bool>,
    crafter: bool,
    mount: Option<(usize, bool, bool)>,
    rows: Option<usize>,
    native: Option<(u64, Option<f32>, Option<f32>)>,
}

impl Game<'_> {
    fn native_slot_inputs(&self) -> Option<(u64, Option<f32>, Option<f32>)> {
        self.shown_native_screen().map(|s| {
            (
                s.layout_revision,
                self.native_screen_inventory_x(),
                self.native_screen_inventory_y(),
            )
        })
    }

    pub(super) fn build_container_layout(
        &self,
        batch: &horizon_client::hud::HudBatch,
        mut inputs: ContainerLayout,
    ) -> ContainerLayout {
        #[cfg(test)]
        LAYOUT_CONSTRUCTIONS.with(|count| count.set(count.get() + 1));
        let origin = inputs.origin;
        let slots = self
            .slots()
            .into_iter()
            .map(|slot| {
                let (x, y) = self.slot_position(slot);
                (slot, [origin.0 + x - 1.0, origin.1 + y - 1.0, 18.0, 18.0])
            })
            .collect();
        inputs.effects = inputs
            .shows_effects
            .then(|| {
                self.inventory_effects_rect(
                    batch,
                    [inputs.origin.0, inputs.origin.1],
                    inputs.image[2],
                    &inputs.effects_layout,
                )
            })
            .flatten();
        inputs.slots = slots;
        inputs
    }

}
