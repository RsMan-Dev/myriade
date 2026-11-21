//! The names of an entity's `Tags` list: the list's strings, or nothing when the tag is not a list of strings.
use horizon_core::nbt::Tag;

/// The names a `Tags` tag holds; `None` when it is not a list of strings.
pub fn tag_names(tag: &Tag) -> Option<Vec<String>> {
    let Tag::List(items) = tag else { return None };
    let mut new = Vec::with_capacity(items.len());
    for item in items {
        let Tag::String(s) = item else { return None };
        let Ok(s) = s.to_string() else { return None };
        new.push(s);
    }
    Some(new)
}
