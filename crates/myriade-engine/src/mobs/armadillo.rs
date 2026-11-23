//! Armadillos (`Armadillo`, `ArmadilloAi`): a player who sprints or rides
//! nearby, an undead mob or the last attacker scares one into a ball (IDLE,
//! ROLLING, SCARED, UNROLLING); it peeks out now and then, and a ball takes
//! less damage. The brain's `ARMADILLO_SCARE_DETECTED` sensor, its
//! `DANGER_DETECTED_RECENTLY` memory and the `ArmadilloBallUp` behaviour are
//! kept in `State`; the walking behaviours are the goals of `register`.
//! AnimState states: roll out, roll up, peek.
use super::{
    Damage, DamageKind, MobEvent, MobWorld, Species, Who,
    goals::{
        AnimalPanic, Breed, Cx, Float, Goal, LOOK, LookAtPlayer, MOVE, RandomLookAround, Stroll,
        Tempt, goal_clone,
    },
};
use horizon_core::{geometry::Vec3, random::RandomSource};

pub const IDLE: u8 = 0;
pub const ROLLING: u8 = 1;
pub const SCARED: u8 = 2;
pub const UNROLLING: u8 = 3;

const ROLL_OUT: usize = 0;
const ROLL_UP: usize = 1;
const PEEK: usize = 2;

/// `ArmadilloState.animationDuration`.
fn animation_duration(state: u8) -> i32 {
    match state {
        ROLLING => 10,
        SCARED => 50,
        UNROLLING => 30,
        _ => 0,
    }
}

/// `Armadillo.ARMADILLO_STATE` and the brain's memory and behaviour.
#[derive(Clone, Debug, Default)]
pub struct State {
    pub state: u8,
    /// `inStateTicks`.
    pub in_state_ticks: i32,
    /// `DANGER_DETECTED_RECENTLY`: ticks to live, 0 when absent.
    danger: i32,
    /// Ticks to the sensor's next scan.
    sensor: i32,
    /// `ArmadilloBallUp`: running, its peek timer and whether it saw the
    /// danger around last tick.
    ball_up: bool,
    next_peek: i32,
    danger_was_around: bool,
    /// Whether the PANIC activity was the active one after the last tick.
    panic_activity: bool,
    /// `peekReceivedClient`.
    peek_received: bool,
}




/// `AdultSensor` and `BabyFollowAdult.create(5..16, 1.25)`: a baby with no walk
/// target walks towards the nearest adult of its kind in sight that is 5 to 17
/// blocks away, until it is within 4.
#[derive(Clone, Debug, Default)]
struct BabyFollowAdult {
    adult: Option<u32>,
}
/// `UniformInt.of(5, 16)`'s bounds and `speedModifier`.
const FOLLOW_MIN: f64 = 5.0;
const FOLLOW_MAX: f64 = 16.0;
const FOLLOW_SPEED: f64 = 1.25;
/// The armadillo's `FOLLOW_RANGE` attribute, the sensor's reach.
const FOLLOW_RANGE: f64 = 16.0;
impl BabyFollowAdult {
}
impl Goal for BabyFollowAdult {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn every_tick(&self) -> bool {
        true
    }
}

/// `ArmadilloState`'s name (`StringRepresentable`, which its `CODEC` writes):
/// what `Armadillo.addAdditionalSaveData` stores under `state`.
pub fn state_name(state: u8) -> &'static str {
    match state {
        ROLLING => "rolling",
        SCARED => "scared",
        UNROLLING => "unrolling",
        _ => "idle",
    }
}
/// `ArmadilloState.CODEC` read back: a name that is no state, and no name at
/// all, are `IDLE` (`read("state", CODEC).orElse(IDLE)`).
pub fn state_by_name(name: Option<&str>) -> u8 {
    match name {
        Some("rolling") => ROLLING,
        Some("scared") => SCARED,
        Some("unrolling") => UNROLLING,
        _ => IDLE,
    }
}

/// `DamageTypeTags.PANIC_ENVIRONMENTAL_CAUSES`.
pub fn environmental(kind: DamageKind) -> bool {
    matches!(
        kind,
        DamageKind::Cactus
            | DamageKind::OnFire
            | DamageKind::InFire
            | DamageKind::Lava
            | DamageKind::LightningBolt
            | DamageKind::HotFloor
            | DamageKind::Freeze
            | DamageKind::SulfurCubeHot
    )
}

/// `EntityTypeTags.UNDEAD`.
fn undead(s: Species) -> bool {
    matches!(
        s,
        Species::Skeleton
            | Species::Stray
            | Species::WitherSkeleton
            | Species::SkeletonHorse
            | Species::Bogged
            | Species::Parched
            | Species::ZombieHorse
            | Species::CamelHusk
            | Species::Zombie
            | Species::ZombieVillager
            | Species::ZombifiedPiglin
            | Species::Zoglin
            | Species::Drowned
            | Species::Husk
            | Species::ZombieNautilus
            | Species::Wither
            | Species::Phantom
    )
}



/// `ArmadilloAi.ArmadilloPanic`: it unrolls, then runs.
#[derive(Clone, Debug)]
struct ArmadilloPanic(AnimalPanic);
impl Goal for ArmadilloPanic {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn name(&self) -> &'static str {
        "Panic"
    }
    fn every_tick(&self) -> bool {
        self.0.every_tick()
    }
}
/// A behaviour of the IDLE activity: `ArmadilloAi.updateActivity` picks
/// PANIC over it while danger is in mind and it is not panicking, and a
/// ball does not walk (`ArmadilloAi$1`, its `MoveToTargetSink`).
#[derive(Clone, Debug)]
struct Idle<G>(G);
impl<G: Goal + Clone + 'static> Goal for Idle<G> {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn every_tick(&self) -> bool {
        self.0.every_tick()
    }
}
/// `FollowTemptation` with `ArmadilloAi.initIdleActivity`'s close enough
/// distance: 1 block for a baby, 2 for an adult.
#[derive(Clone, Debug)]
struct ArmadilloTempt(Tempt);
impl Goal for ArmadilloTempt {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn name(&self) -> &'static str {
        self.0.name()
    }
}
/// The behaviours that do not run while the armadillo is a ball: its
/// `MoveToTargetSink` and the idle activity's walking and looking.
#[derive(Clone, Debug)]
struct Rolled;
impl Goal for Rolled {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use std::sync::Arc;


    /// Spider eyes are armadillo food. Rolled up, the click fails; a brush
    /// does not. Idle, food is left to `Animal.mobInteract`.
    #[test]
    fn a_rolled_up_armadillo_refuses_food() {
        let mut m = armadillo();
        set_state(&mut m, SCARED);
        assert!(client_rejects_food(&m, "minecraft:spider_eye"));
        assert!(!client_rejects_food(&m, "minecraft:brush"));
        set_state(&mut m, IDLE);
        assert!(!client_rejects_food(&m, "minecraft:spider_eye"));
    }

    #[test]
    fn a_hit_rolls_it_up_into_a_ball() {
        // Armadillo.actuallyHurt: a hit by a living thing curls it up and
        // keeps the danger in mind for 80 ticks, so the model shows the
        // shell instead of the armadillo (#2759).
        let mut m = armadillo();
        after_hurt(&mut m, &Damage::player([0.0, 0.0, 0.0]));
        assert_eq!(state(&m), ROLLING);
        // Armadillo.rollUp: the roll sound.
        assert!(
            m.outbox
                .iter()
                .any(|e| matches!(e, MobEvent::Sound(s, ..) if *s == "entity.armadillo.roll")),
            "the roll plays its sound"
        );
        // ArmadilloState.shouldHideInShell: the shell shows after five ticks.
        for _ in 0..6 {
            tick(&mut m);
        }
        assert!(hiding_in_shell(&m), "the model shows the shell");
    }

    /// Armadillo.rollUp and rollOut post `GameEvent.ENTITY_ACTION` (#8714),
    /// silent or not, for the sculk sensors and wardens nearby.
    #[test]
    fn rolling_up_and_out_posts_entity_action() {
        let action = |events: &[MobEvent]| {
            events.iter().any(|e| {
                matches!(e, MobEvent::GameEvent { event, source: 1, .. }
                    if *event == "minecraft:entity_action")
            })
        };
        let mut m = armadillo();
        m.silent = true;
        let mut events = Vec::new();
        roll_up(&mut m, &mut events);
        assert_eq!(state(&m), ROLLING);
        assert!(action(&events), "rolling up posts entity_action");
        let mut events = Vec::new();
        roll_out(&mut m, &mut events);
        assert_eq!(state(&m), IDLE);
        assert!(action(&events), "rolling out posts entity_action");
        // Neither posts anything when it doesn't change state.
        let mut events = Vec::new();
        roll_out(&mut m, &mut events);
        assert!(!action(&events));
    }


    #[test]
    #[ignore = "daily"]
    fn two_armadillos_in_love_make_a_baby() {
        // ArmadilloAi.initIdleActivity: AnimalMakeLove(ARMADILLO, 1.0), so two
        // armadillos fed spider eyes breed (#3512).
        let Some([mut a, mut b, mut c]) =
            real([[0.5, 64.0, 0.5], [2.5, 64.0, 0.5], [0.5, 64.0, 1.5]])
        else {
            return;
        };
        for m in [&mut a, &mut b, &mut c] {
            m.extra.in_love = 600;
        }
        let partner = [b.seen(None, &crate::items::Items::empty())];
        crate::mobs::world::with_test_world(&partner, |w, _| {
            let events = run_goals(&mut a, w, 80);
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    MobEvent::Breed {
                        parent: 1,
                        partner: 2,
                        ..
                    }
                )),
                "the pair breeds"
            );
            assert_eq!(a.extra.in_love, 0);
            assert_eq!(a.extra.age, 6000, "the parent rests");

            // ArmadilloAi.updateActivity: with danger in mind and no panic,
            // PANIC replaces IDLE and its behaviours do not run, nor do a
            // ball's.
            c.extra.armadillo.panic_activity = true;
            let events = run_goals(&mut c, w, 80);
            assert!(!events.iter().any(|e| matches!(e, MobEvent::Breed { .. })));
            assert!(!c.ai.goals.is_running("Breed"));
            c.extra.armadillo.panic_activity = false;
            set_state(&mut c, SCARED);
            run_goals(&mut c, w, 1);
            assert!(!c.ai.goals.is_running("Breed"), "a ball does not breed");
            set_state(&mut c, IDLE);
            run_goals(&mut c, w, 1);
            assert!(c.ai.goals.is_running("Breed"), "it looks for its mate");
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_baby_armadillo_follows_an_adult() {
        // BabyFollowAdult(5..16, 1.25) in the idle activity.
        let Some([mut baby, adult]) = real([[0.5, 64.0, 0.5], [8.5, 64.0, 0.5]]) else {
            return;
        };
        baby.extra.age = -24000;
        crate::mobs::world::with_test_world(
            &[adult.seen(None, &crate::items::Items::empty())],
            |w, _| {
                run_goals(&mut baby, w, 1);
                assert!(baby.ai.goals.is_running("BabyFollowAdult"));
                set_state(&mut baby, SCARED);
                run_goals(&mut baby, w, 1);
                assert!(
                    !baby.ai.goals.is_running("BabyFollowAdult"),
                    "not as a ball"
                );
            },
        );
    }

    #[test]
    #[ignore = "daily"]
    fn a_baby_follows_only_adults_its_sensor_box_meets() {
        // NearestLivingEntitySensor.doTick: the entities whose boxes meet the baby's box inflated
        // by FOLLOW_RANGE (16). Both adults are within BabyFollowAdult's 17 blocks. The baby's box
        // (0.35 to 0.7 wide, by its age scale) inflated reaches x = 16.675 to 16.85: the adult at
        // x = 17.0 (0.7 wide, its box from 16.65) is found, the one at x = 17.2 (from 16.85) is
        // not, though a scan of every mob returns it.
        for (x, follows) in [(17.0, true), (17.2, false)] {
            let Some([mut baby, adult]) = real([[0.5, 64.0, 0.5], [x, 64.0, 0.5]]) else {
                return;
            };
            baby.extra.age = -24000;
            crate::mobs::world::with_test_world(
                &[adult.seen(None, &crate::items::Items::empty())],
                |w, _| {
                    run_goals(&mut baby, w, 1);
                    assert_eq!(
                        baby.ai.goals.is_running("BabyFollowAdult"),
                        follows,
                        "x = {x}"
                    );
                },
            );
        }
    }

    #[test]
    #[ignore = "daily"]
    fn a_baby_stays_when_the_adult_is_within_five_blocks() {
        // BabyFollowAdult: `!closerThan(adult, 5)`.
        let Some([mut baby, adult]) = real([[0.5, 64.0, 0.5], [4.5, 64.0, 0.5]]) else {
            return;
        };
        baby.extra.age = -24000;
        crate::mobs::world::with_test_world(
            &[adult.seen(None, &crate::items::Items::empty())],
            |w, _| {
                run_goals(&mut baby, w, 1);
                assert!(!baby.ai.goals.is_running("BabyFollowAdult"));
            },
        );
    }

    #[test]
    #[ignore = "daily"]
    fn a_tempted_baby_comes_closer_than_an_adult() {
        // ArmadilloAi.initIdleActivity: FollowTemptation is close enough at
        // 1 block for a baby and 2 for an adult.
        let Some([mut baby, mut adult]) = real([[0.5, 64.0, 0.5], [4.5, 64.0, 0.5]]) else {
            return;
        };
        baby.extra.age = -24000;
        crate::mobs::world::with_test_world(&[], |w, _| {
            let mut events = Vec::new();
            let mut test_level = crate::health::TestLevel::new(0);
            let level = &mut test_level.level();
            let mut cx = Cx {
                w,
                level,
                events: &mut events,
            };
            let mut tempt = ArmadilloTempt(Tempt::new(1.25, baby.kind.food.clone(), false));
            tempt.tick(&mut baby, &mut cx);
            assert_eq!(tempt.0.stop_distance, 1.0);
            tempt.tick(&mut adult, &mut cx);
            assert_eq!(tempt.0.stop_distance, 2.0);
        });
    }

    #[test]
    fn campfire_damage_is_fire_and_has_no_knockback_but_does_not_cause_panic() {
        // `campfire` is in #is_fire and #no_knockback, but in neither
        // #panic_causes nor #panic_environmental_causes (#4037).
        let source = Damage::of(DamageKind::Campfire);
        assert!(source.is_fire());
        assert!(source.no_knockback());
        assert!(!source.causes_panic());
        assert!(!environmental(DamageKind::Campfire));
    }
    /// `Armadillo.addAdditionalSaveData` writes `state` (the `ArmadilloState`
    /// name, always) and `readAdditionalSaveData` switches to it, `idle` when
    /// the save names none. Horizon models the state and wrote only
    /// `scute_time`, so a rolled-up armadillo unloaded with its chunk came back
    /// idle: hits it took for half the damage past the first point cost it the
    /// whole amount, and its shell went away.
    #[test]
    fn a_rolled_up_armadillo_keeps_its_state_through_the_chunk_save() {
        use crate::entity_nbt::{mob_from_nbt, mob_to_nbt};
        use horizon_core::nbt::Tag;
        fn armadillo_tag(state: Option<&str>) -> Tag {
            let mut fields = vec![
                ("id", Tag::String("minecraft:armadillo".into())),
                (
                    "Pos",
                    Tag::List(vec![
                        Tag::Double(0.5_f64.to_bits()),
                        Tag::Double(64.0_f64.to_bits()),
                        Tag::Double(0.5_f64.to_bits()),
                    ]),
                ),
            ];
            if let Some(state) = state {
                fields.push(("state", Tag::String(state.into())));
            }
            Tag::compound(fields)
        }
        let types = [("minecraft:armadillo".into(), armadillo().kind.clone())].into();
        // A ball takes half of the damage past the first point, and hides in
        // its shell once the roll is five ticks in.
        let mut ball = mob_from_nbt(&armadillo_tag(Some("rolling")), &types, 1, None).unwrap();
        assert_eq!(state(&ball), ROLLING, "the loaded state");
        assert_eq!(reduce_damage(&ball, 10.0), 4.5, "a rolling armadillo");
        ball.extra.armadillo.in_state_ticks = 6;
        assert!(hiding_in_shell(&ball), "six ticks into the roll");
        // The save names the state the armadillo is in now, not the one its
        // compound came in with (`store`, not `storeNullable`).
        let saved = mob_to_nbt(
            &ball,
            [0; 4],
            Some(&armadillo_tag(Some("idle"))),
            None,
            |_| None,
        );
        assert_eq!(saved.get("state"), Some(&Tag::String("rolling".into())));
        let back = mob_from_nbt(&saved, &types, 2, None).unwrap();
        assert_eq!(state(&back), ROLLING, "the reloaded state");
        assert_eq!(reduce_damage(&back, 10.0), 4.5);
        // `switchToState` restarts the time in the state
        // (`onSyncedDataUpdated`), so the shell goes in again six ticks later.
        assert_eq!(back.extra.armadillo.in_state_ticks, 0);
        assert!(!hiding_in_shell(&back), "a reload restarts the roll");
        // No state, or a name that is no state: `orElse(IDLE)`, full damage.
        for absent in [None, Some("sneaking")] {
            let idle = mob_from_nbt(&armadillo_tag(absent), &types, 3, None).unwrap();
            assert_eq!(state(&idle), IDLE, "{absent:?}");
            assert_eq!(reduce_damage(&idle, 10.0), 10.0, "{absent:?}");
        }
    }
}

#[cfg(test)]
mod panic_environmental_tests {
    use super::*;

    /// `#panic_environmental_causes` from the vanilla data: every type in it
    /// panics the armadillo (`Armadillo.actuallyHurt` rolls out).
    #[test]
    fn the_environmental_causes_are_vanillas_tag() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/myriade-engine")
            .join(horizon_core::VANILLA_VERSION)
            .join("data/minecraft/tags/damage_type/panic_environmental_causes.json");
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut ids: Vec<&str> = json["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        ids.sort_unstable();
        let kinds = [
            ("minecraft:cactus", DamageKind::Cactus),
            ("minecraft:freeze", DamageKind::Freeze),
            ("minecraft:hot_floor", DamageKind::HotFloor),
            ("minecraft:in_fire", DamageKind::InFire),
            ("minecraft:lava", DamageKind::Lava),
            ("minecraft:lightning_bolt", DamageKind::LightningBolt),
            ("minecraft:on_fire", DamageKind::OnFire),
            ("minecraft:sulfur_cube_hot", DamageKind::SulfurCubeHot),
        ];
        assert_eq!(ids, kinds.map(|(id, _)| id).to_vec());
        for (id, kind) in kinds {
            assert!(environmental(kind), "{id}");
        }
        assert!(!environmental(DamageKind::Drown));
    }
}

#[cfg(test)]
mod armadillo_scute_time_tests {
    use crate::entity_nbt::mob_from_nbt_with_random;
    use crate::mobs::{MobType};
    use horizon_core::nbt::Tag;
    use horizon_core::{
        geometry::Vec3,
        random::{LegacyRandom, RandomSource},
    };
    use std::{collections::HashMap, sync::Arc};

    /// #15784: `Armadillo`'s constructor draws `scuteTime` from its own random, and
    /// `readAdditionalSaveData` replaces it only with a saved `scute_time`. A load without the
    /// key keeps the constructor's draw (never 0, #12559) and draws nothing more.
    #[test]
    #[ignore = "daily"]
    fn an_armadillo_loaded_without_scute_time_keeps_the_constructors_draw() {
        let ran = crate::mobs::world::with_test_world(&[], |_, all| {
            check(Arc::new(all.types["minecraft:armadillo"].clone()));
        });
        if !ran {
            eprintln!("Prepared mob data unavailable; the armadillo load did not run");
        }
    }

}
