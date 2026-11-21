//! The variant's choice at the spawn: the snow kind, the default red kind.

/// The variant of the snow kind (`Fox.Type.SNOW`).
pub const SNOW: u8 = 1;

/// The spawn's choice: the group's variant when it gives one, else snow in a biome with the tag.
pub fn draw(group: Option<u8>, biome_has_snow_tag: bool) -> u8 {
    group.unwrap_or(if biome_has_snow_tag { SNOW } else { 0 })
}
