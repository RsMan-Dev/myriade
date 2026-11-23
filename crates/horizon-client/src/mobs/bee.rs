//! Bees (`Bee`): flying about (`FlyingMoveControl`, `BeeWanderGoal`), finding
//! flowers and hovering over them until they carry nectar
//! (`BeePollinateGoal`), `NeutralMob` anger at whoever hurts them, stinging
//! once and dying of it, and the hive and flower bookkeeping: finding a hive
//! with room nearby (`BeeLocateHiveGoal`), flying home at night, in the rain
//! or with nectar (`BeeGoToHiveGoal`) and going in (`BeeEnterHiveGoal`, done
//! by the host: [`MobEvent::EnterHive`]); the hive lets them out
//! (`crate::beehive`).
use super::{
    Damage, DamageKind, MobEvent, MobWorld, Species, Who,
    control::{LookKind, MoveKind},
    goals::{
        Breed, Cx, Float, FollowParent, Goal, HurtByTarget, MOVE, MeleeAttack, NearestTarget,
        Tempt, goal_clone, reduced,
    },
    nav::NavKind,
    path::PathType,
};
use crate::beehive::BeehiveExt;
use horizon_core::random::{LevelRandom, RandomSource};
use std::collections::HashMap;

const FLAG_ROLL: u8 = 2;
const FLAG_HAS_STUNG: u8 = 4;
const FLAG_HAS_NECTAR: u8 = 8;

/// The bee's own fields (`Bee`'s instance state and its synced flags).
#[derive(Clone, Debug)]
pub struct Bee {
    /// `DATA_FLAGS_ID`: rolling, has stung, has nectar.
    pub flags: u8,
    /// `DATA_ANGER_END_TIME`, -1 when calm.
    pub anger_end: i64,
    /// `persistentAngerTarget`.
    pub angry_at: Option<Who>,
    pub roll: f32,
    pub roll_o: f32,
    pub time_since_sting: i32,
    pub ticks_without_nectar: i32,
    pub stay_out_of_hive: i32,
    pub crops_grown: i32,
    /// `remainingCooldownBeforeLocatingNewHive` and `...NewFlower`.
    pub hive_cooldown: i32,
    pub flower_cooldown: i32,
    pub flower: Option<[i32; 3]>,
    pub hive: Option<[i32; 3]>,
    pub under_water: i32,
    /// `BeePollinateGoal.pollinating`, read by the navigation and look control.
    pub pollinating: bool,
    /// The cooldowns `ValidateHiveGoal` and `ValidateFlowerGoal` draw when built.
    pub validate_hive_cooldown: i32,
    pub validate_flower_cooldown: i32,
    /// `BeeGoToHiveGoal.blacklistedTargets`: the last three hives it gave up on.
    pub hive_blacklist: Vec<[i32; 3]>,
    /// What `sting` took from the bee before the victim's `hurtServer` answer
    /// was known, put back by `sting_refused`.
    undo: Option<StungUndo>,
}
/// `Bee.doHurtTarget`'s sting, kept until the game says whether the victim's
/// `hurtServer` reported a hurt.
#[derive(Clone, Debug)]
struct StungUndo {
    flags: u8,
    anger_end: i64,
    angry_at: Option<Who>,
    last_hurt_by: Option<Who>,
    last_hurt_by_ref: Option<[i32; 4]>,
    last_hurt_by_time: i32,
    target: Option<Who>,
}
impl Default for Bee {
    fn default() -> Self {
        Self {
            flags: 0,
            anger_end: -1,
            angry_at: None,
            roll: 0.0,
            roll_o: 0.0,
            time_since_sting: 0,
            ticks_without_nectar: 0,
            stay_out_of_hive: 0,
            crops_grown: 0,
            hive_cooldown: 0,
            flower_cooldown: 0,
            flower: None,
            hive: None,
            under_water: 0,
            pollinating: false,
            validate_hive_cooldown: 0,
            validate_flower_cooldown: 0,
            hive_blacklist: Vec::new(),
            undo: None,
        }
    }
}



// --- NeutralMob -------------------------------------------------------------


/// `NeutralMob.isValidPlayerTarget`: not creative or spectator, and not on
/// Peaceful.
fn valid_player_target(w: &MobWorld) -> bool {
    w.difficulty.id != 0
        && w.selected_player()
            .is_some_and(|p| !p.creative && !p.spectator)
}

// --- the bee's ticks ----------------------------------------------------------


// --- places -------------------------------------------------------------------

/// `EnvironmentAttributes.BEES_STAY_IN_HIVE`: the overworld's night, or rain.
pub fn bees_stay_in_hive(w: &MobWorld) -> bool {
    let night = w.dimension_type.has_timeline("minecraft:day")
        && (12542..23460).contains(&w.day_time.rem_euclid(24000));
    night || w.raining
}
/// `Bee.attractsBees`.
pub fn attracts_bees(w: &MobWorld, p: [i32; 3]) -> bool {
    let name = w.block_name(p[0], p[1], p[2]);
    if !w.has_tag("minecraft:bee_attractive", name) {
        return false;
    }
    if w.prop(p[0], p[1], p[2], "waterlogged") == Some("true") {
        return false;
    }
    name != "minecraft:sunflower" || w.prop(p[0], p[1], p[2], "half") == Some("upper")
}
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


/// `BeeAttackGoal`: a melee attack while angry and armed.
#[derive(Clone, Debug)]
struct BeeAttack(MeleeAttack);
impl Goal for BeeAttack {
    goal_clone!();
    fn name(&self) -> &'static str {
        "BeeAttackGoal"
    }
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn every_tick(&self) -> bool {
        self.0.every_tick()
    }
}

/// `BeeBecomeAngryTargetGoal`: the player it is angry at, while armed.
#[derive(Clone, Debug)]
struct BecomeAngryTarget(NearestTarget);
impl BecomeAngryTarget {
    fn new() -> Self {
        Self(
            NearestTarget::player(true)
                .interval(10)
                .gate(can_target)
                // NeutralMob.isAngryAt: the persistent anger target, or
                // every player under the `universal_anger` rule.
                .when(|m, w| {
                    get(m).is_some_and(|b| {
                        b.angry_at == Some(Who::Player)
                            || (w.universal_anger && b.angry_at.is_none() && angry(m, w))
                    })
                }),
        )
    }
}
impl Goal for BecomeAngryTarget {
    goal_clone!();
    fn name(&self) -> &'static str {
        "BeeBecomeAngryTargetGoal"
    }
    fn flags(&self) -> u8 {
        self.0.flags()
    }
}

/// `BeeEnterHiveGoal`: into the hive beside it, or forget it when full.
#[derive(Clone, Debug)]
struct EnterHive;
impl Goal for EnterHive {
    goal_clone!();
    fn name(&self) -> &'static str {
        "BeeEnterHiveGoal"
    }
    fn flags(&self) -> u8 {
        0
    }
}

/// `ValidateHiveGoal`: now and then, forget a hive that is gone.
#[derive(Clone, Debug)]
struct ValidateHive {
    last: i64,
}
impl Goal for ValidateHive {
    goal_clone!();
    fn name(&self) -> &'static str {
        "ValidateHiveGoal"
    }
    fn flags(&self) -> u8 {
        0
    }
}

/// `ValidateFlowerGoal`: now and then, forget a flower that is gone.
#[derive(Clone, Debug)]
struct ValidateFlower {
    last: i64,
}
impl Goal for ValidateFlower {
    goal_clone!();
    fn name(&self) -> &'static str {
        "ValidateFlowerGoal"
    }
    fn flags(&self) -> u8 {
        0
    }
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
impl Goal for Pollinate {
    goal_clone!();
    fn name(&self) -> &'static str {
        "BeePollinateGoal"
    }
    fn flags(&self) -> u8 {
        MOVE
    }
    fn every_tick(&self) -> bool {
        true
    }
}

/// `BeeLocateHiveGoal`: the nearest bee home with room within 20 blocks,
/// skipping the ones `BeeGoToHiveGoal` gave up on (unless that is all).
#[derive(Clone, Debug)]
struct LocateHive;
impl Goal for LocateHive {
    goal_clone!();
    fn name(&self) -> &'static str {
        "BeeLocateHiveGoal"
    }
    fn flags(&self) -> u8 {
        0
    }
}


/// `BeeGoToHiveGoal`: home when it wants to go in.
#[derive(Clone, Debug, Default)]
struct GoToHive {
    travelling: i32,
    last_path: Option<super::path::Path>,
    stuck: i32,
}
impl GoToHive {
}
impl Goal for GoToHive {
    goal_clone!();
    fn name(&self) -> &'static str {
        "BeeGoToHiveGoal"
    }
    fn flags(&self) -> u8 {
        MOVE
    }
}

/// `BeeGoToKnownFlowerGoal`: back to the remembered flower after 600 ticks
/// without nectar.
#[derive(Clone, Debug, Default)]
struct GoToKnownFlower {
    travelling: i32,
}
impl Goal for GoToKnownFlower {
    goal_clone!();
    fn name(&self) -> &'static str {
        "BeeGoToKnownFlowerGoal"
    }
    fn flags(&self) -> u8 {
        MOVE
    }
}

/// `BeeGrowCropGoal`: a bee with nectar and a hive grows crops below it.
#[derive(Clone, Debug)]
struct GrowCrop;
impl Goal for GrowCrop {
    goal_clone!();
    fn name(&self) -> &'static str {
        "BeeGrowCropGoal"
    }
    fn flags(&self) -> u8 {
        0
    }
}
/// The state a bee grows a `bee_growables` block to, if it can grow.
fn grown(w: &MobWorld, p: [i32; 3]) -> Option<horizon_world::registry::BlockStateId> {
    let name = w.block_name(p[0], p[1], p[2]);
    if !w.has_tag("minecraft:bee_growables", name) {
        return None;
    }
    let state = w.registry.state(w.world.block(p[0], p[1], p[2]))?;
    let (key, max) = match name {
        "minecraft:beetroots" => ("age", 3),
        // TorchflowerCropBlock.getStateForAge(2): the flower itself.
        "minecraft:torchflower_crop" => {
            let age: i32 = state.properties.get("age")?.parse().ok()?;
            return if age + 1 >= 2 {
                w.registry.default_state("minecraft:torchflower")
            } else {
                let mut s = state.clone();
                s.properties.insert("age".into(), (age + 1).to_string());
                w.registry.id(&s)
            };
        }
        "minecraft:wheat" | "minecraft:carrots" | "minecraft:potatoes" => ("age", 7),
        "minecraft:melon_stem" | "minecraft:pumpkin_stem" => ("age", 7),
        "minecraft:sweet_berry_bush" => ("age", 3),
        // CaveVinesBlock.performBonemeal: berries.
        "minecraft:cave_vines" | "minecraft:cave_vines_plant" => {
            if state.properties.get("berries").map(String::as_str) == Some("true") {
                return None;
            }
            let mut s = state.clone();
            s.properties.insert("berries".into(), "true".into());
            return w.registry.id(&s);
        }
        _ => return None,
    };
    let age: i32 = state.properties.get(key)?.parse().ok()?;
    if age >= max {
        return None;
    }
    let mut s = state.clone();
    s.properties.insert(key.into(), (age + 1).to_string());
    w.registry.id(&s)
}

/// `BeeWanderGoal`: hover to a random spot ahead (or back towards a far hive).
#[derive(Clone, Debug)]
struct Wander;
impl Goal for Wander {
    goal_clone!();
    fn name(&self) -> &'static str {
        "BeeWanderGoal"
    }
    fn flags(&self) -> u8 {
        MOVE
    }
}

/// `FlyingPathNavigation.isStableDestination` for bees: anything above
/// something not air.
pub fn stable_destination(w: &MobWorld, p: [i32; 3]) -> bool {
    !w.facts.is_air(w.world.block(p[0], p[1] - 1, p[2]))
}

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
