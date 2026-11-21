//! An item stack's views for the component resolver and the JSON form of its tags.
use horizon_assets::item_model::StackView;
use horizon_core::nbt::Tag;
use serde_json::Value;

/// The JSON form of a tag, as the component codec reads it.
pub fn tag_json(tag: &Tag) -> Value {
    horizon_client_core::component_codec::raw_tag_json(tag)
}

/// The resolver's view of a client stack: its item, count and component patch.
pub fn stack_view(stack: &crate::items::ItemStack) -> StackView {
    let mut view = StackView::new(&stack.item, stack.count as i32);
    view.bundle_selected = stack.data().map_or(-1, |d| d.bundle_selected.0);
    if let Some(Tag::Compound(c)) = crate::item_data::components_tag(stack.damage, stack.data()) {
        for (k, v) in &c {
            let key = k.to_string_lossy();
            match key.strip_prefix('!') {
                Some(removed) => view.patch.insert(removed.to_owned(), None),
                None => {
                    let value =
                        horizon_client_core::component_codec::nbt_component_to_json(&key, v);
                    view.patch.insert(key, Some(value))
                }
            };
        }
    }
    view
}
