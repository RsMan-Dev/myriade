//! Calculations of the bee: the distances to its hive and flower, its hop toward a spot, the way it hovers and leaves its hive.
use horizon_core::geometry::Vec3;

/// Whether the block `b` is closer than `dist` blocks to `p` (`Mob`'s `closerThan`: the squared distance under `dist` squared).
pub fn within_blocks(b: [i32; 3], p: [i32; 3], dist: i32) -> bool {
    let d = |i: usize| f64::from(p[i] - b[i]);
    d(0) * d(0) + d(1) * d(1) + d(2) * d(2) < f64::from(dist * dist)
}

/// The plan of a hop toward `p` from `here`: the vertical adjustment of the flight (4 up, -4 down, else 0), the horizontal and the
/// vertical radius of the spots looked at.
pub fn hop_plan(here: [i32; 3], p: [i32; 3]) -> (i32, i32, i32) {
    let y_delta = p[1] - here[1];
    let fly = if y_delta > 2 { 4 } else if y_delta < -2 { -4 } else { 0 };
    let dist = (p[0] - here[0]).abs() + (p[1] - here[1]).abs() + (p[2] - here[2]).abs();
    let (h, v) = if dist < 15 { (dist / 2, dist / 2) } else { (6, 8) };
    (fly, h, v)
}

/// The horizontal unit direction from `p` toward the centre of the hive `h`; zero when it is there.
pub fn unit_toward_xz(h: [i32; 3], p: Vec3) -> (f64, f64) {
    let v = [f64::from(h[0]) + 0.5 - p.x, f64::from(h[1]) + 0.5 - p.y, f64::from(h[2]) + 0.5 - p.z];
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1.0e-5 { (0.0, 0.0) } else { (v[0] / len, v[2] / len) }
}

/// The cells of the box of radii `rx`, `ry`, `rz` around `o`, nearest by Manhattan distance first (`withinBoxByManhattanDistance`).
pub fn manhattan_box(o: [i32; 3], rx: i32, ry: i32, rz: i32) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    for depth in 0..=rx + ry + rz {
        let max_x = rx.min(depth);
        for x in -max_x..=max_x {
            let max_y = ry.min(depth - x.abs());
            for y in -max_y..=max_y {
                let z = depth - x.abs() - y.abs();
                if z <= rz {
                    out.push([o[0] + x, o[1] + y, o[2] + z]);
                    if z != 0 {
                        out.push([o[0] + x, o[1] + y, o[2] - z]);
                    }
                }
            }
        }
    }
    out
}

/// The horizontal step out of a hive from its `facing` (none or unknown: no step).
pub fn front_of(facing: Option<&str>) -> [i32; 2] {
    match facing {
        Some("north") => [0, -1],
        Some("south") => [0, 1],
        Some("west") => [-1, 0],
        Some("east") => [1, 0],
        _ => [0, 0],
    }
}

/// The point a bee comes out at: in front of the hive `h` along `f`, at 0.55 plus half its width, or on the hive when the front is `blocked`;
/// its height is half its body's.
pub fn exit_point(h: [i32; 3], f: [i32; 2], blocked: bool, width: f32, height: f32) -> Vec3 {
    let delta = if blocked { 0.0 } else { 0.55 + f64::from(width / 2.0) };
    Vec3::new(f64::from(h[0]) + 0.5 + delta * f64::from(f[0]), f64::from(h[1]) + 0.5 - f64::from(height / 2.0), f64::from(h[2]) + 0.5 + delta * f64::from(f[1]))
}

/// One offset of the hover over a flower, from a draw in `[0, 1)`: within a third of a block.
pub fn hover_offset(draw: f32) -> f64 {
    f64::from((draw * 2.0 - 1.0) * 0.333_333_34)
}

/// Whether a hover that lasted `successful` ticks may end on this ask (the chance is drawn only then).
pub fn may_stop_hover(successful: i32) -> bool {
    successful > 400 && successful % 2 == 0
}

/// Whether the bee is within `reach_sqr` (squared) of the centre of the hive `h`.
pub fn close_to_hive(h: [i32; 3], p: Vec3, reach_sqr: f64) -> bool {
    (f64::from(h[0]) + 0.5 - p.x).powi(2) + (f64::from(h[1]) + 0.5 - p.y).powi(2) + (f64::from(h[2]) + 0.5 - p.z).powi(2) < reach_sqr
}
