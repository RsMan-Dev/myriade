//! The rules of the dragon's breath: the wait and the expiry of a cloud, its instant damage, what it reaches, its victims' delays,
//! the flame's origin and the puffs of one tick.
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::{LegacyRandom, RandomSource};

/// `AreaEffectCloud.WAIT_TIME` (D:69): inert for its first 20 ticks.
pub const WAIT: i32 = 20;
/// The victims are checked every 5 ticks and spared for 20 (`reapplicationDelay`), D:77-79.
const CHECK_EVERY: i32 = 5;
const REAPPLY: i32 = 20;

/// A puff: where it starts, and its velocity.
pub type Puff = ([f64; 3], [f64; 3]);

/// `AreaEffectCloud.serverTick`: discarded once `age - WAIT >= duration`, so the wait does not count against the duration.
pub fn expired(age: i32, duration: i32) -> bool {
    age - WAIT >= duration
}

/// The instant damage of `amplifier` at the cloud's 0.5 scale: `(int) (0.5 * (6 << amplifier) + 0.5)` (D:50-52).
pub fn harm(amplifier: u32) -> f32 {
    (0.5 * f64::from(6u32 << amplifier) + 0.5) as i32 as f32
}

/// Whether a cloud at `at` (its floor) reaches a creature: its box is `radius * 2` wide and 0.5 tall, and the creature's
/// position lies within the radius horizontally (D:57-65).
pub fn hits(at: [f64; 3], radius: f32, bounds: Aabb, pos: Vec3) -> bool {
    let r = f64::from(radius);
    let area = Aabb::new(Vec3::new(at[0] - r, at[1], at[2] - r), Vec3::new(at[0] + r, at[1] + 0.5, at[2] + r));
    let (dx, dz) = (pos.x - at[0], pos.z - at[2]);
    bounds.intersects(area) && dx * dx + dz * dz <= f64::from(radius * radius)
}

/// `AreaEffectCloud.serverTick` victims (D:80-97): every 5 ticks after the wait, drop the entries whose delay has passed, then
/// return the candidates not in the list, recording them until `age + 20`.
pub fn due<W: Copy + PartialEq>(age: i32, victims: &mut Vec<(W, i32)>, candidates: impl IntoIterator<Item = W>) -> Vec<W> {
    if age < WAIT || age % CHECK_EVERY != 0 {
        return Vec::new();
    }
    victims.retain(|&(_, until)| age < until);
    let mut hit = Vec::new();
    for who in candidates {
        if !victims.iter().any(|&(v, _)| v == who) {
            victims.push((who, age + REAPPLY));
            hit.push(who);
        }
    }
    hit
}

/// Where a flame starts (D:2563-2592): 2.5 blocks ahead of the head along its horizontal look (`look`, a unit (x, z)), then down
/// to the first block that is not air; the cloud floor is the block above it.
pub fn flame_origin(head: [f64; 3], look: [f64; 2], is_air: impl Fn(i32, i32, i32) -> bool) -> [f64; 3] {
    let (x, z) = (head[0] + look[0] * 2.5, head[2] + look[1] * 2.5);
    let mut y = head[1] + 0.5;
    let (xi, zi) = (x.floor() as i32, z.floor() as i32);
    while y > 0.0 && is_air(xi, y.floor() as i32, zi) {
        y -= 1.0;
    }
    [x, y.floor() + 1.0, z]
}

/// The head's centre, jittered by `nextGaussian() / 2` on each axis, in x, y, z order (`breath_puffs`, D:3439-3446).
fn jitter(head: [f64; 3], r: &mut LegacyRandom) -> [f64; 3] {
    [head[0] + r.next_gaussian() / 2.0, head[1] + r.next_gaussian() / 2.0, head[2] + r.next_gaussian() / 2.0]
}

/// The landing dragon's puffs of one tick (D:3450-3461): eight, along `-look * 0.08` (0.3 vertically) plus the dragon's motion.
pub fn landing_puffs(look: [f64; 3], head: [f64; 3], motion: [f64; 3], r: &mut LegacyRandom) -> Vec<Puff> {
    (0..8)
        .map(|_| {
            let v = [
                -look[0] * f64::from(0.08_f32) + motion[0],
                -look[1] * f64::from(0.3_f32) + motion[1],
                -look[2] * f64::from(0.08_f32) + motion[2],
            ];
            (jitter(head, r), v)
        })
        .collect()
}

/// The flaming dragon's puffs of one tick (D:3462-3481): on the even ticks before tick 10, eight starts each breathe a fan of
/// six puffs down the look line. `ticks` is the phase's counter, already advanced for the tick.
pub fn flame_puffs(ticks: u32, look: [f64; 3], head: [f64; 3], r: &mut LegacyRandom) -> Vec<Puff> {
    if ticks % 2 != 0 || ticks >= 10 {
        return Vec::new();
    }
    let mut puffs = Vec::with_capacity(48);
    for _ in 0..8 {
        let at = jitter(head, r);
        for j in 0..6 {
            let j = f64::from(j);
            puffs.push((
                at,
                [-look[0] * f64::from(0.08_f32) * j, -look[1] * f64::from(0.6_f32), -look[2] * f64::from(0.08_f32) * j],
            ));
        }
    }
    puffs
}
