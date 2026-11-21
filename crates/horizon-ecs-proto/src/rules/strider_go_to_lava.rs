//! Calculs de la marche vers la lave (`StriderGoToLavaGoal`) : la case de la lave, le point où l'on se tient, le point visé.
use horizon_core::geometry::Vec3;

/// The block a position stands in.
pub fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

/// The point the walk starts toward: the centre of the lava block, standing on the row above it.
pub fn standing_point(b: [i32; 3]) -> [f64; 3] {
    [f64::from(b[0]) + 0.5, f64::from(b[1] + 1), f64::from(b[2]) + 0.5]
}

/// The point the walk re-aims at: the centre of the lava block, on its own row.
pub fn walk_point(b: [i32; 3]) -> [f64; 3] {
    [f64::from(b[0]) + 0.5, f64::from(b[1]), f64::from(b[2]) + 0.5]
}

/// The strider is not yet at the lava block: its squared distance to the block centre is at least 1.
pub fn still_away(p: Vec3, b: [i32; 3]) -> bool {
    let c = [f64::from(b[0]) + 0.5, f64::from(b[1]) + 0.5, f64::from(b[2]) + 0.5];
    (p.x - c[0]).powi(2) + (p.y - c[1]).powi(2) + (p.z - c[2]).powi(2) >= 1.0
}
