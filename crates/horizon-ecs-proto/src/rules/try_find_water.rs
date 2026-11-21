//! The water search's points: the box drawn around the animal's feet, the point the walk aims at, and the background condition.

/// A point of the search box around the feet `b`: the offsets `dx` and `dy` and `dz` are the draws (x 20, y 8, z 20), the feet row is `+ 2 - dy`.
pub fn box_point(b: [i32; 3], dx: i32, dy: i32, dz: i32) -> [i32; 3] {
    [b[0] + dx - 10, b[1] + 2 - dy, b[2] + dz - 10]
}

/// The point the walk aims at: the water block's corner, or its middle (`+ 0.5` on x and z) for the drowned.
pub fn aim_point(p: [i32; 3], centre: bool) -> [f64; 3] {
    let d = if centre { 0.5 } else { 0.0 };
    [f64::from(p[0]) + d, f64::from(p[1]), f64::from(p[2]) + d]
}

/// The background condition: out of the water, and on the ground when the form needs it.
pub fn clear_of_water(in_water: bool, on_ground: bool, needs_ground: bool) -> bool {
    !in_water && (!needs_ground || on_ground)
}
