//! The rules of the dragon's respawn: the cell of each portal side, the growls and the pillar's timing, the cage's bar joins, the spike's
//! disc and the test of a crystal on a spike.
use horizon_core::geometry::{Aabb, Vec3};

/// The one-block cell three out and one up from the exit portal's `origin`, on the side `d` (a unit step in x and z).
pub fn side_cell(origin: [i32; 3], d: [i32; 2]) -> Aabb {
    let lo = Vec3::new(f64::from(origin[0] + d[0] * 3), f64::from(origin[1] + 1), f64::from(origin[2] + d[1] * 3));
    Aabb::new(lo, Vec3::new(lo.x + 1.0, lo.y + 1.0, lo.z + 1.0))
}

/// The growl of the preparing stage: at its first tick, at 50 to 52, and from 95 on.
pub fn preparing_growl(time: u32) -> bool {
    matches!(time, 0 | 50..=52) || time >= 95
}

/// The growl of the summoning stage: from 80 on, and at the first four ticks after its start.
pub fn summoning_growl(time: u32) -> bool {
    time >= 80 || (1..5).contains(&time)
}

/// The pillar stage's two edges within each 40-tick slot: `(beam starts, spike is rebuilt)`.
pub fn pillar_edges(time: u32) -> (bool, bool) {
    (time % 40 == 0, time % 40 == 39)
}

/// The spike a pillar slot works on: the slot's index in the seed's spike list.
pub fn spike_index(time: u32) -> usize {
    (time / 40) as usize
}

/// The cage's bars of a guarded spike at `(dx, dz, dy)` from its centre and base: `None` off the cage, else which of north, south, west
/// and east each bar joins.
pub fn cage_sides(dx: i32, dz: i32, dy: i32) -> Option<[bool; 4]> {
    let top = dy == 3;
    if !(dx.abs() == 2 || dz.abs() == 2 || top) {
        return None;
    }
    let (x_edge, z_edge) = (dx.abs() == 2 || top, dz.abs() == 2 || top);
    Some([x_edge && dz != -2, x_edge && dz != 2, z_edge && dx != -2, z_edge && dx != 2])
}

/// Whether `(dx, dz)` from the spike's centre lies in its disc of `radius` (plus the one-block margin of `EndSpikeFeature`).
pub fn in_disc(dx: i32, dz: i32, radius: i32) -> bool {
    let (dx, dz, r) = (f64::from(dx), f64::from(dz), f64::from(radius));
    dx * dx + dz * dz <= r * r + 1.0
}

/// Whether a crystal at `p` stands on the spike with centre `(cx, cz)`, `radius` and `height`: inside its square, from its top up.
pub fn is_on_spike(p: [f64; 3], cx: i32, cz: i32, radius: i32, height: i32) -> bool {
    (p[0] - f64::from(cx) - 0.5).abs() <= f64::from(radius) && (p[2] - f64::from(cz) - 0.5).abs() <= f64::from(radius) && p[1] >= f64::from(height)
}
