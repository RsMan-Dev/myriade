use crate::{collisions::SceneCollisions, containers::CreakingHeart};
use horizon_core::{
    geometry::{Aabb, Vec3},
    random::{LevelRandom, RandomSource},
};
use horizon_world::registry::BlockStateId;
use std::sync::Arc;

/// `Creaking.CREAKING_ORANGE` and `CREAKING_GRAY`.
pub const ORANGE: u32 = 16_545_810;
pub const GRAY: u32 = 6_250_335;
/// `CreakingHeartBlockEntity.DISTANCE_CREAKING_TOO_FAR`.
const TOO_FAR: f64 = 34.0;
/// `Direction.values()`: down, up, north, south, west, east.
const DIRECTIONS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];
const FACE_NAMES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];


/// The creakings a heart sees and spawns, by network id (a creaking is an ECS entity; no `Mob` is exposed).
pub trait HeartHost {
    /// A creaking that is alive and linked to a heart.
    fn present(&self, id: u32) -> bool;
    /// The loaded creaking with this UUID (its id is linked when it has none).
    fn for_uuid(&mut self, uuid: [i32; 4]) -> Option<u32>;
    fn uuid_for(&mut self, id: u32) -> [i32; 4];
    fn persistent(&self, id: u32) -> bool;
    fn feet(&self, id: u32) -> Option<Vec3>;
    fn bounds(&self, id: u32) -> Option<Aabb>;
    /// `Creaking.setTearingDown`.
    fn set_tearing_down(&mut self, id: u32);
    /// `LivingEntity.setHealth(0)`.
    fn kill(&mut self, id: u32);
    /// The creaking's own voice (`Mob.voice`).
    fn voice(&mut self, id: u32, name: &'static str);
    /// `Creaking.tearDown`: centre and spread of the crumbling box; the creaking is gone.
    fn tear_down(&mut self, id: u32) -> Option<([f64; 3], [f64; 3])>;
    /// `Creaking.playerIsStuckInYou`.
    fn player_is_stuck(&mut self, id: u32) -> bool;
    /// `Level.getNearestPlayer(at, range, false)`: a non-spectator player within `range`.
    fn player_near(&self, at: [f64; 3], range: f64) -> bool;
    /// `Level.addFreshEntity` of a creaking linked to the heart at `heart`; its id.
    fn spawn(&mut self, at: Vec3, yaw: f32, follow_bonus: f64, heart: [i32; 3]) -> Option<u32>;
    /// `ServerLevel.isSpawningMonsters` (rules and difficulty).
    fn spawning_monsters(&self) -> bool;
}

/// `Mob.center`: the middle of the box.
fn center_of(b: Aabb) -> [f64; 3] {
    let (lo, hi) = (b.min_position(), b.max_position());
    [(lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, (lo.z + hi.z) / 2.0]
}



/// `getCreakingProtector`: the protector's entity, resolving a UUID read
/// from a save (forgotten 30 ticks after the heart came back unresolved).
pub fn protector(heart: &mut CreakingHeart, host: &mut impl HeartHost) -> Option<u32> {
    let (id, uuid) = heart.creaking?;
    if let Some(id) = id {
        if host.present(id) {
            return Some(id);
        }
        // setCreakingInfo(uuid)
        heart.creaking = Some((None, uuid));
        heart.ticks_existed = 0;
    }
    if let Some(id) = host.for_uuid(uuid) {
        heart.creaking = Some((Some(id), uuid));
        return Some(id);
    }
    if heart.ticks_existed >= 30 {
        heart.creaking = None;
    }
    None
}
fn distance_to_creaking(heart: &mut CreakingHeart, pos: [i32; 3], host: &mut impl HeartHost) -> f64 {
    protector(heart, host)
        .and_then(|id| host.feet(id))
        .map_or(0.0, |p| {
            let at = Vec3::new(
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]),
                f64::from(pos[2]) + 0.5,
            );
            ((at.x - p.x).powi(2) + (at.y - p.y).powi(2) + (at.z - p.z).powi(2)).sqrt()
        })
}
/// `computeAnalogOutputSignal`.
fn output_signal(heart: &mut CreakingHeart, pos: [i32; 3], host: &mut impl HeartHost) -> i32 {
    if heart.creaking.is_none() || protector(heart, host).is_none() {
        return 0;
    }
    let scaled = distance_to_creaking(heart, pos, host).clamp(0.0, 32.0) / 32.0;
    15 - (scaled * 15.0).floor() as i32
}
/// `Util.shuffledCopy(Direction.values(), random)`.
fn shuffled_directions(lr: &mut impl RandomSource) -> [usize; 6] {
    let mut order = [0, 1, 2, 3, 4, 5];
    for i in (2..=6).rev() {
        let j = lr.next_int_bound(i as i32).unwrap_or(0) as usize;
        order.swap(i - 1, j);
    }
    order
}
fn offset(p: [i32; 3], d: usize) -> [i32; 3] {
    [
        p[0] + DIRECTIONS[d][0],
        p[1] + DIRECTIONS[d][1],
        p[2] + DIRECTIONS[d][2],
    ]
}
fn centre(p: [i32; 3]) -> [f64; 3] {
    [
        f64::from(p[0]) + 0.5,
        f64::from(p[1]) + 0.5,
        f64::from(p[2]) + 0.5,
    ]
}

/// `Mob.finalizeSpawn`: the follow range's random spawn bonus and the
/// handedness, from the level random. Gives the follow range's bonus
/// (ADD_MULTIPLIED_BASE on the base value); the handedness draw only keeps
/// the stream in step.
pub fn finalize_spawn(lr: &mut LevelRandom) -> f64 {
    let bonus = lr.triangle(0.0, 0.114_850_000_000_000_01);
    let _left_handed = lr.next_float() < 0.05;
    bonus
}
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

/// Whether this tick of the heart reaches its update (`updateCreakingState`
/// and the protector checks), the only part of [`server_tick`] that reads the
/// level's other mobs: about once a second, so a host may build them only then.
pub fn tick_updates(heart: &CreakingHeart) -> bool {
    heart.ticker < 0
}



