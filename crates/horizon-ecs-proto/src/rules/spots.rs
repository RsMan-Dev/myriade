use crate::IFloor as _;

/// `GoalUtils.isRestricted(mobRestricted(mob, h), mob, p)`: near its home (`center`, `radius`), a position outside it is refused.
pub fn outside_home(center: [i32; 3], radius: i32, pos: [f64; 3], h: f64, p: [i32; 3]) -> bool {
    let d = |i: usize, v: f64| f64::from(center[i]) + 0.5 - v;
    let (x, y, z) = (d(0, pos[0]), d(1, pos[1]), d(2, pos[2]));
    let reach = f64::from(radius) + h + 1.0;
    let near = x * x + y * y + z * z < reach * reach;
    let inside = (0..3).map(|i| (f64::from(center[i]) - f64::from(p[i])).powi(2)).sum::<f64>() < f64::from(radius.wrapping_mul(radius));
    near && !inside
}

/// The offset is a block offset: it is rounded first, then added to the position and rounded again.
pub fn toward(pos: [f64; 3], d: [i32; 3]) -> [i32; 3] {
    [
        (f64::from(d[0]) + pos[0]).ifloor(),
        (f64::from(d[1]) + pos[1]).ifloor(),
        (f64::from(d[2]) + pos[2]).ifloor(),
    ]
}
