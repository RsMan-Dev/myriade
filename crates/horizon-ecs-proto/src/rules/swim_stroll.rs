//! `SwimStroll` (`brain/behaviours/swim_stroll.rs`): the tries' scale and the next point of the search.

/// The `Mob`'s list of tries, each (h, v) in blocks.
pub const SCALE: &[(i32, i32)] = &[(1, 1), (3, 3), (5, 5), (6, 5), (7, 7), (10, 7)];

/// The next try: `h` blocks along the horizontal direction from `me` to `last`, `v` vertically (the point itself when they coincide).
pub fn extend(me: [f64; 3], last: [f64; 3], h: i32, v: i32) -> [f64; 3] {
    let d = [last[0] - me[0], last[1] - me[1], last[2] - me[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let n = if len < 1.0e-5 { [0.0; 3] } else { d.map(|c| c / len) };
    [me[0] + n[0] * f64::from(h), me[1] + n[1] * f64::from(v), me[2] + n[2] * f64::from(h)]
}
