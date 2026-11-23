//! The warden (`Warden`, `AngerManagement`): anger kept per entity,
//! vibration listening, the sonic boom and melee attack, the heartbeat and
//! tendril animations, the model and its emissive layers. Its brain
//! (`WardenAi`) is in `warden_brain`.
use super::{
    Damage, DamageKind, MobEvent, MobWorld, Species, TargetChange, Who,
    anim::{Poses, State},
    keyframes::Animation,
    path::PathType,
    warden_brain::{self, Brain},
};
use crate::health::DamageLevel;
use crate::sculk::{self, Context, Info};
use horizon_core::{
    geometry::{Vec3, trig::Mth},
    random::RandomSource,
};
use horizon_protocol::entity_data::PositionSource;
use std::{
    collections::{BTreeMap, HashMap},
    f32::consts::PI,
};

pub const EMERGE_DURATION: i64 = warden_brain::EMERGE_DURATION as i64;
pub const DIGGING_COOLDOWN: i64 = warden_brain::DIGGING_COOLDOWN;
const DEFAULT_ANGER: i32 = 35;
const PROJECTILE_ANGER: i32 = 10;
const ON_HURT_ANGER_BOOST: i32 = 20;
const MAX_ANGER: i32 = 150;
const DARKNESS_RADIUS: f64 = 20.0;
/// Darkness lasts 260 ticks and is renewed within the last 200.
pub const DARKNESS_DURATION: i32 = 260;
pub const DARKNESS_REAPPLY: i32 = 200;

// AnimState keyframe slots.
pub const ROAR: usize = 0;
pub const SNIFF: usize = 1;
pub const EMERGE: usize = 2;
pub const DIGGING: usize = 3;
pub const ATTACK: usize = 4;
pub const SONIC_BOOM: usize = 5;
// AnimState values: tendril and heart animation (ticks, 10 at the start).
pub const TENDRIL: usize = 0;
pub const HEART: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pose {
    #[default]
    Standing,
    Emerging,
    Digging,
    Roaring,
    Sniffing,
}
impl Pose {
    fn code(self) -> i32 {
        match self {
            Self::Standing => 0,
            Self::Emerging => 1,
            Self::Digging => 2,
            Self::Roaring => 3,
            Self::Sniffing => 4,
        }
    }
    fn from_code(code: i32) -> Self {
        match code {
            1 => Self::Emerging,
            2 => Self::Digging,
            3 => Self::Roaring,
            4 => Self::Sniffing,
            _ => Self::Standing,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AngerLevel {
    Calm,
    Agitated,
    Angry,
}
impl AngerLevel {
    pub fn by_anger(anger: i32) -> Self {
        if anger >= 80 {
            Self::Angry
        } else if anger >= 40 {
            Self::Agitated
        } else {
            Self::Calm
        }
    }
    pub fn minimum(self) -> i32 {
        match self {
            Self::Calm => 0,
            Self::Agitated => 40,
            Self::Angry => 80,
        }
    }
    pub fn is_angry(self) -> bool {
        self == Self::Angry
    }
    pub fn ambient_sound(self) -> &'static str {
        match self {
            Self::Calm => "entity.warden.ambient",
            Self::Agitated => "entity.warden.agitated",
            Self::Angry => "entity.warden.angry",
        }
    }
    pub fn listening_sound(self) -> &'static str {
        match self {
            Self::Calm => "entity.warden.listening",
            _ => "entity.warden.listening_angry",
        }
    }
}

/// `AngerManagement`: anger per suspect, highest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AngerManagement {
    suspects: Vec<Who>,
    anger: HashMap<Who, i32>,
    /// `angerByUuid`: the suspects that are not in the level, kept by UUID so
    /// their anger goes on calming down while they are away and finds them
    /// again when they come back. `Who` cannot carry one of those (its mobs are
    /// live ids), which is why a save writes this half as UUIDs.
    unloaded: BTreeMap<[i32; 4], i32>,
    /// `conversionDelay`: the anger ticks left before `unloaded` is looked up
    /// in the level again. Vanilla draws the first value from a thread-local
    /// `RandomSource` (0..=2 ticks); a fresh anger starts at 0 here, the fastest
    /// of those, since the mob layer has no such stream and drawing from the
    /// mob's own random would shift that stream's parity.
    conversion_delay: i32,
    highest: i32,
}
/// `AngerManagement.CONVERSION_DELAY`: anger ticks between two lookups of the
/// suspects that are away.
const CONVERSION_DELAY: i32 = 2;
impl AngerManagement {
    /// `new AngerManagement(filter, List<Pair<UUID, Integer>>)`: a saved anger
    /// starts with every suspect in the UUID map. The live map fills up again as
    /// the ticks find them in the level (`convertFromUuids`).
    pub fn from_saved(pairs: impl IntoIterator<Item = ([i32; 4], i32)>) -> Self {
        Self {
            unloaded: pairs.into_iter().collect(),
            ..Self::default()
        }
    }
    /// The suspects that are not in the level, by UUID, angriest first: what a
    /// save writes after the loaded ones (`createUuidAngerPairs` lists the
    /// suspects, then this map).
    pub fn unloaded(&self) -> impl Iterator<Item = ([i32; 4], i32)> + '_ {
        let mut pairs: Vec<([i32; 4], i32)> = self.unloaded.iter().map(|(u, a)| (*u, *a)).collect();
        pairs.sort_by_key(|(_, anger)| std::cmp::Reverse(*anger));
        pairs.into_iter()
    }
    /// One anger tick (`Warden.customServerAiStep` runs it every 20 game
    /// ticks): the UUIDs that are due are looked up in the level, then every
    /// suspect calms by one - the ones that are away, then the ones in the
    /// level, whose invalid are forgotten. `by_uuid` is `Level.getEntity(uuid)`
    /// as a `Who` (`Game::who_by_uuid`); a world that cannot answer resolves
    /// nothing, so those suspects stay away for another tick.
    pub fn tick(&mut self, valid: impl Fn(Who) -> bool, by_uuid: impl Fn([i32; 4]) -> Option<Who>) {
        self.conversion_delay -= 1;
        if self.conversion_delay <= 0 {
            self.convert_from_uuids(by_uuid);
            self.conversion_delay = CONVERSION_DELAY;
        }
        self.unloaded.retain(|_, anger| {
            if *anger <= 1 {
                return false;
            }
            *anger -= 1;
            true
        });
        let mut dropped = Vec::new();
        for (who, anger) in self.anger.iter_mut() {
            if *anger > 1 && valid(*who) {
                *anger -= 1;
            } else {
                dropped.push(*who);
            }
        }
        for who in dropped {
            self.anger.remove(&who);
            self.suspects.retain(|s| *s != who);
        }
        self.sort_and_update_highest();
    }
    /// `AngerManagement.convertFromUuids`: a suspect that is back in the level
    /// rejoins the live map with the anger it kept while it was away. One that
    /// is already in the live map was angered again before this lookup: vanilla's
    /// `increaseAnger` would have added the anger kept under its UUID then
    /// (`angerByUuid.removeInt`, uncapped), which the mob layer cannot do as it
    /// knows no UUIDs, so that anger is added here.
    fn convert_from_uuids(&mut self, by_uuid: impl Fn([i32; 4]) -> Option<Who>) {
        let away = std::mem::take(&mut self.unloaded);
        for (uuid, anger) in away {
            match by_uuid(uuid) {
                Some(who) if self.anger.contains_key(&who) => {
                    *self.anger.entry(who).or_default() += anger;
                }
                Some(who) => {
                    self.anger.insert(who, anger);
                    self.suspects.push(who);
                }
                None => {
                    self.unloaded.insert(uuid, anger);
                }
            }
        }
    }
    fn sort_and_update_highest(&mut self) {
        // AngerManagement.Sorter: the angry first, then players, then the
        // angriest; a stable sort keeps the order of ties.
        let anger = self.anger.clone();
        let level = |w: &Who| anger.get(w).copied().unwrap_or(0);
        self.suspects.sort_by(|a, b| {
            let (aa, ab) = (level(a), level(b));
            let (angry_a, angry_b) = (
                AngerLevel::by_anger(aa).is_angry(),
                AngerLevel::by_anger(ab).is_angry(),
            );
            if angry_a != angry_b {
                return if angry_a {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            let (player_a, player_b) = (*a == Who::Player, *b == Who::Player);
            if player_a != player_b {
                return if player_a {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            ab.cmp(&aa)
        });
        self.highest = match self.suspects.len() {
            0 => 0,
            1 => level(&self.suspects[0]),
            _ => self.suspects.iter().map(&level).max().unwrap_or(0),
        };
    }
    /// `increaseAnger`: the entity's new anger (at most 150).
    pub fn increase(&mut self, who: Who, amount: i32) -> i32 {
        let new_suspect = !self.anger.contains_key(&who);
        let current = (self.anger.get(&who).copied().unwrap_or(0) + amount).min(MAX_ANGER);
        self.anger.insert(who, current);
        if new_suspect {
            self.suspects.push(who);
        }
        self.sort_and_update_highest();
        current
    }
    pub fn clear(&mut self, who: Who) {
        self.anger.remove(&who);
        self.suspects.retain(|s| *s != who);
        self.sort_and_update_highest();
    }
    /// `getActiveAnger`: the target's anger, or the highest when there is none.
    pub fn active_anger(&self, target: Option<Who>) -> i32 {
        match target {
            None => self.highest,
            Some(t) => self.anger.get(&t).copied().unwrap_or(0),
        }
    }
    /// `getActiveEntity`: the first suspect that can still be targeted.
    pub fn active_entity(&self, can_target: impl Fn(Who) -> bool) -> Option<Who> {
        self.suspects.iter().copied().find(|w| can_target(*w))
    }
    pub fn anger_of(&self, who: Who) -> i32 {
        self.anger.get(&who).copied().unwrap_or(0)
    }
    pub fn suspects(&self) -> &[Who] {
        &self.suspects
    }
}

/// The warden's state: anger, its vibration listener and its brain.
#[derive(Clone, Debug, Default)]
pub struct Warden {
    pub anger: AngerManagement,
    pub vibration: sculk::Data,
    pub brain: Brain,
    /// `getClientAngerLevel`, synchronised once a second.
    pub client_anger: i32,
    /// Attackers noted by `hurtServer` and not yet reacted to.
    hurt_by: Vec<(Option<Who>, bool)>,
}
impl Warden {
    pub fn target(&self) -> Option<Who> {
        self.brain.mem.attack_target.value()
    }
}


// ---- entity facts ----

/// `Entity.isAlliedTo`: `source` on `warden`'s team (`Team.isAlliedTo`).
fn allied(teams: &super::Teams<'_>, warden: u32, source: Option<Who>) -> bool {
    let Some(source) = source else {
        return false;
    };
    crate::scoreboard::teams_allied(teams.of(Who::Mob(warden)), teams.of(source))
}
/// A `GameEvent.Context` as this warden reads it: `canReceiveVibration`
/// refuses a living source its `canTargetEntity` rejects, allies included.
pub fn heard_by(ctx: Context, teams: &super::Teams<'_>, warden: u32) -> Context {
    Context {
        targetable: ctx.targetable
            && !allied(teams, warden, ctx.source.and_then(|source| source.actor())),
        ..ctx
    }
}


/// `Warden.canTargetEntity`: living, neither creative nor spectator, not a
/// warden, not an ally, not invulnerable, alive.
pub fn can_target(w: &MobWorld, warden: u32, who: Who) -> bool {
    let targetable = match who {
        Who::Player => w
            .selected_player()
            .is_some_and(|p| p.alive && !p.creative && !p.spectator),
        Who::Mob(id) => w
            .seen(id)
            .is_some_and(|s| s.alive && s.species != Species::Warden && !s.invulnerable),
    };
    targetable && !allied(&w.teams, warden, Some(who))
}
fn position_of(w: &MobWorld, who: Who) -> Option<Vec3> {
    w.target_info(who).map(|t| t.position)
}

// ---- anger ----


// ---- damage ----



// ---- vibrations ----


// ---- the server tick ----

/// `Warden.getHeartBeatDelay`.
pub fn heart_beat_delay(client_anger: i32) -> i32 {
    let anger = client_anger as f32 / AngerLevel::Angry.minimum() as f32;
    40 - (anger.clamp(0.0, 1.0) * 30.0).floor() as i32
}

// ---- model ----


#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn anger_levels_follow_the_thresholds() {
        assert_eq!(AngerLevel::by_anger(0), AngerLevel::Calm);
        assert_eq!(AngerLevel::by_anger(39), AngerLevel::Calm);
        assert_eq!(AngerLevel::by_anger(40), AngerLevel::Agitated);
        assert_eq!(AngerLevel::by_anger(79), AngerLevel::Agitated);
        assert_eq!(AngerLevel::by_anger(80), AngerLevel::Angry);
        assert!(AngerLevel::Angry.is_angry());
        assert_eq!(AngerLevel::Calm.ambient_sound(), "entity.warden.ambient");
        assert_eq!(
            AngerLevel::Agitated.listening_sound(),
            "entity.warden.listening_angry"
        );
    }

    #[test]
    fn a_suspect_angered_again_before_its_lookup_keeps_its_saved_anger() {
        // `increaseAnger` on a new suspect adds what `angerByUuid` kept for it:
        // a reloaded warden angered at the player again before the first lookup
        // keeps both angers, not only the new one.
        let player = [5, 5, 5, 5];
        let mut a = AngerManagement::from_saved([(player, 95)]);
        assert_eq!(a.increase(Who::Player, 35), 35);
        a.tick(|_| true, |u| (u == player).then_some(Who::Player));
        // 35 + 95 = 130, then one anger tick calms it by one.
        assert_eq!(a.anger_of(Who::Player), 129);
        assert_eq!(a.unloaded().count(), 0);
        assert_eq!(a.suspects(), [Who::Player]);
    }

    #[test]
    fn anger_accumulates_to_a_cap_and_decays_each_second() {
        let mut a = AngerManagement::default();
        assert_eq!(a.increase(Who::Player, 35), 35);
        assert_eq!(a.increase(Who::Player, 35), 70);
        assert_eq!(a.increase(Who::Player, 100), 150);
        assert_eq!(a.active_anger(None), 150);
        a.tick(|_| true, |_| None);
        assert_eq!(a.anger_of(Who::Player), 149);
        // An invalid suspect is forgotten.
        a.increase(Who::Mob(3), 10);
        a.tick(|w| w == Who::Player, |_| None);
        assert_eq!(a.anger_of(Who::Mob(3)), 0);
        assert_eq!(a.suspects(), &[Who::Player]);
        // Anger of one point is gone after the next second.
        let mut b = AngerManagement::default();
        b.increase(Who::Mob(1), 1);
        b.tick(|_| true, |_| None);
        assert!(b.suspects().is_empty());
    }

    #[test]
    fn the_angriest_player_comes_first() {
        let mut a = AngerManagement::default();
        a.increase(Who::Mob(1), 90);
        a.increase(Who::Player, 50);
        a.increase(Who::Mob(2), 120);
        // Angry suspects first, the angrier mob before the weaker one...
        assert_eq!(a.suspects(), &[Who::Mob(2), Who::Mob(1), Who::Player]);
        // ...until the player is angry as well: players lead ties of level.
        a.increase(Who::Player, 40);
        assert_eq!(a.suspects()[0], Who::Player);
        assert_eq!(a.active_entity(|w| w != Who::Player), Some(Who::Mob(2)));
        assert_eq!(a.active_anger(Some(Who::Mob(1))), 90);
        assert_eq!(a.active_anger(Some(Who::Mob(9))), 0);
        a.clear(Who::Player);
        assert_eq!(a.suspects(), &[Who::Mob(2), Who::Mob(1)]);
        assert_eq!(a.active_anger(None), 120);
    }

    #[test]
    fn a_single_suspect_sets_the_highest_anger() {
        let mut a = AngerManagement::default();
        a.increase(Who::Mob(5), 44);
        assert_eq!(a.active_anger(None), 44);
        a.clear(Who::Mob(5));
        assert_eq!(a.active_anger(None), 0);
    }

    #[test]
    fn the_heart_beats_faster_when_angry() {
        assert_eq!(heart_beat_delay(0), 40);
        assert_eq!(heart_beat_delay(40), 25);
        assert_eq!(heart_beat_delay(80), 10);
        assert_eq!(heart_beat_delay(150), 10);
    }

    #[test]
    fn the_pose_codes_round_trip() {
        for p in [
            Pose::Standing,
            Pose::Emerging,
            Pose::Digging,
            Pose::Roaring,
            Pose::Sniffing,
        ] {
            assert_eq!(Pose::from_code(p.code()), p);
        }
    }
}

#[cfg(test)]
mod digging_tests {
    use super::*;
    use crate::mobs::MobTypes;
    use std::sync::Arc;


    /// `Warden.clientDiggingParticles` (#8614): while the emerging or digging
    /// animation is under 4500 ms the client wants 30 block particles a tick,
    /// so `client_tick` has to hand the event to the caller instead of
    /// dropping it.
    #[test]
    fn an_emerging_warden_hands_its_digging_particles_back() {
        let Some(mut m) = mob("minecraft:warden") else {
            return;
        };
        let mut out = Vec::new();
        // The client moves the age on before it ticks (`tickCount++`), so a
        // warden's first client tick is never at age 0, a heartbeat tick.
        m.age += 1;
        client_tick(&mut m, &mut out);
        assert!(out.is_empty(), "a standing warden digs nothing: {out:?}");

        set_pose(&mut m, Pose::Emerging);
        client_tick(&mut m, &mut out);
        assert!(
            out.iter()
                .any(|e| matches!(e, MobEvent::DigParticles { .. })),
            "an emerging warden digs: {out:?}"
        );

        // Past 4500 ms the animation has played out.
        m.age += 20 * 5;
        out.clear();
        client_tick(&mut m, &mut out);
        assert!(
            !out.iter()
                .any(|e| matches!(e, MobEvent::DigParticles { .. })),
            "the animation is over: {out:?}"
        );
    }
}

