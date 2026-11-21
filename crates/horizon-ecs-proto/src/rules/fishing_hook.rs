//! The numbers of a fishing hook (`FishingHook`): the bite's plunge, the lure's wait and tease, the margin of a path, the float's
//! force, the pull of a catch and the rod's damage.
use horizon_core::geometry::Vec3;
use horizon_core::random::{LegacyRandom, RandomSource};

/// `FishingHook.onSyncedDataUpdated` on the bite: the plunge `-0.4 * nextFloat(0.6, 1.0)`, in float as vanilla.
pub fn bite_impulse(r: &mut impl RandomSource) -> f64 {
    f64::from(-0.4_f32 * (0.6_f32 + r.next_float() * (1.0_f32 - 0.6_f32)))
}

/// The lure's wait: 100 to 600 ticks less the rod's Lure (`lureWait` with no medium).
pub fn lure_wait(rng: &mut LegacyRandom, lure: i32) -> i32 {
    100 + rng.next_int_bound(501).unwrap_or(0) - lure
}

/// The margin of a target's box for the path (`ProjectileUtil.computeMargin`): 0 to 0.3 over the first eight ticks.
pub fn path_margin(tick_count: u8) -> f64 {
    f64::from(i32::from(tick_count).saturating_sub(2).clamp(0, 6)) / 20.0
}

/// The chance per tick that a fish teases the lure: 0.15, more as the lure's wait ends (under 60 ticks left).
pub fn lure_tease(time_until_lured: i32) -> f32 {
    let mut tease = 0.15_f32;
    let t = time_until_lured;
    if t < 20 {
        tease += (20 - t) as f32 * 0.05;
    } else if t < 40 {
        tease += (40 - t) as f32 * 0.02;
    } else if t < 60 {
        tease += (60 - t) as f32 * 0.01;
    }
    tease
}

/// The force that holds the float at the surface: its height above the cell's water, with a push of 0.1 when it is almost level.
pub fn buoyancy_force(y: f64, vel_y: f64, cell_y: i32, liquid: f32) -> f64 {
    let mut force = y + vel_y - f64::from(cell_y) - f64::from(liquid);
    if force.abs() < 0.01 {
        force += force.signum() * 0.1;
    }
    force
}

/// The height where the float rests in the water: the buoyancy force is zero there, with no velocity (`cell_y` + the liquid's height).
pub fn rest_height(cell_y: i32, liquid: f32) -> f64 {
    f64::from(cell_y) + f64::from(liquid)
}

/// The pull of a hooked target towards its owner at `player`: the offset from `at` times `share` (`pullEntity`).
pub fn pull_toward(player: [f64; 3], at: Vec3, share: f64) -> [f64; 3] {
    [(player[0] - at.x) * share, (player[1] - at.y) * share, (player[2] - at.z) * share]
}

/// The rod's damage on a reel: 3 for an item and 5 for another target when hooked (`hooked` tells whether it is an item), else 1 for a
/// bite and 2 on the ground for a catch, 2 on the ground otherwise.
pub fn reel_damage(hooked: Option<bool>, bite: bool, on_ground: bool) -> i32 {
    match hooked {
        Some(true) => 3,
        Some(false) => 5,
        None if bite => if on_ground { 2 } else { 1 },
        None => if on_ground { 2 } else { 0 },
    }
}
