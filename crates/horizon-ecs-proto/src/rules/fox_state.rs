//! The fox's shared state: the ticks of its crouch, the tick where its selector asks, the wait of a retry, and the path clear to a point.
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

/// Ticks of crouch after which `Fox.isFullyCrouched` holds: the amount goes up 0,2 a tick from 0 and stops at 5
/// (`fox.rs:351-358`), computed with the same float steps as the `Mob`.
pub fn crouch_ticks_to_full() -> u64 {
    let (mut v, mut n) = (0.0_f32, 0u64);
    while v != 5.0 {
        v = (v + 0.2).min(5.0);
        n += 1;
    }
    n
}
/// A tick where the `Mob`'s selector asks `canUse` and ticks the goal (`ai.rs:2870`): tick + id even; the first ticks too.
pub fn is_selector_tick(now: u64, phase: u64) -> bool {
    now <= 1 || (now + phase) % 2 == 0
}
/// The wait of a retry: at least one tick.
pub fn retry_wait(wait: i32) -> u64 {
    wait.max(1) as u64
}
/// `Fox.isPathClear`: six points on the way to `to`, each with only replaceable blocks in the three above it.
pub fn path_clear(mw: &MobWorld, from: Vec3, to: Vec3) -> bool {
    let (zdiff, xdiff) = (to.z - from.z, to.x - from.x);
    let slope = zdiff / xdiff;
    for i in 0..6 {
        let step = f64::from(i as f32 / 6.0);
        let z = if slope == 0.0 { 0.0 } else { zdiff * step };
        let x = if slope == 0.0 { xdiff * step } else { z / slope };
        for j in 1..4 {
            let at = [(from.x + x).floor() as i32, (from.y + f64::from(j)).floor() as i32, (from.z + z).floor() as i32];
            if !mw.facts.get(mw.world.block(at[0], at[1], at[2])).replaceable {
                return false;
            }
        }
    }
    true
}
