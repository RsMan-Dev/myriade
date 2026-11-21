//! The village stroll's spot: a walk leans to the nearest village section unless the villager roams freely, and the centre of that section.

/// True when the villager roams freely: fewer than two sections to the village, or the nearest one is its own section.
pub fn roams_freely(sections_to_village: i32, best: [i32; 3], section: [i32; 3]) -> bool {
    sections_to_village <= 1 || best == section
}

/// The centre of section `s` (16 blocks wide, the x and z centres at 0.5), as the `Mob` aims at it.
pub fn section_centre(s: [i32; 3]) -> [f64; 3] {
    [f64::from(s[0] * 16 + 8) + 0.5, f64::from(s[1] * 16 + 8), f64::from(s[2] * 16 + 8) + 0.5]
}
