use horizon_core::geometry::{Vec3, trig::Mth};
use horizon_core::random::{LegacyRandom, LevelRandom, RandomSource};
use std::f32::consts::PI;

/// `PiglinAi.ADMIRE_DURATION`.
pub const ADMIRE: i32 = 119;
/// `PiglinAi.eat`: the `ATE_RECENTLY` memory lasts 200 ticks (kept in
/// `extra.piglin.ate_recently`).
pub const ATE_RECENTLY: i32 = 200;
/// `AbstractPiglin.CONVERSION_TIME` (and the hoglin's).
const CONVERSION: i32 = 300;
/// `PiglinAi.CELEBRATION_TIME`: how long the `CELEBRATE_LOCATION` and
/// `DANCING` memories last.
const CELEBRATION: i32 = 300;

/// `ZombifiedPiglin.populateDefaultEquipmentSlots`: a golden sword, or one
/// time in 20 a golden spear, off `random` (`Pig.thunderHit` passes the pig's
/// own, `this.getRandom()`).
pub fn default_equipment(random: &mut impl RandomSource) -> &'static str {
    if random.next_int_bound(20).unwrap_or(1) == 0 {
        "minecraft:golden_spear"
    } else {
        "minecraft:golden_sword"
    }
}

/// `PiglinAi.BARTERING_ITEM`: vanilla's barter currency, compared by item.
pub const BARTERING_ITEM: &str = "minecraft:gold_ingot";


/// `Activity`: what a piglin's or hoglin's brain is busy with. `Admire`,
/// `Celebrate` and `Ride` are a piglin's only; the others are both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activity {
    #[default]
    Idle,
    Fight,
    Avoid,
    Admire,
    Celebrate,
    Ride,
}




/// `TimeUtil.rangeOfSeconds(5, 7)` of `PiglinAi.AVOID_ZOMBIFIED_DURATION` and
/// `BABY_AVOID_NEMESIS_DURATION`, sampled from the level random.
fn avoid_duration(lr: &mut LevelRandom) -> i32 {
    lr.next_int_between_inclusive(100, 140).unwrap_or(100)
}
/// `SetEntityLookTargetSometimes.Ticker.tickDownAndCheck`: with nothing left
/// to wait for, draw the next wait from `interval` and say no; otherwise
/// count down and say yes when it reaches nothing.
fn tick_down_and_check(wait: &mut i32, lr: &mut LevelRandom, interval: (i32, i32)) -> bool {
    if *wait == 0 {
        *wait = lr
            .next_int_between_inclusive(interval.0, interval.1)
            .unwrap_or(interval.0)
            - 1;
        false
    } else {
        *wait -= 1;
        *wait == 0
    }
}
/// `PiglinAi.SPEED_MULTIPLIER_WHEN_MOUNTING`.
const MOUNTING_SPEED: f64 = 0.8;
/// The block under `at` (`BlockPos.containing`).
fn block_of(at: Vec3) -> [i32; 3] {
    [
        at.x.floor() as i32,
        at.y.floor() as i32,
        at.z.floor() as i32,
    ]
}
/// `PiglinAi.RETREAT_DURATION` and `HoglinAi.RETREAT_DURATION`:
/// `TimeUtil.rangeOfSeconds(5, 20)`, sampled from the level random.
fn retreat_duration(lr: &mut LevelRandom) -> i32 {
    lr.next_int_between_inclusive(100, 400).unwrap_or(100)
}
/// `PiglinAi.TIME_BETWEEN_HUNTS` (30 to 120 seconds) sampled from the level
/// random.
fn hunt_wait(lr: &mut LevelRandom) -> i32 {
    lr.next_int_between_inclusive(600, 2400).unwrap_or(600)
}
/// What `SetWalkTargetAwayFrom.entity` walks away from.
#[derive(Clone, Copy, Debug)]
enum AwayFrom {
    /// The retreat activity's `AVOID_TARGET`.
    AvoidTarget,
    /// The idle activity's `NEAREST_VISIBLE_ADULT_PIGLIN` (a hoglin's).
    AdultPiglin,
}
/// `SetWalkTargetAwayFrom.entity(memory, speed, desiredDistance, interruptCurrentWalk)`:
/// closer than the desired distance to what it avoids, pick a spot away
/// from it and walk there, unless a walk is on and may not be interrupted.
/// A walk at the same speed whose spot lies away from it is left alone.
#[derive(Clone, Debug)]
struct WalkAway {
    from: AwayFrom,
    speed: f64,
    desired: f64,
    interrupt: bool,
}
impl WalkAway {
    fn retreat(speed: f64, desired: f64, interrupt: bool) -> Self {
        Self {
            from: AwayFrom::AvoidTarget,
            speed,
            desired,
            interrupt,
        }
    }
    fn adult_piglin(speed: f64, desired: f64, interrupt: bool) -> Self {
        Self {
            from: AwayFrom::AdultPiglin,
            speed,
            desired,
            interrupt,
        }
    }
}
/// A goal of the idle and fight activities: it waits while the brain is in
/// the retreat activity.
#[derive(Clone, Debug)]
struct NotRetreating<G>(G);

/// A goal of the fight activity: `HoglinAi.updateActivity` runs after the
/// brain, so a hoglin given a target fights from the next tick on (here
/// `brain_step` runs after the goals). The goal may start a tick early, as
/// the goal selector picks new goals every other tick only and the brain
/// every tick, but it acts only once the activity is fight.
#[derive(Clone, Debug)]
struct InFight<G>(G);

/// A goal of the idle and avoid activities' movement: the ride activity has
/// none of it (`PiglinAi.initRideHoglinActivity`), so it waits while the
/// brain is in the ride activity.
#[derive(Clone, Debug)]
struct NotRiding<G>(G);

/// `BackUpIfTooClose.create(5, 0.75)` of `PiglinAi.initFightActivity`, behind
/// `hasCrossbow`: with no walk target and the attack target in sight within 5
/// blocks, look at it and strafe backwards every tick. It takes no flags so the
/// crossbow goal keeps aiming meanwhile.
#[derive(Clone, Debug)]
struct BackUpIfTooClose;
impl BackUpIfTooClose {
}
/// `PiglinBruteAi.HOME_CLOSE_ENOUGH_DISTANCE`: how near home the walk target
/// counts as arrived.
const HOME_CLOSE_ENOUGH: i32 = 2;
/// `PiglinBruteAi.HOME_TOO_FAR_DISTANCE`: past this the memory is no help.
const HOME_TOO_FAR: f64 = 100.0;
/// `StrollToPoi(HOME, 0.6F, 2, 100)` of `PiglinBruteAi.createIdleMovementBehaviors`:
/// a brute more than a hundred blocks from the spot it was spawned at gives up
/// on it, and one that has wandered off walks back to within two blocks of it.
///
/// The vanilla behaviour sets the walk target home every 80 ticks; this goal
/// instead keeps it only while the brute is outside the reach of the plain
/// stroll that shares its priority (`RandomStroll`, ten blocks), which is the
/// wandering `StrollAroundPoi(HOME, 0.6F, 5)` of the same activity does around
/// the memory. A goal selector here starts the first goal that may run, where
/// vanilla draws a weighted one each time, so a home target that outranked the
/// stroll inside its own reach would leave the brute standing at home. As in
/// vanilla, a try is followed by 80 ticks without one (`nextOkStartTime`), so a
/// home it cannot path to costs one search every four seconds, not every tick.
#[derive(Clone, Debug)]
struct BruteStrollHome {
    /// `StrollToPoi.nextOkStartTime`: the game time before which it does not try.
    next_ok: i64,
}
/// `SetWalkTargetAwayFrom.pos(NEAREST_REPELLENT, 1.0, 8, true)` of the hoglin's
/// idle activity: closer than 8 blocks to a repellent, walk away from it.
#[derive(Clone, Debug)]
struct HoglinAvoidRepellent;
/// `PiglinAi.avoidRepellent`: `SetWalkTargetAwayFrom.pos(NEAREST_REPELLENT, 1.0, 8, false)`
/// of the idle activity. Closer than 8 blocks to a repellent and not walking
/// anywhere, walk away from it.
#[derive(Clone, Debug)]
struct PiglinAvoidRepellent;
/// `PiglinAi.isPlayerHoldingLovedItem`: the idle and celebrate activities'
/// `SetEntityLookTarget.create(.., 14.0)` and the ride activity's, with 8.0,
/// turn to look at the player in sight within that many blocks who holds gold.
#[derive(Clone, Debug)]
struct LookAtLoved;
impl LookAtLoved {
}
/// Admiring a gold item held in the off hand: stand and look at it.
#[derive(Clone, Debug)]
struct Admire;
/// `initAdmireItemActivity` for an item only seen: the idle and fight behaviours
/// stand by while `GoToWantedItem` (the game sets the point) walks to it.
#[derive(Clone, Debug)]
struct AdmireSeen;
/// The celebrate activity: `GoToTargetLocation` on the spot a killed target
/// fell (within 2 blocks, or 4 and slower while dancing).
#[derive(Clone, Debug)]
struct Celebrate;
impl Celebrate {
}
/// `GoToWantedItem`: walk to the gold the game spotted.
#[derive(Clone, Debug)]
struct GoToWantedItem;
/// `PiglinArmPose`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiglinPose {
    Default,
    Dancing,
    MeleeWeapon,
    CrossbowHold,
    CrossbowCharge,
    AdmiringItem,
}




#[cfg(test)]
mod brute_sound_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use crate::mobs::world::with_test_world;
    use std::sync::Arc;

    fn sounds(events: &[MobEvent]) -> Vec<&'static str> {
        events
            .iter()
            .filter_map(|e| match e {
                MobEvent::Sound(sound, ..) => Some(*sound),
                _ => None,
            })
            .collect()
    }

    /// `PiglinBruteAi.updateActivity`: a brute with an attack target enters
    /// the fight activity and growls `entity.piglin_brute.angry`, losing the
    /// target is silent, and a piglin keeps `entity.piglin.angry`.
    #[test]
    fn a_brute_with_a_target_selects_the_fight_activity() {
        let mut brute = mob(Species::PiglinBrute);
        brute.ai.target = Some(Who::Player);
        assert_eq!(activity(&brute), Activity::Fight);
    }

    /// The growl itself, with the activity switch.
    #[test]
    #[ignore = "daily"]
    fn a_brute_names_the_brute_growl() {
        let mut brute = mob(Species::PiglinBrute);
        brute.ai.target = Some(Who::Player);
        // The switch itself takes a level; it runs where the vanilla data is.
        assert!(
            with_test_world(&[], |w, _| {
                let mut events = Vec::new();
                update_activity(&mut brute, w, &mut events);
                assert_eq!(sounds(&events), ["entity.piglin_brute.angry"]);
                brute.ai.target = None;
                let mut events = Vec::new();
                update_activity(&mut brute, w, &mut events);
                assert!(sounds(&events).is_empty());
                let mut piglin = mob(Species::Piglin);
                piglin.ai.target = Some(Who::Player);
                let mut events = Vec::new();
                update_activity(&mut piglin, w, &mut events);
                assert_eq!(sounds(&events), ["entity.piglin.angry"]);
            }),
            "vanilla data is missing"
        );
    }

    /// `PiglinBruteAi.maybePlayActivitySound`: the level random is drawn on
    /// every tick, fighting or not, and a hit only sounds in a fight.
    #[test]
    fn the_growl_roll_draws_every_tick_and_only_sounds_in_a_fight() {
        let mut brute = mob(Species::PiglinBrute);
        let mut lr = crate::mobs::world::level_random(7);
        let mut control = LegacyRandom::new(7);
        let mut events = Vec::new();
        let mut growls = 0;
        for tick in 0..2000 {
            let fight = tick % 2 == 0;
            brute.extra.piglin.activity = if fight {
                Activity::Fight
            } else {
                Activity::Idle
            };
            // The same draw on a stream of its own says when the roll hit.
            let hit = control.next_float() < 0.0125;
            maybe_play_brute_sound(&mut brute, &mut lr, &mut events);
            if hit && fight {
                growls += 1;
            }
            assert_eq!(sounds(&events).len(), growls, "tick {tick}");
        }
        assert!(growls > 0, "the roll never hit");
    }
}

#[cfg(test)]
mod hurt_broadcast_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use crate::mobs::world::{test_player, with_test_world};
    use std::sync::Arc;


    #[test]
    fn hoglin_broadcast_selection_proposes_without_mutating() {
        let mut receiver = mob(1, Species::Hoglin, Vec3::ZERO);
        let near = Who::Mob(2);
        let far = Who::Mob(3);
        let position = |who, _current| match who {
            Who::Mob(2) => Some(Vec3::new(2.0, 0.0, 0.0)),
            Who::Mob(3) => Some(Vec3::new(4.0, 0.0, 0.0)),
            Who::Player => Some(Vec3::new(4.0, 0.0, 0.0)),
            _ => None,
        };
        assert_eq!(receive_attack_target(&receiver, far, position), Some(far));
        assert_eq!(receiver.ai.target, None);
        receiver.extra.ints[0] = 1;
        assert_eq!(receive_attack_target(&receiver, near, position), None);
        receiver.extra.ints[0] = 0;
        receiver.ai.target = Some(near);
        assert_eq!(receive_attack_target(&receiver, far, position), None);
        receiver.ai.target = Some(far);
        assert_eq!(receive_attack_target(&receiver, near, position), Some(near));
        assert_eq!(receiver.ai.target, Some(far));
        assert_eq!(
            receive_attack_target(&receiver, near, |_, _| position(near, false)),
            Some(near)
        );
        assert_eq!(
            receive_attack_target(&receiver, Who::Mob(4), position),
            None
        );
        receiver.ai.target = Some(Who::Mob(4));
        assert_eq!(receive_attack_target(&receiver, near, position), Some(near));
        receiver.ai.target = Some(Who::Player);
        assert_eq!(receive_attack_target(&receiver, near, position), Some(near));
        receiver.ai.target = Some(far);
        assert_eq!(
            receive_attack_target(&receiver, Who::Player, position),
            Some(Who::Player)
        );
    }

    /// `PiglinBruteAi.wasHurtBy` is `PiglinAi.maybeRetaliate`, which calls the adult piglins
    /// around it, the regular ones too (#7790).
    #[test]
    #[ignore = "daily"]
    fn a_brute_hit_by_the_player_calls_the_piglins_around_it() {
        let mut events = Vec::new();
        let ran = with_test_world(&[], |w, _| {
            let mut w = w.clone();
            w.player = Some(test_player(Vec3::new(2.0, 64.0, 0.0)));
            let mut brute = mob(1, Species::PiglinBrute, Vec3::new(0.0, 64.0, 0.0));
            brute.ai.last_hurt_by = Some(Who::Player);
            brute.ai.last_hurt_by_time = 5;
            let piglin = mob(2, Species::Piglin, Vec3::new(3.0, 64.0, 0.0));
            let others = [piglin.seen(None, &crate::items::Items::empty())];
            w.others = &others;
            let mut lr = crate::mobs::world::level_random(1);
            before_goals(&mut brute, &w, &mut lr, &mut events);
        });
        if !ran {
            return;
        }
        assert!(
            events.iter().any(|e| matches!(
                e,
                MobEvent::SetTarget {
                    mob: 2,
                    target: Who::Player
                }
            )),
            "the piglin takes the player as its target: {events:?}"
        );
    }
}

#[cfg(test)]
mod crossbow_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use crate::mobs::world::level_random;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;


    #[test]
    fn admired_item_drops_when_killed_without_a_player() {
        use crate::mobs::drops::{Death, Drop, die};
        use crate::random_sequences::RandomSequences;
        use horizon_world::loot::{LootData, PackOverlay};

        let root =
            std::env::temp_dir().join(format!("horizon-piglin-drop-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("data/minecraft/loot_table")).unwrap();
        std::fs::write(root.join("items.json"), "{}").unwrap();
        let defaults = horizon_world::item_defaults::read(&root).unwrap();
        let loot = LootData::load(&root, &PackOverlay::default(), &defaults).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        let death = Death {
            loot: &loot,
            damage: None,
            player: None,
            player_killed: false,
            luck: 0.0,
            vehicle: None,
            mob_drops: true,
            world_seed: 0,
        };
        let mut apple = piglin();
        hold_in_offhand(
            &mut apple,
            crate::items::ItemStack::new("minecraft:golden_apple", 1),
        );
        assert!(apple.persistent);
        let mut m = piglin();
        hold_in_offhand(
            &mut m,
            crate::items::ItemStack::new("minecraft:gold_ingot", 1),
        );
        assert!(!m.persistent);
        let (drops, _) = die(
            &mut m,
            &death,
            &mut RandomSequences::default(),
            &mut level_random(7),
        );
        assert!(
            drops.iter().any(
                |drop| matches!(drop, Drop::Item(item) if item.item == "minecraft:gold_ingot")
            )
        );
    }

    /// The spawn weapon is equipped after the goals are registered, so the
    /// crossbow goal must exist on a piglin that holds nothing yet, and the
    /// melee goal must be off once a crossbow is held.
    #[test]
    fn a_piglin_with_a_crossbow_shoots_instead_of_meleeing() {
        let mut m = piglin();
        assert!(format!("{:?}", m.ai.goals).contains("CrossbowAttack"));
        assert!(!super::super::spear_use::holds_non_melee_weapon(&m));
        m.extra.set_main_hand(Some("minecraft:crossbow".into()));
        assert!(super::super::spear_use::holds_non_melee_weapon(&m));
        m.extra.set_main_hand(Some("minecraft:golden_sword".into()));
        assert!(!super::super::spear_use::holds_non_melee_weapon(&m));
    }

    #[test]
    fn a_loved_item_in_sight_is_admired_until_too_far_or_too_long() {
        let mut b = Memories::default();
        // Seen: the memory is set at once, and not again while it is there.
        b.admire_seen(false, Some(25.0), true);
        assert_eq!(b.seen_admire, ADMIRE);
        b.seen_admire = 50;
        b.admire_seen(false, Some(25.0), true);
        assert_eq!(b.seen_admire, 50);
        // An item that is not loved starts nothing.
        let mut c = Memories::default();
        c.admire_seen(false, Some(25.0), false);
        assert_eq!(c.seen_admire, 0);
        // 9 blocks away or gone (`closerThan(9)`) ends it.
        b.admire_seen(false, Some(81.0), true);
        assert_eq!(b.seen_admire, 0);
        b.seen_admire = 50;
        b.admire_seen(false, None, true);
        assert_eq!(b.seen_admire, 0);
        // Admiring disabled by a player's blow: never starts.
        let mut d = Memories {
            admire_disabled: 3,
            ..Memories::default()
        };
        d.admire_seen(false, Some(1.0), true);
        assert_eq!(d.seen_admire, 0);
        // Tired of trying: 201 ticks near the item stop it and disable the walk
        // for 200 ticks, in which it does not start again.
        let mut e = Memories::default();
        e.admire_seen(false, Some(4.0), true);
        for _ in 0..203 {
            e.seen_admire = 50;
            e.admire_seen(false, Some(4.0), true);
        }
        assert_eq!((e.seen_admire, e.walk_off, e.reach_time), (0, 200, None));
        e.admire_seen(false, Some(4.0), true);
        assert_eq!(e.seen_admire, 0);
        // An item in the off hand leaves the memory alone.
        let mut f = Memories {
            seen_admire: 40,
            ..Memories::default()
        };
        f.admire_seen(true, None, false);
        assert_eq!(f.seen_admire, 40);
    }
}

#[cfg(test)]
mod conversion_tests {
    use super::*;
    use crate::mobs::world::{level_random, with_test_world};


}

#[cfg(test)]
mod cannot_hunt_save_tests {
    use super::*;
    use horizon_core::nbt::Tag;


    fn saved(cannot_hunt: Option<i8>) -> Tag {
        let mut fields = vec![
            ("id", Tag::String("minecraft:piglin".into())),
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(0.0f64.to_bits()),
                    Tag::Double(64.0f64.to_bits()),
                    Tag::Double(0.0f64.to_bits()),
                ]),
            ),
        ];
        if let Some(value) = cannot_hunt {
            fields.push(("CannotHunt", Tag::Byte(value)));
        }
        Tag::compound(fields)
    }

    /// `Piglin.readAdditionalSaveData` reads `CannotHunt` with
    /// `getBooleanOr`, and `addAdditionalSaveData` always writes it. Horizon
    /// wrote a hardcoded 0 and never read it back, so a piglin that had
    /// `cannotHunt` set could hunt hoglins again after a reload.
    #[test]
    fn a_saved_cannot_hunt_is_read_and_written_back() {
        let template = piglin();
        let types = [("minecraft:piglin".into(), template.kind)].into();
        for (raw, expected) in [(Some(1), true), (Some(0), false), (None, false)] {
            let m = crate::entity_nbt::mob_from_nbt(&saved(raw), &types, 1, None).unwrap();
            assert_eq!(m.extra.piglin.cannot_hunt, expected);
            assert_eq!(can_hunt(&m), !expected);
            let out = crate::entity_nbt::mob_to_nbt(&m, [1, 2, 3, 4], None, None, |_| None);
            assert_eq!(out.get("CannotHunt"), Some(&Tag::Byte(i8::from(expected))));
        }
    }
}

#[cfg(test)]
mod brute_home_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use crate::mobs::world::with_built_test_world;
    use horizon_world::registry::BlockRegistry;
    use std::sync::Arc;

    /// A stone floor under the brute, and the chunks the pathfinder reads.
    fn build(world: &mut crate::world::World, registry: &BlockRegistry) {
        let stone = registry.default_state("minecraft:stone").unwrap();
        for cx in -4..4 {
            for cz in -4..4 {
                world
                    .chunks
                    .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
            }
        }
        for x in -64..64 {
            for z in -64..64 {
                world.set(x, -1, z, stone).unwrap();
            }
        }
    }
    /// `StrollToPoi(HOME, 0.6F, 2, 100)` of the brute's idle movement
    /// behaviours, through the goal `register_brute` gives it: a brute more
    /// than ten blocks from the spot it spawned at walks back to it, and one
    /// that is near home, past a hundred blocks, or without the memory does
    /// not.
    #[test]
    #[ignore = "daily"]
    fn a_brute_that_wanders_off_walks_back_home() {
        assert!(
            with_built_test_world(&[], build, |w, _| {
                let mut test_level = crate::health::TestLevel::new(3);
                let mut events = Vec::new();
                for (label, home, walks) in [
                    ("far enough to walk back", Some([24, 0, 0]), true),
                    ("near home", Some([5, 0, 0]), false),
                    ("past the memory's reach", Some([120, 0, 0]), false),
                    ("no home", None, false),
                ] {
                    let mut m = brute(Vec3::new(0.5, 0.0, 0.5), home);
                    register_brute(&mut m);
                    let mut cx = Cx {
                        w,
                        level: &mut test_level.level(),
                        events: &mut events,
                    };
                    let (running, target) = tick_goals(&mut m, &mut cx);
                    if walks {
                        assert!(running, "{label}: the brute sets off home");
                        assert_eq!(target, home, "{label}: and the walk ends at home");
                    } else {
                        assert!(!running, "{label}: no walk home");
                        assert_eq!(target, None, "{label}: and no path");
                    }
                }
            }),
            "vanilla data is missing"
        );
    }
    /// `StrollToPoi.nextOkStartTime`: after a try the brute waits 80 ticks before the
    /// next one, so a home out of reach is not searched for on every tick.
    #[test]
    #[ignore = "daily"]
    fn a_brute_tries_the_way_home_once_every_80_ticks() {
        assert!(
            with_built_test_world(&[], build, |w, _| {
                let mut test_level = crate::health::TestLevel::new(3);
                let mut events = Vec::new();
                let mut cx = Cx {
                    w,
                    level: &mut test_level.level(),
                    events: &mut events,
                };
                let mut m = brute(Vec3::new(0.5, 0.0, 0.5), Some([24, 0, 0]));
                let mut goal = BruteStrollHome { next_ok: 0 };
                assert!(goal.can_use(&mut m, &mut cx), "the first try finds a path");
                m.ai.pending_path = None;
                assert!(
                    !goal.can_use(&mut m, &mut cx),
                    "no second search in the same 80 ticks"
                );
                assert_eq!(goal.next_ok, w.game_time + 80);
            }),
            "vanilla data is missing"
        );
    }
}

#[cfg(test)]
mod hoglin_pacified_feeding_tests {
    use super::*;
    use crate::mobs::ai::{feed_animal, takes_food};
    use crate::mobs::sensor_scan_tests::kind;
    use crate::mobs::world::with_built_test_world;
    use horizon_world::registry::BlockRegistry;
    use std::sync::Arc;

    /// A stone floor under the origin and, at `(3, 0, 0)`, a warped fungus
    /// (one of `#hoglin_repellents`) while `repellent` is set.
    fn build(repellent: bool) -> impl FnOnce(&mut crate::world::World, &BlockRegistry) {
        move |world, registry| {
            let stone = registry.default_state("minecraft:stone").unwrap();
            for cx in -4..4 {
                for cz in -4..4 {
                    world
                        .chunks
                        .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
                }
            }
            for x in -64..64 {
                for z in -64..64 {
                    world.set(x, -1, z, stone).unwrap();
                }
            }
            if repellent {
                let fungus = registry.default_state("minecraft:warped_fungus").unwrap();
                world.set(3, 0, 0, fungus).unwrap();
            }
        }
    }
    /// Without any world (CI has no vanilla data): the `PACIFIED` memory alone
    /// makes an adult refuse its food, and a baby still grows.
    #[test]
    fn the_pacified_memory_alone_refuses_an_adult_and_lets_a_baby_grow() {
        let mut level = LegacyRandom::new(0);
        let mut adult = hoglin(0);
        assert!(takes_food(&adult), "an unpacified adult breeds");
        adult.extra.ints[0] = 200;
        assert!(!takes_food(&adult), "a pacified adult refuses its food");
        assert!(feed_animal(&mut adult, &mut level, None).is_none());
        assert_eq!(adult.extra.in_love, 0);

        let mut baby = hoglin(-24000);
        baby.extra.ints[0] = 200;
        assert!(feed_animal(&mut baby, &mut level, None).is_some());
        assert_eq!(baby.extra.age, -24000 + 120 * 20, "a tenth of the growth");
    }
    /// `BecomePassiveIfMemoryPresent.create(NEAREST_REPELLENT, 200)` sets
    /// `PACIFIED` for 200 ticks while the sensor holds the repellent, and the
    /// sensor drops it as soon as no repellent is in reach: walking away from
    /// the fungus keeps the adult refusing food for the rest of the 200 ticks,
    /// then it breeds again (`HoglinAi.wasHurtBy` is the other way out).
    #[test]
    #[ignore = "daily"]
    fn pacification_outlives_the_repellent_by_200_ticks() {
        let mut level = LegacyRandom::new(0);
        let ran = with_built_test_world(&[], build(true), |w, _| {
            let mut m = hoglin(0);
            tick(&mut m, w, 20);
            assert!(pacified(&m));
            // Out of `HoglinSpecificSensor`'s reach (8 sideways, 4 up or down).
            m.position = Vec3::new(40.5, 0.0, 0.5);
            tick(&mut m, w, 20);
            assert_eq!(m.extra.repellent, None, "the sensor lost the fungus");
            assert!(pacified(&m), "but PACIFIED still has 180 ticks left");
            assert!(!takes_food(&m));
            assert!(feed_animal(&mut m, &mut level, None).is_none());
            assert_eq!(m.extra.in_love, 0);

            tick(&mut m, w, 180);
            assert!(!pacified(&m), "PACIFIED expires after 200 ticks");
            assert!(takes_food(&m), "the adult breeds again");
            assert!(feed_animal(&mut m, &mut level, None).is_some());
            assert_eq!(m.extra.in_love, 600);
        });
        assert!(ran, "vanilla data is missing");
    }
}
