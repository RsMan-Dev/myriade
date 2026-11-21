//! The numbers of a ride: the jump bar, the seat of a rider, the drive of a charged, a stick or a dashing mount, the temper and the navigation.
use horizon_core::geometry::trig::Mth;
use horizon_core::geometry::Vec3;

/// `Player.jumpRidingTicks` to the scale of the jump bar (`Player.getJumpRidingScale`).
pub fn jump_bar(ticks: u32) -> f32 {
    if ticks < 10 { ticks as f32 * 0.1 } else { 0.8 + 2.0 / (ticks - 9) as f32 * 0.1 }
}
/// The seat of a rider: where the mount carries it (forward offset along the mount's yaw, a fraction of its height).
pub fn seat_point(pos: Vec3, yaw: f32, body_height: f32, offset: f32, fraction: f32) -> Vec3 {
    let a = yaw.to_radians();
    let (dx, dz) = (f64::from(-a.msin() * offset), f64::from(a.mcos() * offset));
    Vec3::new(pos.x + dx, pos.y + f64::from(body_height * fraction), pos.z + dz)
}
/// The forward input a mount takes: a quarter of a backward one.
pub fn drive_forward(forward: f32) -> f32 {
    if forward <= 0.0 { forward * 0.25 } else { forward }
}
/// The velocity of a charged mount's jump: the jump bar lifts it, and when walking forward it is pushed forward by 0,4 of the bar.
pub fn charged_jump(mut v: Vec3, yaw: f32, jump_strength: f32, jump: f32, forward: f32) -> Vec3 {
    let s = yaw.to_radians();
    v.y = f64::from(jump_strength * jump);
    if forward > 0.0 {
        v.x += f64::from(-0.4 * s.msin() * jump);
        v.z += f64::from(0.4 * s.mcos() * jump);
    }
    v
}
/// The speed factor of a stick boost at `elapsed` of `total` own ticks: `1 + gain * sin(pi * elapsed / total)`.
pub fn boost_factor(elapsed: u32, total: u32, gain: f32) -> f32 {
    1.0 + gain * (elapsed as f32 / total as f32 * std::f32::consts::PI).msin()
}
/// The velocity of a camel's dash: `push` of the speed along the look and `lift` up, scaled by the jump bar.
pub fn dash_push(vel: Vec3, yaw: f32, push: f64, lift: f64, lift_scale: f64, speed: f64, jump: f32) -> Vec3 {
    let a = yaw.to_radians();
    let (dx, dz, k) = (f64::from(-a.msin()), f64::from(a.mcos()), f64::from(jump));
    Vec3::new(vel.x + dx * push * k * speed, vel.y + lift * k * lift_scale, vel.z + dz * push * k * speed)
}
/// `AbstractHorse.modifyTemper`'s result: the amount added, held between 0 and the highest temper.
pub fn temper_after(temper: i32, amount: i32, max: i32) -> i32 {
    (temper + amount).clamp(0, max)
}
/// The squared distance between two points.
pub fn dist_sq(a: Vec3, b: Vec3) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)
}
/// Whether a creature rider's mount takes a new path: when it is idle, or every 10 ticks of its beat.
pub fn repath_due(idle: bool, beat: u64) -> bool {
    idle || beat.is_multiple_of(10)
}
/// `AbstractNautilus.getRiddenInput`: strafe, up and forward of the look's pitch, halved when going backwards.
pub fn look_input(strafe: f32, forward: f32, pitch: f32) -> (f32, f32, f32) {
    let a = pitch.to_radians();
    let k = if forward < 0.0 { -0.5 } else { 1.0 };
    let live = if forward != 0.0 { 1.0 } else { 0.0 };
    (strafe, -a.msin() * live * k, a.mcos() * live * k)
}
/// The dash of a nautilus: `push` times the jump's `power`, the movement speed and the block's speed factor, along the look (yaw and
/// pitch of the rider), added to the velocity (`AbstractNautilus.executeRidersJump`).
pub fn look_dash(vel: Vec3, yaw: f32, pitch: f32, push: f64, power: f32, speed: f64, block: f32) -> Vec3 {
    let push = push * f64::from(power) * speed * f64::from(block);
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    let c = f64::from(p.mcos());
    Vec3::new(vel.x - f64::from(y.msin()) * c * push, vel.y - f64::from(p.msin()) * push, vel.z + f64::from(y.mcos()) * c * push)
}
/// `HappyGhast.getRiddenInput`: the look's pitch gives forward and up (a half of it backwards), a held jump adds 0,5 up, all scaled by
/// 3,9 times the flying speed.
pub fn fly_input(strafe: f32, forward: f32, pitch: f32, jumping: bool, flying_speed: f64) -> (f32, f32, f32) {
    let (xxa, mut yya, zza) = look_input(strafe, forward, pitch);
    if jumping {
        yya += 0.5;
    }
    let k = (3.9_f64 * flying_speed) as f32;
    (xxa * k, yya * k, zza * k)
}
/// `HappyGhast.getRiddenSpeed`: five thirds of the flying speed.
pub fn fly_speed(flying_speed: f64) -> f32 {
    (flying_speed * 5.0 / 3.0) as f32
}
/// `getPlayerJumpPendingScale`: 0,4 to 0,8 with the charge, 1 from 90.
pub fn pending_scale(data: i32) -> f32 {
    if data >= 90 { 1.0 } else { 0.4 + 0.4 * data as f32 / 90.0 }
}
