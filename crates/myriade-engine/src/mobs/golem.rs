use crate::projectiles::{Projectile, ProjectileKind};
use horizon_core::{geometry::Vec3, random::RandomSource};

/// `ResetUniversalAngerTargetGoal` with `alertOthersOfSameType` false (the
/// iron golem's): with the `universal_anger` rule on, a golem whose latest
/// attacker is a player, hit after the one it last handled, forgets its
/// target and starts its anger timer over.
#[derive(Clone, Debug, Default)]
pub struct ResetUniversalAnger {
    /// `lastHurtByPlayerTimestamp`.
    timestamp: i32,
}
/// `SectionPos.of`: the section a position is in.
fn section_of(p: Vec3) -> [i32; 3] {
    [
        (p.x.floor() as i32) >> 4,
        (p.y.floor() as i32) >> 4,
        (p.z.floor() as i32) >> 4,
    ]
}
/// `SectionPos.center` as `Vec3.atBottomCenterOf` reads it.
fn section_center(s: [i32; 3]) -> [f64; 3] {
    [
        f64::from(s[0] * 16 + 8) + 0.5,
        f64::from(s[1] * 16 + 8),
        f64::from(s[2] * 16 + 8) + 0.5,
    ]
}
/// `DefendVillageTargetGoal`: a player that a villager nearby has no
/// good word for (reputation of -100 or worse) is attacked on sight.
#[derive(Clone, Debug)]
struct DefendVillage;
/// `MoveTowardsTargetGoal`: close in on a target within range.
#[derive(Clone, Debug)]
struct MoveTowardsTarget {
    speed: f64,
    within: f64,
    /// The spot `canUse` picked, which `start` walks to.
    wanted: [f64; 3],
}
/// `OfferFlowerGoal`: by day, now and then, hold out a poppy to a villager.
#[derive(Clone, Debug)]
struct OfferFlower {
    ticks: i32,
    villager: Option<u32>,
}
fn triangle_wave(x: f32, period: f32) -> f32 {
    ((x % period) - period * 0.5).abs() / (period * 0.5) * 2.0 - 1.0
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use std::sync::Arc;


    fn villager_at(id: u32, at: Vec3) -> crate::mobs::Seen {
        mob(id, Species::Villager, at).seen(None, &crate::items::Items::empty())
    }

    fn pick(w: &MobWorld) -> Option<u32> {
        let mut golem = mob(1, Species::IronGolem, Vec3::new(0.5, 64.0, 0.5));
        gift_candidate(&mut golem, w)
    }

    fn with_chunks(world: &mut crate::world::World, _: &horizon_world::registry::BlockRegistry) {
        for cx in -2..2 {
            for cz in -2..2 {
                world.chunks.insert(
                    (cx, cz),
                    std::sync::Arc::new(crate::chunk::Chunk::empty(cx, cz)),
                );
            }
        }
    }

    /// `OfferFlowerGoal`: the search box is the golem's box inflated by
    /// (6, 2, 6) and the candidate must be within six blocks (Euclidean).
    #[test]
    #[ignore = "daily"]
    fn the_poppy_goes_to_a_villager_in_the_box_within_six_blocks() {
        let cases = [
            (Vec3::new(4.5, 64.0, 0.5), true),
            // 6.5 blocks on one axis: outside the range.
            (Vec3::new(7.0, 64.0, 0.5), false),
            // About 8.5 blocks away, inside the per-axis box of six.
            (Vec3::new(6.0, 64.0, 6.0), false),
            // Five blocks above: outside the box's two-block rise.
            (Vec3::new(0.5, 69.0, 0.5), false),
        ];
        for (at, offered) in cases {
            let villager = villager_at(2, at);
            crate::mobs::world::with_built_test_world(&[villager], with_chunks, |w, _| {
                assert_eq!(pick(w).is_some(), offered, "villager at {at:?}");
            });
        }
    }

    /// The candidate must also be in sight, and a fixed-time dimension is
    /// never bright outside.
    #[test]
    #[ignore = "daily"]
    fn the_poppy_needs_sight_and_a_dimension_with_a_day() {
        let villager = villager_at(2, Vec3::new(4.5, 64.0, 0.5));
        let wall = |world: &mut crate::world::World, r: &horizon_world::registry::BlockRegistry| {
            with_chunks(world, r);
            let stone = r.default_state("minecraft:stone").unwrap();
            for y in 62..70 {
                for z in -6..8 {
                    world.set(2, y, z, stone).unwrap();
                }
            }
        };
        crate::mobs::world::with_built_test_world(std::slice::from_ref(&villager), wall, |w, _| {
            assert_eq!(pick(w), None, "a villager behind a wall");
        });
        crate::mobs::world::with_built_test_world(&[villager], with_chunks, |w, _| {
            assert!(bright_outside(w));
            assert_eq!(pick(w), Some(2));
            let mut end = w.clone();
            end.dimension_type =
                horizon_world::dimension_type::DimensionFacts::vanilla("minecraft:the_end")
                    .unwrap();
            assert!(!bright_outside(&end));
            let mut night = w.clone();
            night.daylight = false;
            assert!(!bright_outside(&night));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn snow_trail_posts_block_place_with_golem_and_snow_state() {
        let ran = crate::mobs::world::with_built_test_world(
            &[],
            |world, reg| {
                world
                    .chunks
                    .insert((0, 0), Arc::new(crate::chunk::Chunk::empty(0, 0)));
                world
                    .set(1, 0, 1, reg.default_state("minecraft:stone").unwrap())
                    .unwrap();
            },
            |w, _| {
                let mut golem = mob(42, Species::SnowGolem, Vec3::new(1.5, 1.0, 1.5));
                let mut events = Vec::new();
                snow_ai_step(&mut golem, w, &mut events);
                let snow = w.registry.default_state("minecraft:snow").unwrap();
                let posted: Vec<_> = events
                    .iter()
                    .filter_map(|e| match e {
                        MobEvent::GameEvent {
                            event,
                            at,
                            source,
                            state,
                        } => Some((*event, *at, *source, *state)),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    posted,
                    vec![("minecraft:block_place", [1.5, 1.5, 1.5], 42, snow)]
                );
                events.clear();
                let mut denied = w.clone();
                denied.mob_griefing = false;
                snow_ai_step(&mut golem, &denied, &mut events);
                assert!(
                    events
                        .iter()
                        .all(|e| !matches!(e, MobEvent::GameEvent { .. }))
                );
            },
        );
        // Without the vanilla data under .local/myriade-engine the world cannot be built: the test has nothing to check.
        assert!(ran, "vanilla data is missing");
    }

    /// `SnowGolem.aiStep` tests `isAir` (cave and void air too) and
    /// `SnowLayerBlock.canSurvive`: the two snow-layer tags, then the block
    /// below's top face (`Block.isFaceFull(collisionShape, UP)`), or a snow
    /// layer eight deep, whose collision shape is only 14/16 high.
    #[test]
    #[ignore = "daily"]
    fn snow_trail_follows_is_air_and_the_snow_layer_tags() {
        let cases: &[(&str, &[(&str, &str)], &str, bool)] = &[
            ("minecraft:stone", &[], "minecraft:cave_air", true),
            ("minecraft:stone", &[], "minecraft:void_air", true),
            ("minecraft:soul_sand", &[], "minecraft:air", true),
            ("minecraft:mud", &[], "minecraft:air", true),
            ("minecraft:honey_block", &[], "minecraft:air", true),
            ("minecraft:ice", &[], "minecraft:air", false),
            ("minecraft:packed_ice", &[], "minecraft:air", false),
            ("minecraft:barrier", &[], "minecraft:air", false),
            ("minecraft:stone", &[], "minecraft:water", false),
            // A top slab's and upside-down stairs' top face is full even
            // though neither is a full block; a bottom one's is not.
            (
                "minecraft:stone_slab",
                &[("type", "top")],
                "minecraft:air",
                true,
            ),
            (
                "minecraft:stone_slab",
                &[("type", "bottom")],
                "minecraft:air",
                false,
            ),
            (
                "minecraft:stone_stairs",
                &[("half", "top"), ("facing", "north")],
                "minecraft:air",
                true,
            ),
            (
                "minecraft:stone_stairs",
                &[("half", "bottom"), ("facing", "north")],
                "minecraft:air",
                false,
            ),
            // Eight layers of snow take a layer on top, seven do not.
            ("minecraft:snow", &[("layers", "8")], "minecraft:air", true),
            ("minecraft:snow", &[("layers", "7")], "minecraft:air", false),
        ];
        for (floor, properties, here, expect) in cases {
            crate::mobs::world::with_built_test_world(
                &[],
                |world, reg| {
                    let mut floor_state = reg
                        .state(reg.default_state(floor).unwrap())
                        .unwrap()
                        .clone();
                    for (key, value) in *properties {
                        floor_state
                            .properties
                            .insert((*key).into(), (*value).into());
                    }
                    let floor = reg.id(&floor_state).unwrap();
                    world
                        .chunks
                        .insert((0, 0), Arc::new(crate::chunk::Chunk::empty(0, 0)));
                    for x in 0..3 {
                        for z in 0..3 {
                            world.set(x, 0, z, floor).unwrap();
                            world
                                .set(x, 1, z, reg.default_state(here).unwrap())
                                .unwrap();
                        }
                    }
                },
                |w, _| {
                    let mut golem = mob(7, Species::SnowGolem, Vec3::new(1.5, 1.0, 1.5));
                    let mut events = Vec::new();
                    snow_ai_step(&mut golem, w, &mut events);
                    let placed = events
                        .iter()
                        .filter(|event| {
                            let MobEvent::PlaceBlock { mob, pos, state } = event else {
                                return false;
                            };
                            assert_eq!(*mob, golem.id);
                            assert_eq!(*pos, [1, 1, 1]);
                            assert_eq!(&*w.facts.get(*state).name, "minecraft:snow");
                            true
                        })
                        .count();
                    assert_eq!(
                        placed,
                        usize::from(*expect),
                        "{floor}{properties:?} under {here}"
                    );
                },
            );
        }
    }

    /// `MoveTowardsTargetGoal.canUse` picks a random spot within 16 blocks and
    /// 7 up or down toward the target and keeps it for `start`, so the golem
    /// walks to that spot rather than straight at the entity.
    #[test]
    #[ignore = "daily"]
    fn an_iron_golem_walks_to_a_spot_near_its_target() {
        let build = |world: &mut crate::world::World,
                     registry: &horizon_world::registry::BlockRegistry| {
            let stone = registry.default_state("minecraft:stone").unwrap();
            for cx in -2..2 {
                for cz in -2..2 {
                    world.chunks.insert(
                        (cx, cz),
                        std::sync::Arc::new(crate::chunk::Chunk::empty(cx, cz)),
                    );
                }
            }
            for x in -24..24 {
                for z in -24..24 {
                    for y in 60..=63 {
                        world.set(x, y, z, stone).unwrap();
                    }
                }
            }
        };
        crate::mobs::world::with_built_test_world(&[], build, |w, _| {
            let mut w = w.clone();
            w.player = Some(test_player(Vec3::new(8.5, 64.0, 0.5)));
            // The spot is drawn at random, and a flat world only offers a few
            // heights: the seed follows the mob's id, so try the ids in turn
            // (vanilla gives the pick ten candidates before giving up).
            let mut found = None;
            for id in 1..400 {
                let mut m = mob(id, Species::IronGolem, Vec3::new(0.5, 64.0, 0.5));
                m.ai.target = Some(Who::Player);
                let mut goal = MoveTowardsTarget {
                    speed: 0.9,
                    within: 32.0,
                    wanted: [0.0; 3],
                };
                let mut events = Vec::new();
                let mut test_level = crate::health::TestLevel::new(0);
                let level = &mut test_level.level();
                let mut cx = Cx {
                    w: &w,
                    level,
                    events: &mut events,
                };
                if goal.can_use(&mut m, &mut cx) {
                    found = Some((m, goal));
                    break;
                }
            }
            let (m, goal) = found.expect("a spot among ten candidates");
            let wanted = goal.wanted;
            let (dx, dz) = (wanted[0] - m.position.x, wanted[2] - m.position.z);
            assert!((dx * dx + dz * dz).sqrt() <= 16.0, "within 16 blocks");
            assert!(
                (wanted[1] - m.position.y).abs() <= 7.0,
                "within 7 up or down"
            );
            // `DefaultRandomPos.getPosTowards` only accepts a spot within a
            // quarter turn of the target, so the horizontal step to the spot
            // is at most 90 degrees off the step to the target.
            let to_target = (8.5 - m.position.x, 0.5 - m.position.z);
            let to_spot = (wanted[0] - m.position.x, wanted[2] - m.position.z);
            assert!(
                to_target.0 * to_spot.0 + to_target.1 * to_spot.1 >= 0.0,
                "the spot is toward the target"
            );
        });
    }

    /// `ResetUniversalAngerTargetGoal`: with the rule on, a golem hurt by a
    /// player forgets its target once and not again until hurt again; with
    /// the rule off it does nothing.
    #[test]
    #[ignore = "daily"]
    fn a_golem_hurt_by_a_player_forgets_its_target_once_under_universal_anger() {
        for rule in [true, false] {
            assert!(
                with_test_world(&[], |w, _| {
                    let mut w = w.clone();
                    w.universal_anger = rule;
                    let mut golem = mob(1, Species::IronGolem, Vec3::new(0.5, 64.0, 0.5));
                    golem.age = 50;
                    golem.ai.target = Some(Who::Mob(2));
                    golem.extra.anger = 5;
                    golem.ai.last_hurt_by = Some(Who::Player);
                    golem.ai.last_hurt_by_time = 10;
                    let mut goal = ResetUniversalAnger::default();
                    let mut events = Vec::new();
                    let mut test_level = crate::health::TestLevel::new(0);
                    let mut cx = Cx {
                        w: &w,
                        level: &mut test_level.level(),
                        events: &mut events,
                    };
                    assert_eq!(goal.can_use(&mut golem, &mut cx), rule);
                    if !rule {
                        return;
                    }
                    goal.start(&mut golem, &mut cx);
                    assert_eq!(golem.ai.target, None);
                    assert_eq!(golem.ai.last_hurt_by, None);
                    assert!((400..=780).contains(&golem.extra.anger));
                    assert!(!goal.can_use(&mut golem, &mut cx), "not again");
                    // Hurt again: a newer hit by the player.
                    golem.ai.last_hurt_by = Some(Who::Player);
                    golem.ai.last_hurt_by_time = 60;
                    assert!(goal.can_use(&mut golem, &mut cx));
                }),
                "vanilla data is missing"
            );
        }
    }

    /// The golems' target predicates test `instanceof Enemy`: the zombie
    /// horse and camel husk share the monster category but are not `Enemy`.
    #[test]
    fn mounts_in_the_monster_category_are_not_golem_prey() {
        let prey = |sp| {
            let mut m = mob(2, sp, Vec3::new(0.5, 64.0, 0.5));
            let mut kind = (*m.kind).clone();
            kind.category = "monster".into();
            m.kind = Arc::new(kind);
            golem_prey(&m.seen(None, &crate::items::Items::empty()))
        };
        assert!(prey(Species::Zombie));
        assert!(!prey(Species::Creeper));
        assert!(!prey(Species::ZombieHorse));
        assert!(!prey(Species::CamelHusk));
    }

    /// `IronGolem.canAttack` refuses creepers, and `HurtByTargetGoal.canUse`
    /// asks it: the creeper that hurt the golem is never fought back, while
    /// a zombie that hurts it is.
    #[test]
    #[ignore = "daily"]
    fn a_creeper_that_hurts_an_iron_golem_is_never_fought_back() {
        let creeper = mob(2, Species::Creeper, Vec3::new(1.5, 64.0, 0.5));
        let zombie = mob(3, Species::Zombie, Vec3::new(1.5, 64.0, 0.5));
        assert!(
            with_test_world(
                &[
                    creeper.seen(None, &crate::items::Items::empty()),
                    zombie.seen(None, &crate::items::Items::empty()),
                ],
                |w, _| {
                    let mut golem = mob(1, Species::IronGolem, Vec3::new(0.5, 64.0, 0.5));
                    let mut goal = HurtByTarget::new();
                    let mut events = Vec::new();
                    let mut test_level = crate::health::TestLevel::new(0);
                    let level = &mut test_level.level();
                    let mut cx = Cx {
                        w,
                        level,
                        events: &mut events,
                    };

                    golem.ai.last_hurt_by = Some(Who::Mob(2));
                    golem.ai.last_hurt_by_time = 1;
                    assert!(!goal.can_use(&mut golem, &mut cx));
                    assert_eq!(golem.ai.target, None, "a creeper is refused");

                    golem.ai.last_hurt_by = Some(Who::Mob(3));
                    golem.ai.last_hurt_by_time = 2;
                    assert!(goal.can_use(&mut golem, &mut cx));
                    goal.start(&mut golem, &mut cx);
                    assert_eq!(golem.ai.target, Some(Who::Mob(3)));
                },
            ),
            "vanilla data is missing"
        );
    }

    /// A village golem (not built by a player) still goes after the player
    /// who angered it.
    #[test]
    #[ignore = "daily"]
    fn a_village_golem_still_targets_the_player_who_angered_it() {
        assert!(
            with_test_world(&[], |w, _| {
                let mut w = w.clone();
                w.player = Some(test_player(Vec3::new(3.5, 64.0, 0.5)));
                let mut golem = mob(1, Species::IronGolem, Vec3::new(0.5, 64.0, 0.5));
                golem.extra.anger = 100;
                golem.extra.angry_at = Some(Who::Player);
                let mut events = Vec::new();
                let mut test_level = crate::health::TestLevel::new(0);
                let level = &mut test_level.level();
                let mut cx = Cx {
                    w: &w,
                    level,
                    events: &mut events,
                };
                let mut goal = NearestTarget::player(true)
                    .interval(0)
                    .when(super::super::tame::angry_at_player);
                assert!(goal.can_use(&mut golem, &mut cx));
                goal.start(&mut golem, &mut cx);
                assert_eq!(golem.ai.target, Some(Who::Player));
            }),
            "vanilla data is missing"
        );
    }
    /// `DefendVillageTargetGoal` finds players with
    /// `TargetingConditions.forCombat().range(64)`, whose range is scaled by
    /// visibility: an invisible player at 9.5 blocks is out of reach.
    #[test]
    #[ignore = "daily"]
    fn an_invisible_player_does_not_make_a_golem_defend_the_village() {
        use crate::gossip::{GossipType, Gossips};
        let mut outcomes = Vec::new();
        if !with_test_world(&[], |w, _| {
            for invisible in [false, true] {
                let mut world = w.clone();
                let mut p = test_player(Vec3::new(9.5, 64.0, 0.5));
                p.invisible = invisible;
                world.player = Some(p.clone());
                let mut villager = mob(2, Species::Villager, Vec3::new(2.5, 64.0, 0.5));
                let mut gossips = Gossips::default();
                gossips.add(p.uuid, GossipType::MajorNegative, 100);
                villager.extra.villager = Some(Box::new(crate::mobs::villager::Ext {
                    gossips,
                    ..Default::default()
                }));
                let seen = [villager.seen(None, &crate::items::Items::empty())];
                let world = crate::mobs::MobWorld {
                    others: &seen,
                    ..world
                };
                let mut golem = mob(1, Species::IronGolem, Vec3::new(0.5, 64.0, 0.5));
                golem.ai.targets.add(1, DefendVillage);
                let mut lr = crate::health::TestLevel::new(0);
                let mut events = Vec::new();
                let mut cx = Cx {
                    w: &world,
                    level: &mut lr.level(),
                    events: &mut events,
                };
                let mut goals = std::mem::take(&mut golem.ai.targets);
                goals.tick(&mut golem, &mut cx);
                outcomes.push(goals.is_running("DefendVillage"));
                golem.ai.targets = goals;
            }
        }) {
            return;
        }
        assert_eq!(
            outcomes,
            vec![true, false],
            "a visible hostile player is defended against; an invisible one is not"
        );
    }
}

#[cfg(test)]
mod created_golem_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use std::sync::Arc;


    /// `IronGolem.canAttack`: a golem a player built never takes a player as
    /// its target, though a village golem does and both still go after
    /// monsters. The world holds one player, so "its builder" and "a player"
    /// are the same check, as they are in vanilla.
    #[test]
    #[ignore = "daily"]
    fn a_golem_a_player_built_never_targets_one() {
        let Some((mut built, mut village, zombie)) = angry_golems() else {
            return;
        };
        assert!(
            with_test_world(&[], |w, _| {
                let mut w = w.clone();
                w.player = Some(test_player(Vec3::new(2.5, 64.0, 0.5)));
                assert_eq!(
                    target(&mut built, &w, 200),
                    None,
                    "a golem a player built turned on its builder"
                );
                assert_eq!(
                    target(&mut village, &w, 200),
                    Some(Who::Player),
                    "a village golem goes after the player"
                );
            }),
            "vanilla data is missing"
        );
        assert!(
            with_test_world(&[zombie], |w, _| {
                assert_eq!(
                    target(&mut built, w, 200),
                    Some(Who::Mob(3)),
                    "a golem a player built still fights monsters"
                );
            }),
            "vanilla data is missing"
        );
    }
}

#[cfg(test)]
mod crack_sound_tests {
    use super::*;
    use crate::mobs::Damage;
    use std::sync::Arc;

    /// `LivingEntity.actuallyHurt`: absorbed damage from a player is credited in
    /// tenths; other attackers and a hit with nothing absorbed credit nothing.
    #[test]
    fn player_hit_credits_dealt_absorbed() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = golem();
        // Absorption I: max_absorption 4, which a hit clamps the amount to.
        m.add_effect("absorption", 600, 0);
        m.absorption = 4.0;
        assert!(m.damage(level, 1.5, Damage::player([0.0, 0.0, 1.0])));
        assert_eq!(m.dealt_absorbed, 15);
        m.invulnerable = 0;
        assert!(m.damage(level, 10.0, Damage::player([0.0, 0.0, 1.0])));
        assert_eq!(m.dealt_absorbed, 15 + 25);
        m.invulnerable = 0;
        m.damage(level, 2.0, Damage::player([0.0, 0.0, 1.0]));
        assert_eq!(m.dealt_absorbed, 40);
        let mut other = golem();
        other.add_effect("absorption", 600, 0);
        other.absorption = 4.0;
        other.damage(level, 1.5, Damage::of(crate::mobs::DamageKind::Generic));
        assert_eq!(other.dealt_absorbed, 0);
    }

    /// `IronGolem.hurtServer`: a hit that leaves the golem in a new
    /// `Crackiness.Level` (`Crackiness.GOLEM` is 0.75, 0.5, 0.25) plays
    /// `entity.iron_golem.damage` once, and hits inside one level play none.
    #[test]
    fn a_crack_is_heard_once_at_the_threshold() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = golem();
        assert_eq!(cracks(level, &mut m, 30.0), 1, "100 to 70 crosses 0.75");
        assert_eq!(
            cracks(level, &mut m, 10.0),
            0,
            "70 to 60 stays at the same crack"
        );
        assert_eq!(cracks(level, &mut m, 20.0), 1, "60 to 40 crosses 0.5");
        assert_eq!(
            cracks(level, &mut m, 10.0),
            0,
            "40 to 30 stays at the same crack"
        );
        assert_eq!(cracks(level, &mut m, 10.0), 1, "30 to 20 crosses 0.25");
    }
}

#[cfg(test)]
mod snow_golem_stroll_tests {
    use super::*;
    use std::sync::Arc;


    /// `SnowGolem.registerGoals` adds `new WaterAvoidingRandomStrollGoal(this, 1.0, 1.0000001E-5F)`
    /// (the bytecode's float constant): the chance of a plain stroll instead of a land one is
    /// that, not the two-argument constructor's default 0.001.
    #[test]
    fn a_snow_golem_strolls_with_the_rare_probability() {
        let mut m = snow_golem();
        // The made-up type brings goals of its own.
        m.ai.goals.clear();
        register_snow(&mut m);
        let goals = format!("{:?}", m.ai.goals);
        let rare = format!("water_avoiding: {:?}", Some(1.000_000_1e-5_f32));
        let default = format!("water_avoiding: {:?}", Some(0.001_f32));
        assert!(goals.contains(&rare), "{goals}");
        assert!(!goals.contains(&default), "{goals}");
        // One stroll goal, not two.
        assert_eq!(goals.matches("water_avoiding:").count(), 1, "{goals}");
    }
}
