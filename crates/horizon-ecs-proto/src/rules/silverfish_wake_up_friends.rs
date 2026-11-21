/// The offsets 0, 1, -1, 2, -2 ... up to `limit`.
pub fn offsets(limit: i32) -> impl Iterator<Item = i32> {
    std::iter::once(0).chain((1..=limit).flat_map(|i| [i, -i]))
}

/// The block name is an infested block.
pub fn is_infested(name: &str) -> bool {
    name.starts_with("minecraft:infested_")
}
