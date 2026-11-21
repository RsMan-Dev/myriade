use horizon_core::geometry::Vec3;
use horizon_core::random::RandomSource;

/// The life of an orb, from its last merge: 6 000 ticks.
pub const LIFE: u64 = 6000;
/// The reach of the attraction: `getNearestPlayer(this, 8.0)`.
pub const REACH: f64 = 8.0;

/// The tick of an orb's expiry check: its life after `age_base`, kept between `now + 1` and `now + 4000`.
pub fn expire_tick(age_base: u64, now: u64) -> u64 {
    (age_base + LIFE).min(now + 4000).max(now + 1)
}

/// The wait to the next tick 1 modulo 20 of an orb that is `age` ticks old (`ExperienceOrb.tick`: `tickCount % 20 == 1`).
pub fn check_wait(age: u64) -> u64 {
    (21 - age % 20 - 1) % 20 + 1
}

/// Whether an orb of that age scans for merges and looks for a player (`tickCount % 20 == 1`).
pub fn is_scan_tick(age: u64) -> bool {
    age % 20 == 1
}

/// The throw of a new orb (`ExperienceOrb.award`): three draws, x, y, z, with a spread of 0.2 and a speed of 2.
pub fn throw_velocity(random: &mut impl RandomSource) -> Vec3 {
    Vec3::new((random.next_double() * 0.2 - 0.1) * 2.0, random.next_double() * 0.2 * 2.0, (random.next_double() * 0.2 - 0.1) * 2.0)
}

/// The pull of an orb at `pos` towards a player at `target` within `REACH`: the speed it gains this tick, `None` when it gains none.
pub fn attraction(pos: Vec3, target: Vec3) -> Option<Vec3> {
    let d = Vec3::new((target.x - pos.x) / REACH, (target.y + 0.81 - pos.y) / REACH, (target.z - pos.z) / REACH);
    let length = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
    let power = 1.0 - length;
    if power > 0.0 && length > 1e-6 {
        let power = power * power;
        Some(Vec3::new(d.x / length * power * 0.1, d.y / length * power * 0.1, d.z / length * power * 0.1))
    } else {
        None
    }
}

/// The age base of a stack that takes in another of age `other_age`: the smaller age wins.
pub fn merged_age_base(now: u64, age_base: u64, other_age: u64) -> u64 {
    now.saturating_sub((now - age_base).min(other_age))
}

/// Whether the orb `theirs` may merge with `ours`: their ids are a multiple of 40 apart.
pub fn same_merge_group(ours: u32, theirs: u32) -> bool {
    (theirs as i32).wrapping_sub(ours as i32) % 40 == 0
}
