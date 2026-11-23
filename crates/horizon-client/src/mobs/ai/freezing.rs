//! `LivingEntity.canFreeze` and the powder-snow test behind it.
use super::{MobWorld};
use horizon_core::geometry::Vec3;



/// The player's HEAD item belongs to vanilla's gaze disguise tag.
pub fn wears_gaze_disguise(head: Option<&str>, w: &MobWorld) -> bool {
    head.is_some_and(|item| {
        w.items
            .and_then(|items| items.item_tags.get("minecraft:gaze_disguise_equipment"))
            .is_some_and(|tag| tag.contains(item))
    })
}
