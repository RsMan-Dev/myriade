//! The End fight's state (`Fight`, D:120-165; `load_fight` D:1118 for the gateway order) and the three events that the crystals, the
//! death and the respawn write for the rest of the game. One fight per world: a resource, written by this lot only.
use crate::cow::Cause;
use crate::goals::Ev;
use bevy_ecs::prelude::*;

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
