//! The breeze's shot, slide and long jump (`brain/behaviours/breeze.rs`): the firing point, the aim, the wind charge's spread, the point
//! behind a target, the middle circle, the jump's reach and landing. The draws are passed in, in the order the behaviour takes them.
use myriade_engine_lib::blocks::Fluid;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

/// The wind charge's speed (`BreezeUtil`'s wind charge) and the spread of its aim, in degrees.
pub const SHOT_SPEED: f64 = 0.7;
pub const SHOT_SPREAD: f64 = 3.0;
/// The angles of the long jump, shuffled before the first that reaches the target.
pub const ALLOWED_ANGLES: [i32; 5] = [40, 55, 60, 75, 80];

/// The block cell of a point.
pub fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

/// `Breeze.getFiringYPosition`: half the height above the feet, and 0.3 more.
pub fn firing_y(pos_y: f64, height: f32) -> f64 {
    pos_y + f64::from(height) / 2.0 + f64::from(0.3_f32)
}

/// `BreezeUtil.hasLineOfSight`'s reach: 50, or the follow range when it is longer.
pub fn los_range(follow: f64) -> f64 {
    50.0_f64.max(follow)
}

/// The aim of a shot from `from` at a target at `target`, `height` tall: towards its upper part (0.3 of its height).
pub fn aim(target: Vec3, height: f64, firing_y: f64, from: Vec3) -> Vec3 {
    Vec3::new(target.x - from.x, target.y + height * 0.3 - firing_y, target.z - from.z)
}

/// The velocity of a wind charge aimed along `d`, with the spread: six uniform draws, in order (x, y, z, each a difference of two).
pub fn wind_shot(d: Vec3, draws: [f64; 6]) -> Vec3 {
    let len = d.length().max(1.0e-9);
    let spread = 0.017_227_5 * SHOT_SPREAD;
    let tri = |a: f64, b: f64| 0.0 + spread * (a - b);
    Vec3::new(
        (d.x / len + tri(draws[0], draws[1])) * SHOT_SPEED,
        (d.y / len + tri(draws[2], draws[3])) * SHOT_SPEED,
        (d.z / len + tri(draws[4], draws[5])) * SHOT_SPEED,
    )
}

/// `BreezeUtil.randomPointBehindTarget`: a point 4 to 8 blocks behind a target at `target`, facing `yaw` degrees, the angle a normal
/// draw (`u`, `v` uniforms, Box-Muller) around its back; `f` is the distance's float.
pub fn behind_point(target: Vec3, yaw: f64, u: f64, v: f64, f: f32) -> Vec3 {
    let u = u.max(1.0e-12);
    let gauss = (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos();
    let view = (yaw + 180.0 + gauss * 90.0 / 2.0).to_radians();
    let r = f64::from(4.0 + f * (8.0 - 4.0));
    Vec3::new(target.x - view.sin() * r, target.y, target.z + view.cos() * r)
}

/// `Slide.start`'s middle circle: a point at 8 to 4 blocks from `pos` towards `target`, `a` being the uniform that picks the distance.
pub fn middle_circle(pos: Vec3, target: Vec3, a: f64) -> Vec3 {
    let d = Vec3::new(target.x - pos.x, target.y - pos.y, target.z - pos.z);
    let distance = d.length() - (8.0 + a * (4.0 - 8.0));
    let n = d.normalize();
    Vec3::new(pos.x + n.x * distance, pos.y + n.y * distance, pos.z + n.z * distance)
}

/// `Slide.start`: the target is near enough (16 squared, 10 vertically) for the slide to go away from it.
pub fn near_for_away(dx: f64, dy: f64, dz: f64) -> bool {
    dx * dx + dz * dz < 16.0 && dy.abs() < 10.0
}

/// Whether the point `away` is farther from `target` than `pos` is.
pub fn is_further(target: Vec3, away: Vec3, pos: Vec3) -> bool {
    target.distance_squared(away) > target.distance_squared(pos)
}

/// `LongJump.canJumpFromCurrentPosition`: not on honey, and four blocks of air or water above `p`.
pub fn can_jump_from(mw: &MobWorld, p: [i32; 3]) -> bool {
    if mw.block_name(p[0], p[1], p[2]) == "minecraft:honey_block" {
        return false;
    }
    (1..=4).all(|i| mw.facts.is_air(mw.world.block(p[0], p[1] + i, p[2])) || mw.fluid(p[0], p[1] + i, p[2]) == Fluid::Water)
}

/// `LongJump.snapToSurface`: the block above where a clip ten blocks down hits, else ten blocks up.
pub fn snap_to_surface(mw: &MobWorld, p: Vec3) -> Option<[i32; 3]> {
    let above = |h: Vec3| [h.x.floor() as i32, h.y.floor() as i32 + 1, h.z.floor() as i32];
    mw.clip_block(p, Vec3::new(p.x, p.y - 10.0, p.z)).map(above).or_else(|| mw.clip_block(p, Vec3::new(p.x, p.y + 10.0, p.z)).map(above))
}

/// `EntityType.isBlockDangerous` for a creature that is not fire immune.
pub fn dangerous(name: &str) -> bool {
    matches!(
        name,
        "minecraft:fire" | "minecraft:soul_fire" | "minecraft:lava" | "minecraft:magma_block" | "minecraft:lava_cauldron" | "minecraft:wither_rose" | "minecraft:sweet_berry_bush" | "minecraft:cactus" | "minecraft:powder_snow"
    )
}

/// `LongJump.canRun`: floating (water above 0.4, or lava) is no jump.
pub fn floats(water: f64, in_lava: bool) -> bool {
    water > 0.4 || in_lava
}

/// `LongJump.canRun`: the target has gone beyond the follow range.
pub fn out_of_follow(target: Vec3, pos: Vec3, follow: f64) -> bool {
    target.distance_squared(pos) >= follow * follow
}

/// `LongJump.canRun`: the target is within 4 blocks.
pub fn too_close(target: Vec3, pos: Vec3) -> bool {
    target.distance(pos) as f32 - 4.0 <= 0.0
}

/// `LongJump.calculateOptimalJumpVector`: the largest velocity of the arc, from the follow range.
pub fn jump_max_velocity(follow: f64) -> f32 {
    0.058_333_334_f32 * follow as f32
}

/// The cooldown set when the jump lands: 2 ticks when hurt, else 10.
pub fn landing_cooldown(hurt: bool) -> u64 {
    if hurt { 2 } else { 10 }
}
