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

/// `AngerManagement.CONVERSION_DELAY`: anger ticks between two lookups of the
/// suspects that are away.
const CONVERSION_DELAY: i32 = 2;



// ---- entity facts ----




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

