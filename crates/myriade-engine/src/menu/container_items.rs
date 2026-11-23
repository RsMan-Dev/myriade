//! `Item.canFitInsideContainerItems`: whether a stack may go inside an item that holds
//! items, a shulker box or a bundle.
use super::ItemStack;

/// Vanilla's `Item.canFitInsideContainerItems`: a shulker box can't go inside an item that
/// holds items (`ShulkerBoxBlock` items answer false).
pub fn vanilla_fits_inside_container_items(stack: &ItemStack) -> bool {
    !stack.item.ends_with("shulker_box")
}

/// `ItemStack.canFitInsideContainerItems` (`ShulkerBoxSlot.mayPlace`).
pub fn fits_inside_container_items(stack: &ItemStack) -> bool {
    vanilla_fits_inside_container_items(stack)
}

/// `BundleContents.canItemBeInBundle`: a stack that isn't empty and fits inside container
/// items ([`fits_inside_container_items`]).
pub fn can_be_in_bundle(stack: &ItemStack) -> bool {
    stack.count > 0 && fits_inside_container_items(stack)
}
