//! The attribute modifiers an item stack grants (`ItemStack.getAttributeModifiers`):
//! the stack's own `minecraft:attribute_modifiers` or its item's
//! (`ItemStack::default_attribute_modifiers`). Every reader of a stack's
//! modifiers goes through here.
use horizon_items::item_data::ModifierRow;
use horizon_items::items::{ItemStack, Items};
use std::borrow::Cow;

/// The modifiers `stack` grants, with its item's from `items`.
pub fn attribute_modifiers<'a>(stack: &ItemStack, items: &'a Items) -> Cow<'a, [ModifierRow]> {
    stack.default_attribute_modifiers(items)
}

/// `attribute_modifiers` with the item's list given: `default`.
pub fn attribute_modifiers_or<'a>(
    stack: &ItemStack,
    default: &'a [ModifierRow],
) -> Cow<'a, [ModifierRow]> {
    stack.default_attribute_modifiers_or(default)
}
