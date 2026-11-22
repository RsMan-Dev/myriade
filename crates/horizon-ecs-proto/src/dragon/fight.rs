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
use crate::rules::fight::{off_heading, turn_accel, view_vector};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

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
    /// The fight's dragon as the save names it (`dragon_uuid`); the living one's uuid wins when it has one.
    pub dragon_uuid: Option<u128>,
    /// Ticks since the fight last saw its dragon (`dragon_unseen`): a saved dragon that is not found is replaced after 1200.
    pub dragon_unseen: u32,
    /// Ticks until the next players' scan (`player_scan`).
    pub player_scan: u64,
    pub needs_scan: bool,
    pub crystals_placed: bool,
    pub dirty: bool,
    /// The bar was shown for a living fight dragon at the last tick (`BarGone` when it goes).
    pub bar_shown: bool,
    /// `fight_load` has run for this world.
    pub loaded: bool,
    /// The fight dragon's `Silent` marker as the last tick read it: the death, which despawns the dragon, reports it.
    pub silent: bool,
}

/// What the fight keeps in its save (`ender_dragon_fight.dat`, `save_fight`): the fight's own fields, without the dragon's entity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FightSave {
    pub killed: bool,
    pub previously_killed: bool,
    pub exit_portal: Option<[i32; 3]>,
    pub gateways: Vec<i32>,
    pub respawn: Option<(RespawnStage, u32)>,
    pub respawn_crystals: Vec<[f64; 3]>,
    pub crystals_placed: bool,
    pub dragon_uuid: Option<u128>,
    pub needs_scan: bool,
}

/// The fight as the host reads it: its dragon, its spikes, its exit portal, the gateways and the respawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FightView {
    pub dragon: Option<Entity>,
    pub crystals_alive: u32,
    pub exit_portal: Option<[i32; 3]>,
    pub killed: bool,
    pub previously_killed: bool,
    pub respawn: Option<(RespawnStage, u32)>,
    pub gateways: Vec<i32>,
    pub loaded: bool,
}

/// The fight of a saved world (`load_fight`): the gateways are shuffled from `seed` when the save has none, the players are scanned at the
/// first tick.
pub fn restore(seed: i64, s: FightSave) -> EndFight {
    let gateways = if s.gateways.is_empty() { shuffled_gateways(seed) } else { s.gateways };
    EndFight {
        gateways,
        killed: s.killed,
        previously_killed: s.previously_killed,
        exit_portal: s.exit_portal,
        respawn: s.respawn,
        dragon_uuid: s.dragon_uuid,
        needs_scan: s.needs_scan,
        crystals_placed: s.crystals_placed,
        loaded: true,
        ..Default::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrystalDestroyed {
    pub crystal: Entity,
    pub at: [f64; 3],
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

/// The kill is credited (`credit_dragon_kill`, `LivingEntity.die`'s `awardKillScore`): the player who hurt the dragon within the last
/// 100 ticks, and the blow that took it down. The game awards the statistic, the score and the advancement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KillCredit {
    pub by: u32,
    pub last_blow: Cause,
    pub by_player: bool,
}

/// The fight's outgoing events, read by the game once per tick; each is pushed by the one module that owns it.
#[derive(Resource, Default)]
pub struct FightEvents {
    pub crystal_destroyed: Ev<CrystalDestroyed>,
    pub players: Ev<FightPlayersChanged>,
    pub killed: Ev<DragonKilled>,
    pub credit: Ev<KillCredit>,
}

impl FightEvents {
    /// Once per tick, with the other events (`cow::due`): what was pushed becomes readable at the next tick.
    pub fn swap(&mut self) {
        self.crystal_destroyed.swap();
        self.players.swap();
        self.killed.swap();
        self.credit.swap();
    }
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

/// The scanning dragon turns toward `pp` while it is more than 10 degrees off its heading (`Scanning`, D:2290-2330).
pub fn turn_head(w: &mut World, e: Entity, pp: Vec3) {
    let (Some(pos), Some(yaw)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Yaw>(e).map(|y| y.0)) else { return };
    if !(0.0..=10.0).contains(&off_heading(yaw, pos, pp)) {
        let h = head(w, e);
        let accel = turn_accel(yaw, w.get::<YawAccel>(e).map_or(0.0, |a| a.0), pp.x - h.x, pp.z - h.z);
        w.entity_mut(e).insert((YawAccel(accel), Yaw(yaw + accel)));
    }
}
