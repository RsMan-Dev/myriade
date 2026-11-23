use crate::health::DamageLevel;
use crate::projectiles::{Projectile, ProjectileKind};
use horizon_core::{
    geometry::{Vec3, trig},
    random::{LegacyRandom, RandomSource},
};
use myriade_types::SoundSource;

/// `Activity` (the ones these brains use).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Core,
    Idle,
    Fight,
}
impl Act {
    fn slot(self) -> u8 {
        match self {
            Act::Core => 1,
            Act::Idle => 5,
            Act::Fight => 14,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Act::Core => "core",
            Act::Idle => "idle",
            Act::Fight => "fight",
        }
    }
}


/// The sensors, in `BRAIN_PROVIDER` order (all scan every 20 ticks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sensor {
    NearestLiving,
    HurtBy,
    NearestPlayers,
    BreezeAttack,
}






// ---- Sensors ------------------------------------------------------------


// ---- Behaviours ---------------------------------------------------------

/// `StartAttacking`'s target finders.
#[derive(Clone, Copy, Debug)]
enum Finder {
    /// The breeze's `NEAREST_ATTACKABLE`.
    NearestAttackable,
    /// The breeze's `HURT_BY` source, if living.
    HurtBy,
    /// The creaking's `NEAREST_VISIBLE_ATTACKABLE_PLAYER`, while active.
    ActiveCreaking,
}




fn manhattan_sq(a: [i32; 3], b: [i32; 3]) -> i64 {
    let d = [
        i64::from(a[0] - b[0]),
        i64::from(a[1] - b[1]),
        i64::from(a[2] - b[2]),
    ];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}
fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

// ---- The breeze's behaviours --------------------------------------------

/// `Mth.wrapDegrees(float)`.
fn wrap_degrees(angle: f32) -> f32 {
    let mut w = angle % 360.0;
    if w >= 180.0 {
        w -= 360.0;
    }
    if w < -180.0 {
        w += 360.0;
    }
    w
}
/// The attack target's position, eye height, facing and height.
struct Quarry {
    position: Vec3,
    head_yaw: f32,
    height: f64,
    passenger: bool,
}

// ---- The packages -------------------------------------------------------



// ---- The probe's view ---------------------------------------------------

fn pos(p: [i32; 3]) -> String {
    format!("{},{},{}", p[0], p[1], p[2])
}
