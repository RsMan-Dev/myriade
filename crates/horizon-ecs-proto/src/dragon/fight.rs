//! The End fight's state (`Fight`, D:120-165; `load_fight` D:1118 for the gateway order) and the three events that the crystals, the
//! death and the respawn write for the rest of the game. One fight per world: a resource, written by this lot only.
use super::data::{is_sitting, HOVERING, TAKEOFF};
use super::flight::YawAccel;
use super::nodes::FlightNodes;
use super::parts::Parts;
use super::phases::{self, PhaseId};
use crate::cow::{Cause, Pos, Yaw};
use crate::goals::{Ev, Online};
use crate::hostile::PlayerState;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{trig, Vec3};

pub const GATEWAYS: i32 = 20;
/// The players of the fight are looked at every 20 ticks (`fight_has_players`, D:1329).
pub const PLAYER_SCAN: u64 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RespawnStage {
    Start,
    Preparing,
    Pillars,
    Summoning,
}

#[derive(Resource, Default, Debug)]
pub struct EndFight {
    pub dragon: Option<Entity>,
    /// Spike crystals alive: placed +1, destroyed -1 (the portal crystals of a respawn are not counted).
    pub crystals_alive: u16,
    pub exit_portal: Option<[i32; 3]>,
    /// The gateways still to pop, in the seeded order; the last one is popped first.
    pub gateways: Vec<i32>,
    pub killed: bool,
    pub previously_killed: bool,
    /// The stage and the ticks spent in it (reset on every stage change).
    pub respawn: Option<(RespawnStage, u32)>,
    /// The four portal crystals of a respawn, and the one that carries its timer.
    pub respawn_crystals: Vec<Entity>,
    pub players: bool,
    pub blast_by: Option<u32>,
    pub last_blow: Option<Cause>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrystalDestroyed {
    pub crystal: Entity,
    pub by: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FightPlayersChanged {
    pub present: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragonKilled {
    pub by: Option<u32>,
    pub last_blow: Option<Cause>,
}

/// The fight's outgoing events, read by the game once per tick; each is pushed by the one module that owns it.
#[derive(Resource, Default)]
pub struct FightEvents {
    pub crystal_destroyed: Ev<CrystalDestroyed>,
    pub players: Ev<FightPlayersChanged>,
    pub killed: Ev<DragonKilled>,
}

/// `Collections.shuffle` of the gateways 0..20 with the seeded `LegacyRandom` (`load_fight`).
pub fn shuffled_gateways(seed: i64) -> Vec<i32> {
    use horizon_core::random::{LegacyRandom, RandomSource};
    let mut r = LegacyRandom::new(seed);
    let mut v: Vec<i32> = (0..GATEWAYS).collect();
    for i in (2..=v.len()).rev() {
        let j = r.next_int_bound(i as i32).unwrap_or(0) as usize;
        v.swap(i - 1, j);
    }
    v
}

/// The fight as the world loads it: `previously_killed` comes from the exit portal's state (`end_portal_active`, D:1205).
pub fn load(seed: i64, previously_killed: bool) -> EndFight {
    EndFight { gateways: shuffled_gateways(seed), previously_killed, ..Default::default() }
}

/// The players' presence changed: one event, and only then (`fight_has_players`, D:1329).
pub fn set_players(w: &mut World, present: bool) {
    let changed = {
        let f = w.resource_mut::<EndFight>().into_inner();
        let changed = f.players != present;
        f.players = present;
        changed
    };
    if changed {
        w.resource_mut::<FightEvents>().players.push(FightPlayersChanged { present });
    }
}

/// The crystal the dragon heals from (`nearest_crystal`), rare: only the dragon of a fight with crystals carries it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NearestCrystal(pub Option<Entity>);

/// A player's crystal blow asks the holding dragon to strafe (`strafe_request`): taken at the next holding tick.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrafeRequest(pub u32);

/// The spike crystals alive (`fight.alive_crystals`), kept by events, never counted by a scan.
pub fn alive_crystals(w: &World) -> u16 {
    w.resource::<EndFight>().crystals_alive
}

/// `FightPlayer.enemy` and `Level.getNearestPlayer`: the targetable player nearest `at`, within `range` of `at` and at most `dy`
/// above or below it; none on Peaceful. Players are read in id order (the first wins a tie).
pub fn nearest_enemy(w: &World, at: Vec3, range: Option<f64>, dy: Option<f64>) -> Option<(u32, Vec3)> {
    let (on, ps) = (w.resource::<Online>(), w.resource::<PlayerState>());
    if ps.peaceful {
        return None;
    }
    on.players()
        .filter(|p| ps.targetable(p.id))
        .map(|p| (p.id, Vec3::new(p.x, p.y, p.z)))
        .filter(|(_, p)| range.is_none_or(|r| p.distance_squared(at) <= r * r) && dy.is_none_or(|d| (p.y - at.y).abs() <= d))
        .min_by(|a, b| a.1.distance_squared(at).total_cmp(&b.1.distance_squared(at)))
}

/// `FightPlayer.enemy`: a targetable player, not on Peaceful.
pub fn is_enemy(w: &World, id: u32) -> bool {
    let ps = w.resource::<PlayerState>();
    !ps.peaceful && ps.targetable(id)
}

/// The feet of player `id`, if it is in the level.
pub fn player_pos(w: &World, id: u32) -> Option<Vec3> {
    let p = w.resource::<Online>().get(id)?;
    Some(Vec3::new(p.x, p.y, p.z))
}

/// The eye of player `id` (`box top - 0.18`).
pub fn eye_y(w: &World, id: u32) -> f64 {
    w.resource::<Online>().get(id).map_or(0.0, |p| p.y + 1.62)
}

/// `Entity.hasLineOfSight`, blocks only.
pub fn line_of_sight(w: &World, from: Vec3, to: Vec3) -> bool {
    crate::blocks::world_of(w).line_of_sight(from, to)
}

pub fn is_air(w: &World, x: i32, y: i32, z: i32) -> bool {
    crate::level::block_is_air(w, [x, y, z])
}

/// The dragon's head part (`parts[0]`).
pub fn head(w: &World, e: Entity) -> Vec3 {
    w.get::<Parts>(e).map_or(Vec3::ZERO, |p| Vec3::new(p.0[0].pos[0], p.0[0].pos[1], p.0[0].pos[2]))
}

pub fn yaw(w: &World, e: Entity) -> f32 {
    w.get::<Yaw>(e).map_or(0.0, |y| y.0)
}

/// `view_vector(yaw, pitch)` of Horizon's `game.rs`: plain `f64` trigonometry.
fn view_vector(yaw: f32, pitch: f32) -> Vec3 {
    let (y, p) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    Vec3::new(-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos())
}

/// The view vector at pitch 0 (the fireball leaves one block back along it).
pub fn view(w: &World, e: Entity) -> Vec3 {
    view_vector(yaw(w, e), 0.0)
}

/// `dragon_look` (D:2646): the view vector, tipped down on a landing or takeoff by the distance to the podium, and 45 degrees
/// down when perched.
pub fn look(w: &World, e: Entity) -> Vec3 {
    let id = w.get::<PhaseId>(e).map_or(HOVERING, |p| p.0);
    let pitch = if id == phases::LANDING || id == TAKEOFF {
        let (p, here) = (w.resource::<FlightNodes>().podium(), w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0));
        let dist = ((p.x - here.x).powi(2) + (p.y - here.y).powi(2) + (p.z - here.z).powi(2)).sqrt() as f32 / 4.0;
        -6.0 / dist.max(1.0) * 1.5 * 5.0
    } else if is_sitting(id) {
        -45.0
    } else {
        0.0
    };
    view_vector(yaw(w, e), pitch)
}

fn unit3(x: f64, y: f64, z: f64) -> [f64; 3] {
    let len = (x * x + y * y + z * z).sqrt();
    if len < f64::from(1.0e-5_f32) { [0.0; 3] } else { [x / len, y / len, z / len] }
}

fn wrap64(d: f64) -> f64 {
    let mut r = d % 360.0;
    if r >= 180.0 {
        r -= 360.0;
    }
    if r < -180.0 {
        r += 360.0;
    }
    r
}

/// The scanning dragon turns toward `pp` while it is more than 10 degrees off its heading (`Scanning`, D:2290-2330).
pub fn turn_head(w: &mut World, e: Entity, pp: Vec3) {
    let (Some(pos), Some(yaw)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Yaw>(e).map(|y| y.0)) else { return };
    let aim = unit3(pp.x - pos.x, 0.0, pp.z - pos.z);
    let r = f64::from(yaw * trig::DEG_TO_RAD);
    let dir = unit3(f64::from(trig::sin(r)), 0.0, f64::from(-trig::cos(r)));
    let dot = (dir[0] * aim[0] + dir[1] * aim[1] + dir[2] * aim[2]) as f32;
    let angle = (f64::from(dot).acos() * 180.0 / f64::from(std::f32::consts::PI)) as f32 + 0.5;
    if !(0.0..=10.0).contains(&angle) {
        let h = head(w, e);
        let (xd, zd) = (pp.x - h.x, pp.z - h.z);
        let delta = wrap64(180.0 - trig::atan2(xd, zd) * 180.0 / f64::from(std::f32::consts::PI) - f64::from(yaw)).clamp(-100.0, 100.0);
        let mut accel = w.get::<YawAccel>(e).map_or(0.0, |a| a.0);
        accel *= 0.8;
        let dist = ((xd * xd + zd * zd).sqrt() as f32 + 1.0).min(40.0);
        accel += delta as f32 * (0.7 / dist / dist);
        w.entity_mut(e).insert((YawAccel(accel), Yaw(yaw + accel)));
    }
}
