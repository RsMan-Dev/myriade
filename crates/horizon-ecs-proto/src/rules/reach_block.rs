//! Calculs de la marche vers le bloc d'une mémoire (`WalkToBlockMemory`) : centre du bloc, distance de Manhattan, arrivée, pas atteint.
use horizon_core::geometry::Vec3;

/// Le centre du bloc `p` au sol.
pub fn block_centre(p: [i32; 3]) -> [f64; 3] {
    [f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5]
}

/// The Manhattan distance between two blocks.
pub fn manhattan(a: [i32; 3], b: [i32; 3]) -> i32 {
    (0..3).map(|i| (a[i] - b[i]).abs()).sum()
}

/// The walk has arrived: the centre of block `p` is within `range` of `pos` horizontally.
pub fn arrived(p: [i32; 3], pos: Vec3, range: f64) -> bool {
    let (dx, dz) = (f64::from(p[0]) + 0.5 - pos.x, f64::from(p[2]) + 0.5 - pos.z);
    dx * dx + dz * dz <= range * range
}

/// The random step `step` is reached from `pos`: the centre is less than 3 blocks away.
pub fn step_reached(step: [i32; 3], pos: Vec3) -> bool {
    crate::rules::block_reach::centre(step).distance_squared(pos) < 9.0
}
