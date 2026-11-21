//! The numbers of a firework rocket (`FireworkRocketEntity`): its box, its glide, its blast and its rise.
use horizon_client_lib::arrows::clip_block_hit;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};

/// The box of a rocket: 0.25 wide and high.
const SIZE: f64 = 0.25;

/// The life in ticks of a rocket: `10 * (1 + flight)` and the two draws of the life.
pub fn life_ticks(flight: u8, first: i32, second: i32) -> i32 {
    10 * (1 + i32::from(flight)) + first + second
}

/// `Entity.move` box of a rocket at `at`.
pub fn box_at(at: Vec3) -> Aabb {
    let w = SIZE / 2.0;
    Aabb::new(Vec3::new(at.x - w, at.y, at.z - w), Vec3::new(at.x + w, at.y + SIZE, at.z + w))
}

/// `ServerPlayer.travel`'s boost of a glider: the holder's velocity, plus its look (0.1), plus the look at 1.5 minus the velocity,
/// half (`FireworkRocketEntity.tick`, attached to a gliding player).
pub fn glide_boost(v: Vec3, look: Vec3) -> Vec3 {
    let axis = |m: f64, l: f64| m + l * 0.1 + (l * 1.5 - m) * 0.5;
    Vec3::new(axis(v.x, look.x), axis(v.y, look.y), axis(v.z, look.z))
}

/// `dealExplosionDamage`'s amount for a target at `to`: `damage * sqrt((5 - distance) / 5)`, the distance in floats.
pub fn blast_damage(damage: f32, from: Vec3, to: Vec3) -> f32 {
    let (x, y, z) = ((from.x - to.x) as f32, (from.y - to.y) as f32, (from.z - to.z) as f32);
    let distance = f64::from(x * x + y * y + z * z).sqrt() as f32;
    damage * ((5.0 - f64::from(distance)) / 5.0).sqrt() as f32
}

/// The blast's base damage: `5 + 2 * explosions`, given to the holder and scaled by distance for the others.
pub fn detonation_damage(explosions: u8) -> f32 {
    5.0 + f32::from(explosions) * 2.0
}

/// The blast's reach to a target with its feet at `feet`: within 5 blocks, and seen from the blast at its feet or its middle
/// (`ClipContext` collider, no fluids). Returns the damage of `damage` at that distance, when it reaches.
pub fn exposure(mw: &MobWorld, from: Vec3, feet: Vec3, height: f64, damage: f32) -> Option<f32> {
    let d2 = (feet.x - from.x).powi(2) + (feet.y - from.y).powi(2) + (feet.z - from.z).powi(2);
    if d2 > 25.0 {
        return None;
    }
    let sees = (0..2).any(|step| {
        let to = Vec3::new(feet.x, feet.y + height * 0.5 * f64::from(step), feet.z);
        clip_block_hit(mw.world, mw.facts, mw.collision, from, Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z)).is_none()
    });
    sees.then(|| blast_damage(damage, from, feet))
}

/// The margin of a rocket's hit box, growing with its age from the third tick to 0.3 blocks.
pub fn hit_margin(age: u32) -> f64 {
    f64::from(((age as i32 - 2) as f32 / 20.0).clamp(0.0, 0.3))
}

/// The motion of a rocket that rises: it speeds up (1.15 sideways, 1.0 after a sideways stop) and rises 0.04.
pub fn rise_motion(vel: Vec3, horizontal: bool) -> Vec3 {
    let f = if horizontal { 1.0 } else { 1.15 };
    Vec3::new(vel.x * f, vel.y + 0.04, vel.z * f)
}

/// Whether a move stopped the rocket sideways: its motion and its move differ on x or z.
pub fn is_sideways_stop(motion: Vec3, moved: Vec3) -> bool {
    let differs = |a: f64, b: f64| (b - a).abs() >= f64::from(1.0e-5_f32);
    differs(motion.x, moved.x) || differs(motion.z, moved.z)
}
