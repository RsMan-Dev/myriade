use crate::beehive::BeehiveExt;
use horizon_core::random::{LevelRandom, RandomSource};
use std::collections::HashMap;

const FLAG_ROLL: u8 = 2;
const FLAG_HAS_STUNG: u8 = 4;
const FLAG_HAS_NECTAR: u8 = 8;




// --- NeutralMob -------------------------------------------------------------



// --- the bee's ticks ----------------------------------------------------------


// --- places -------------------------------------------------------------------

/// `BlockPos.withinBoxByManhattanDistance`: nearest first by Manhattan
/// distance, then x, y, and +z before -z.
fn manhattan_box(o: [i32; 3], rx: i32, ry: i32, rz: i32) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    for depth in 0..=rx + ry + rz {
        let max_x = rx.min(depth);
        for x in -max_x..=max_x {
            let max_y = ry.min(depth - x.abs());
            for y in -max_y..=max_y {
                let z = depth - x.abs() - y.abs();
                if z <= rz {
                    out.push([o[0] + x, o[1] + y, o[2] + z]);
                    if z != 0 {
                        out.push([o[0] + x, o[1] + y, o[2] - z]);
                    }
                }
            }
        }
    }
    out
}

// --- goals ----------------------------------------------------------------------




/// `BeeEnterHiveGoal`: into the hive beside it, or forget it when full.
#[derive(Clone, Debug)]
struct EnterHive;

/// `ValidateHiveGoal`: now and then, forget a hive that is gone.
#[derive(Clone, Debug)]
struct ValidateHive {
    last: i64,
}

/// `ValidateFlowerGoal`: now and then, forget a flower that is gone.
#[derive(Clone, Debug)]
struct ValidateFlower {
    last: i64,
}

/// `BeePollinateGoal`: fly to a flower within five blocks and hover about
/// over it; after 400 ticks there the bee carries nectar.
#[derive(Clone, Debug, Default)]
struct Pollinate {
    successful: i32,
    last_sound: i32,
    hover: Option<[f64; 3]>,
    ticks: i32,
    /// `unreachableFlowerCache`: flowers and the game time until which they
    /// count as out of reach.
    unreachable: HashMap<[i32; 3], i64>,
}
impl Pollinate {
    fn long_enough(&self) -> bool {
        self.successful > 400
    }
}

/// `BeeLocateHiveGoal`: the nearest bee home with room within 20 blocks,
/// skipping the ones `BeeGoToHiveGoal` gave up on (unless that is all).
#[derive(Clone, Debug)]
struct LocateHive;


/// `BeeGoToHiveGoal`: home when it wants to go in.
#[derive(Clone, Debug, Default)]
struct GoToHive {
    travelling: i32,
    last_path: Option<super::path::Path>,
    stuck: i32,
}
impl GoToHive {
}

/// `BeeGoToKnownFlowerGoal`: back to the remembered flower after 600 ticks
/// without nectar.
#[derive(Clone, Debug, Default)]
struct GoToKnownFlower {
    travelling: i32,
}

/// `BeeGrowCropGoal`: a bee with nectar and a hive grows crops below it.
#[derive(Clone, Debug)]
struct GrowCrop;

/// `BeeWanderGoal`: hover to a random spot ahead (or back towards a far hive).
#[derive(Clone, Debug)]
struct Wander;


#[cfg(test)]
mod tests {
    use super::sting;
    use crate::mobs::goals::Cx;
    use crate::mobs::world::with_test_world;
    use crate::mobs::{MobEvent, Species, Who, sensor_scan_tests::kind};
    use horizon_core::geometry::Vec3;


    /// `NeutralMob.updatePersistentAnger`: anger at the player ends on
    /// Peaceful, as for a creative or spectator player (#11377).
    #[test]
    #[ignore = "daily"]
    fn peaceful_ends_a_bees_anger_at_the_player() {
        assert!(
            with_test_world(&[], |w, _| {
                for (difficulty, still_angry) in [(2, true), (0, false)] {
                    let mut w = w.clone();
                    w.player = Some(crate::mobs::world::test_player(Vec3::new(1.0, 0.0, 0.0)));
                    w.difficulty =
                        crate::mobs::spawning::LocalDifficulty::new(difficulty, 18000, 0, 0.0);
                    let mut m = bee();
                    let b = super::st(&mut m);
                    b.angry_at = Some(Who::Player);
                    b.anger_end = w.game_time + 400;
                    super::update_persistent_anger(&mut m, &w);
                    let angry = super::get(&m).is_some_and(|b| b.angry_at.is_some());
                    assert_eq!(angry, still_angry, "difficulty {difficulty}");
                }
            }),
            "vanilla data is missing"
        );
    }

    /// `BeeGrowCropGoal.tick`: level event 2011 (data 15) comes before the
    /// crop's new state (#11377).
    #[test]
    #[ignore = "daily"]
    fn a_bee_sparks_a_crop_before_growing_it() {
        use crate::mobs::goals::Goal;
        crate::mobs::world::with_built_test_world(
            &[],
            |world, registry| {
                let wheat = registry.default_state("minecraft:wheat").unwrap();
                world.chunks.insert(
                    (0, 0),
                    std::sync::Arc::new(crate::chunk::Chunk::empty(0, 0)),
                );
                world.set(0, 63, 0, wheat).unwrap();
            },
            |w, _| {
                let mut m = bee();
                m.position = Vec3::new(0.5, 64.0, 0.5);
                let mut test_level = crate::health::TestLevel::new(1);
                let mut events = Vec::new();
                // The goal grows on a 1-in-reduced(30) roll.
                for _ in 0..1000 {
                    if !events.is_empty() {
                        break;
                    }
                    let mut cx = Cx {
                        w,
                        level: &mut test_level.level(),
                        events: &mut events,
                    };
                    super::GrowCrop.tick(&mut m, &mut cx);
                }
                assert!(
                    matches!(
                        events[..],
                        [
                            MobEvent::LevelEvent {
                                id: 2011,
                                pos: [0, 63, 0],
                                data: 15
                            },
                            MobEvent::SetBlock {
                                pos: [0, 63, 0],
                                ..
                            }
                        ]
                    ),
                    "{events:?}"
                );
            },
        );
    }

    /// `Bee.doHurtTarget` on Normal: a mob is poisoned for 10 seconds (200 ticks).
    #[test]
    #[ignore = "daily"]
    fn a_bee_sting_asks_for_ten_seconds_of_poison_on_normal() {
        assert!(
            with_test_world(&[], |w, _| {
                let mut m = bee();
                let mut events = Vec::new();
                let mut test_level = crate::health::TestLevel::new(1);
                let mut cx = Cx {
                    w,
                    level: &mut test_level.level(),
                    events: &mut events,
                };
                sting(&mut m, Who::Mob(9), &mut cx);
                assert!(events.iter().any(|e| matches!(
                    e,
                    MobEvent::Sting {
                        target: Who::Mob(9),
                        poison_ticks: 200,
                        ..
                    }
                )));
            }),
            "vanilla data is missing"
        );
    }

    /// `NeutralMob.isAngryAtAllPlayers`: with `universal_anger` on and no
    /// persistent anger target left, a bee still angry at the player targets
    /// them; with the rule off it targets no one.
    #[test]
    #[ignore = "daily"]
    fn universal_anger_lets_an_angry_bee_pick_the_player() {
        assert!(
            with_test_world(&[], |w, _| {
                for rule in [false, true] {
                    let mut w = w.clone();
                    w.player = Some(crate::mobs::world::test_player(Vec3::new(1.0, 0.0, 0.0)));
                    w.universal_anger = rule;
                    let mut m = bee();
                    let b = super::st(&mut m);
                    b.angry_at = None;
                    b.anger_end = w.game_time + 400;
                    m.ai.goals.add(1, super::BecomeAngryTarget::new());
                    let mut random = crate::health::TestLevel::new(1);
                    let mut events = Vec::new();
                    let mut cx = Cx {
                        w: &w,
                        level: &mut random.level(),
                        events: &mut events,
                    };
                    let mut goals = std::mem::take(&mut m.ai.goals);
                    for _ in 0..200 {
                        goals.tick(&mut m, &mut cx);
                    }
                    let targeted = m.ai.target == Some(Who::Player);
                    m.ai.goals = goals;
                    assert_eq!(targeted, rule, "universal_anger {rule}");
                }
            }),
            "vanilla data is missing"
        );
    }

    /// `NeutralMob.addPersistentAngerSaveData` and
    /// `readPersistentAngerSaveData`: `anger_end_time` and `angry_at`, the
    /// reference to the player the bee is angry at (`Bee` keeps the anger
    /// itself). `mob_to_nbt`/`mob_from_nbt` are the pair `Game::mob_tree_nbt`
    /// and `Game::load_mob` go through. Horizon modelled the target in
    /// `Bee.angry_at` and wrote only the timer, so a bee that had stung a
    /// player came back with its anger still counting but no one to attack.
    #[test]
    fn a_bees_anger_target_survives_a_save() {
        use crate::entity_nbt::{mob_from_nbt, mob_to_nbt, player_uuid};
        use horizon_core::nbt::Tag;
        let mut m = bee();
        {
            let b = super::st(&mut m);
            b.anger_end = 500;
            b.angry_at = Some(Who::Player);
        }
        let saved = mob_to_nbt(&m, [1, 2, 3, 4], None, None, |_| None);
        assert_eq!(
            saved.get("angry_at"),
            Some(&Tag::IntArray(player_uuid().to_vec())),
            "the save names the player"
        );
        let types = std::collections::HashMap::from([(m.kind.id.clone(), m.kind.clone())]);
        let back = mob_from_nbt(&saved, &types, 2, None).unwrap();
        let b = super::get(&back).unwrap();
        assert_eq!(b.angry_at, Some(Who::Player), "the reloaded target");
        assert_eq!(b.anger_end, 500, "the reloaded timer");
        // `storeNullable`: a calm bee writes no `angry_at`, even over a
        // compound that still names one.
        let calm = bee();
        let saved = mob_to_nbt(&calm, [1, 2, 3, 4], Some(&saved), None, |_| None);
        assert_eq!(saved.get("angry_at"), None, "no target, no key");
    }
}
