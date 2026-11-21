//! The rules of the dragon's hurts: the damage a part takes, the damage window, what a blow leaves, the kill credit, the knockback
//! direction, the blast's share on each part and the wing and head zones with their push.
use crate::rules::dragon_data::{EYE, HEAD, HEAD_ZONE, HURT_INVULNERABLE, HURT_TIME, HURT_WINDOW, NECK, WING_DROP, WING_LIFT, WING_PUSH, WING_ZONE};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::{LegacyRandom, RandomSource};

/// The kill credit window (`d.ticks - hit <= 100`).
pub const CREDIT_TICKS: u32 = 100;

/// The damage a blow does to part `part`: the body parts take a quarter of the hit and at most one more; the head and neck take all of it.
pub fn part_damage(part: usize, damage: f32) -> f32 {
    if part != HEAD && part != NECK { damage / 4.0 + damage.min(1.0) } else { damage }
}

/// Whether the damage window is open: within `HURT_WINDOW` ticks of a hit, only the excess over the last hit lands.
pub fn window_open(window: i32) -> bool {
    window > HURT_WINDOW
}

/// The damage that lands: inside the window, the excess over the last hit; else the whole damage.
pub fn dealt(damage: f32, last: f32, window: bool) -> f32 {
    if window { damage - last } else { damage }
}

/// The health left after `dealt` from `before`, and whether it is a killing blow that starts the dying: a perched dragon stops at 0,
/// any other goes on at 1 and dies.
pub fn health_after(before: f32, dealt: f32, sitting: bool) -> (f32, bool) {
    let after = before - dealt;
    if after <= 0.0 {
        if sitting { (0.0, false) } else { (1.0, true) }
    } else {
        (after, false)
    }
}

/// The window the dragon keeps after a blow: the old one inside the window, else the invulnerable time.
pub fn window_after(window: bool, old: i32) -> i32 {
    if window { old } else { HURT_INVULNERABLE }
}

/// The `hurt_time` after a blow: a full hit sets it, the excess keeps the old one.
pub fn time_after(full: bool, old: i32) -> i32 {
    if full { HURT_TIME } else { old }
}

/// Whether a hit at tick `hit` still credits the kill at tick `ticks`.
pub fn in_credit(ticks: u32, hit: u32) -> bool {
    ticks - hit <= CREDIT_TICKS
}

/// The knockback direction `(xd, zd)` redrawn while it is too short: two draws of the dragon's stream per try, scaled by 0.01.
pub fn redraw_dir(mut xd: f64, mut zd: f64, r: &mut LegacyRandom) -> (f64, f64) {
    while xd * xd + zd * zd < f64::from(1.0e-5_f32) {
        xd = (r.next_double() - r.next_double()) * 0.01;
        zd = (r.next_double() - r.next_double()) * 0.01;
    }
    (xd, zd)
}

/// The knockback of `power` along `(xd, zd)`, normalised.
pub fn push_dir(xd: f64, zd: f64, power: f64) -> (f64, f64) {
    let len = (xd * xd + zd * zd).sqrt();
    (xd / len * power, zd / len * power)
}

/// The distance of `at` from the blast's `center`, in diameters of the blast.
pub fn blast_dist(at: [f64; 3], center: [f64; 3], diameter: f64) -> f64 {
    ((at[0] - center[0]).powi(2) + (at[1] - center[1]).powi(2) + (at[2] - center[2]).powi(2)).sqrt() / diameter
}

/// `Entity.pushFromExplosion` on the dragon from its feet `p`: the push along the blast to its eye, `(1 - distance) * exposure`.
/// `None` outside the blast.
pub fn blast_push(p: [f64; 3], center: [f64; 3], radius: f32, exposure: f32) -> Option<[f64; 3]> {
    let diameter = f64::from(radius * 2.0);
    let at = |y: f64| [p[0] - center[0], y - center[1], p[2] - center[2]];
    let len = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let dist = len(at(p[1])) / diameter;
    if dist > 1.0 {
        return None;
    }
    let away = at(p[1] + f64::from(EYE));
    let l = len(away);
    // `Vec3.normalize` gives zero below 1.0E-5F.
    if l < 9.999_999_747_378_752e-6 {
        return Some([0.0; 3]);
    }
    let power = (1.0 - dist) * f64::from(exposure);
    Some(away.map(|d| d / l * power))
}

/// The box of a wing's contact zone: its part box inflated by `WING_ZONE` and dropped by `WING_DROP`.
pub fn wing_box(b: Aabb) -> Aabb {
    let [x, y, z] = WING_ZONE;
    b.inflate(Vec3::new(x, y, z)).move_by(Vec3::new(0.0, -WING_DROP, 0.0))
}

/// The box of a head's contact zone: its part box inflated by `HEAD_ZONE`.
pub fn head_box(b: Aabb) -> Aabb {
    b.inflate(Vec3::new(HEAD_ZONE, HEAD_ZONE, HEAD_ZONE))
}

/// The centre of a box on the ground plane, `(x, z)`.
pub fn centre_xz(b: Aabb) -> (f64, f64) {
    ((b.min_position().x + b.max_position().x) / 2.0, (b.min_position().z + b.max_position().z) / 2.0)
}

/// The wing's push on something at `(x, z)`: away from the body's centre `(bc, bz)`, `WING_PUSH` on the ground, `WING_LIFT` up.
pub fn push_from(x: f64, z: f64, bc: f64, bz: f64) -> [f64; 3] {
    let (xd, zd) = (x - bc, z - bz);
    let dd = (xd * xd + zd * zd).max(0.1);
    [xd / dd * WING_PUSH, WING_LIFT, zd / dd * WING_PUSH]
}
