//! The numbers of an explosion: the exposure of a box to the centre, the share of the diameter, the push and the damage (`ServerExplosion`).
use myriade_engine_lib::arrows::clip_block_hit;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};

/// `ServerExplosion.getSeenPercent` for a box: the share of the grid's rays to `center` that no block collider stops.
pub fn seen_share(mw: &MobWorld, center: Vec3, b: Aabb) -> f32 {
    let (lo, hi) = (b.min_position(), b.max_position());
    let (xs, ys, zs) = (1.0 / ((hi.x - lo.x) * 2.0 + 1.0), 1.0 / ((hi.y - lo.y) * 2.0 + 1.0), 1.0 / ((hi.z - lo.z) * 2.0 + 1.0));
    let (xo, zo) = ((1.0 - (1.0 / xs).floor() * xs) / 2.0, (1.0 - (1.0 / zs).floor() * zs) / 2.0);
    let (mut hits, mut count) = (0, 0);
    let mut xx = 0.0;
    while xx <= 1.0 {
        let mut yy = 0.0;
        while yy <= 1.0 {
            let mut zz = 0.0;
            while zz <= 1.0 {
                let from = Vec3::new(lo.x + (hi.x - lo.x) * xx + xo, lo.y + (hi.y - lo.y) * yy, lo.z + (hi.z - lo.z) * zz + zo);
                let to = Vec3::new(center.x - from.x, center.y - from.y, center.z - from.z);
                if clip_block_hit(mw.world, mw.facts, mw.collision, from, to).is_none() {
                    hits += 1;
                }
                count += 1;
                zz += zs;
            }
            yy += ys;
        }
        xx += xs;
    }
    hits as f32 / count as f32
}

/// The distance from `at` to `feet` as a share of the `diameter` (1 at the edge of the blast).
pub fn distance_share(feet: Vec3, at: Vec3, diameter: f64) -> f64 {
    ((feet.x - at.x).powi(2) + (feet.y - at.y).powi(2) + (feet.z - at.z).powi(2)).sqrt() / diameter
}

/// The power a creature at `dist` (a share of the diameter) is pushed or hurt with: `(1 - dist) * exposure`.
pub fn push_power(dist: f64, exposure: f32) -> f64 {
    (1.0 - dist) * f64::from(exposure)
}

/// The damage a creature takes at `dist` (a share of the diameter) with `exposure`.
pub fn blast_damage(dist: f64, exposure: f32, diameter: f64) -> f32 {
    let pow = (1.0 - dist) * f64::from(exposure);
    ((pow * pow + pow) / 2.0 * 7.0 * diameter + 1.0) as f32
}

/// The value of a fuse at `now`: it moved by `dir` a tick since `at` from `value`, kept in `0..=max`.
pub fn fuse_value(value: i32, dir: i8, at: u64, now: u64, max: i32) -> i32 {
    (value + i32::from(dir) * (now - at) as i32).clamp(0, max)
}

/// The tick a rising fuse at `value` is due at: `max - value` ticks from `now`, at least one.
pub fn fuse_arm_tick(now: u64, value: i32, max: i32) -> u64 {
    now + (max - value).max(1) as u64
}

/// The blast radius of a creature: twice the radius when it is charged (struck by lightning).
pub fn charged_radius(radius: f32, charged: bool) -> f32 {
    radius * if charged { 2.0 } else { 1.0 }
}
