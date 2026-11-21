//! The harness of a happy ghast: which item names are harnesses.

/// An item is a harness when its name ends with `_harness`.
pub fn is_harness_name(name: &str) -> bool {
    name.ends_with("_harness")
}
