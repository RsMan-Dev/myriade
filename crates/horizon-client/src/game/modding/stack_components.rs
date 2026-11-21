//! A stack's effective components: the item defaults with the stack's own patch on top.
use horizon_assets::item_model::StackView;
use serde_json::Value;
use std::collections::BTreeMap;

/// `ComponentsView.get`: the stack's patch for `id` (a removal reads as absent), else the item's default.
pub(in crate::game) fn stack_component(
    defaults: &BTreeMap<String, Value>,
    stack: &StackView,
    id: &str,
) -> Option<Value> {
    match stack.patch.get(id) {
        Some(patched) => patched.clone(),
        None => defaults.get(stack.item.as_str())?.get(id).cloned(),
    }
}
