use crate::blocks::Fluid;
use crate::health::DamageLevel;
use horizon_core::{geometry::Vec3, random::RandomSource};
use horizon_world::move_gate::MoveGate;
use std::collections::HashMap;

// Nautilus state: `tame`/`owner`, the saddle, `head` the body armour,
// `variant` the zombie's variant (0 temperate, 1 warm), `anim_ticks` the
// dash cooldown and `flag2` dashing, `counter` the attack target cooldown,
// `counter2` the charge cooldown, `anger` the ticks left of `ANGRY_AT`
// (at `ai.last_hurt_by`), `ints[1]` the rider's breath effect given,
// `scratch[1]` a dash whose sound is still to play.
const DASH_COOLDOWN: i32 = 40;
const TIME_BETWEEN_ATTACKS: i32 = 80;
const ANGER_DURATION: i32 = 400;
const ATTACK_TARGET_COOLDOWN: &str = "minecraft:attack_target_cooldown";
/// `ZombieNautilusVariants`: the variant keys in index order.
pub const VARIANTS: [&str; 2] = ["temperate", "warm"];

/// `TamableAnimal.feed`'s heal: twice the food's nutrition, or 1 for an item
/// without a food component (a fish bucket).
pub fn feed_heal(nutrition: Option<i32>) -> f32 {
    nutrition.map_or(1.0, |n| 2.0 * n as f32)
}
/// `ChargeAttack(80, ..., speed, 2, 12, 11, dash)`: rush straight at the
/// target and ram it once.
#[derive(Clone, Debug)]
struct Charge {
    speed: f64,
    start: Vec3,
    velocity: Vec3,
}
impl Charge {
    fn new(speed: f64) -> Self {
        Self {
            speed,
            start: Vec3::ZERO,
            velocity: Vec3::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use std::sync::Arc;





    /// `AbstractNautilus.onPlayerJump` keeps the released bar's scale (0.4 to
    /// 1.0, none while the dash clock runs) and `tickRidden` spends it:
    /// `executeRidersJump` pushes along the rider's look at 0.5 on land,
    /// starts the 40-tick clock, raises DASH and queues the dash sound.
    #[test]
    fn the_dash_runs_off_the_scale_the_released_bar_left() {
        let mut m = ridden();
        assert!(on_player_jump(&mut m, 100));
        assert_eq!(m.rider.expect("the rider stays on").jump, 1.0);
        super::super::equine::ridden_step(&mut m, crate::test_resources::server_loop_context());
        assert_eq!(
            m.rider.expect("the rider stays on").jump,
            0.0,
            "the scale is spent"
        );
        assert_eq!(m.extra.anim_ticks, DASH_COOLDOWN);
        assert!(m.extra.flag2, "DASH");
        assert_eq!(m.extra.scratch[1], 1, "the dash sound plays next tick");
        let k = 0.5 * m.movement_speed();
        assert!(
            (m.velocity.z - k).abs() < 1.0e-9 && m.velocity.x.abs() < 1.0e-9,
            "a dash along +z: {:?} against {k}",
            m.velocity
        );
        // A dash still in its cooldown takes nothing (`dashCooldown > 0`), and
        // neither does an unsaddled nautilus.
        assert!(!on_player_jump(&mut m, 50));
        m.extra.anim_ticks = 0;
        m.extra.set_saddled(false);
        assert!(!on_player_jump(&mut m, 50));
        assert_eq!(m.rider.expect("the rider stays on").jump, 0.0);
    }
}

#[cfg(test)]
mod hostile_target_tests {
    use super::*;
    use std::sync::Arc;

    /// A pool from x -8 to 8, with a stone wall at x = 3 when `wall`.
    fn pool(
        wall: bool,
    ) -> impl FnOnce(&mut crate::world::World, &horizon_world::registry::BlockRegistry) {
        move |world, registry| {
            let water = registry.default_state("minecraft:water").unwrap();
            let stone = registry.default_state("minecraft:stone").unwrap();
            for cx in -1..2 {
                for cz in -1..1 {
                    world
                        .chunks
                        .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
                }
            }
            for x in -8..=18 {
                for y in 60..=66 {
                    for z in -4..=4 {
                        let block = if wall && x == 3 { stone } else { water };
                        world.set(x, y, z, block).unwrap();
                    }
                }
            }
        }
    }


    #[test]
    #[ignore = "daily"]
    fn a_nautilus_only_targets_a_pufferfish_it_sees() {
        let near = Vec3::new(6.5, 62.0, 0.5);
        // 16 out on x and 3 on z: inside the sensor's box, past range(16).
        let far = Vec3::new(16.5, 62.0, 3.5);
        let (Some(open), Some(walled), Some(beyond)) = (
            targets(false, near),
            targets(true, near),
            targets(false, far),
        ) else {
            return;
        };
        assert!(open.contains(&Some(Who::Mob(2))), "{open:?}");
        assert!(open.iter().all(|t| t.is_none() || *t == Some(Who::Mob(2))));
        assert!(walled.iter().all(Option::is_none), "{walled:?}");
        assert!(beyond.iter().all(Option::is_none), "{beyond:?}");
    }
}

#[cfg(test)]
mod cooldown_memory_tests {
    use super::*;
    use horizon_core::nbt::Tag;
    use std::sync::Arc;

    const MEMORY: &str = "minecraft:attack_target_cooldown";
    fn raw(id: &str, state: Option<(i32, Option<i64>)>, no_ai: bool) -> Tag {
        let memory = state.map(|(value, ttl)| {
            let mut entries = vec![("value", Tag::Int(value))];
            if let Some(ttl) = ttl {
                entries.push(("ttl", Tag::Long(ttl)));
            }
            Tag::compound(entries)
        });
        let memories = Tag::compound(memory.into_iter().map(|m| (MEMORY, m)));
        let mut fields = vec![
            ("id", Tag::String(id.into())),
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(4.5f64.to_bits()),
                    Tag::Double(3.0f64.to_bits()),
                    Tag::Double(4.5f64.to_bits()),
                ]),
            ),
            ("NoAI", Tag::Byte(i8::from(no_ai))),
            ("NoGravity", Tag::Byte(1)),
            ("Health", Tag::Float(10.0f32.to_bits())),
            ("PersistenceRequired", Tag::Byte(1)),
            ("Brain", Tag::compound([("memories", memories)])),
        ];
        if !no_ai {
            fields.push(("Owner", Tag::IntArray(vec![1, 2, 3, 4])));
        }
        Tag::compound(fields)
    }
    fn memory(tag: &Tag) -> Option<&Tag> {
        tag.get("Brain")?.get("memories")?.get(MEMORY)
    }
    fn expected(state: Option<(i32, Option<i64>)>) -> Option<Tag> {
        state.map(|(value, ttl)| {
            let mut entries = vec![("value", Tag::Int(value))];
            if let Some(ttl) = ttl.filter(|&ttl| ttl != i64::MAX) {
                entries.push(("ttl", Tag::Long(ttl)));
            }
            Tag::compound(entries)
        })
    }
    #[test]
    fn decoded_cooldown_presence_ttl_and_model_mutation_save_without_raw() {
        let mut failures = Vec::new();
        for (id, species) in [
            ("minecraft:nautilus", Species::Nautilus),
            ("minecraft:zombie_nautilus", Species::ZombieNautilus),
        ] {
            let mut kind = (*crate::mobs::sensor_scan_tests::kind()).clone();
            kind.id = id.into();
            kind.species = species;
            let types = std::collections::HashMap::from([(id.to_owned(), Arc::new(kind))]);
            for state in [
                None,
                Some((0, None)),
                Some((37, None)),
                Some((37, Some(5))),
                Some((37, Some(i64::MAX))),
            ] {
                let tag = raw(id, state, true);
                let mut m = crate::entity_nbt::mob_from_nbt(&tag, &types, 7, None)
                    .expect("actual class decode");
                assert_eq!(m.species(), species);
                assert!(m.no_ai && m.no_gravity && m.alive());
                assert_eq!(m.health, 10.0);
                assert_eq!(m.extra.counter, state.map_or(0, |s| s.0));
                for source in [None, Some(&tag)] {
                    let out = saved(&m, source);
                    if memory(&out).cloned() != expected(state) {
                        failures.push(format!(
                            "{id} {state:?} raw={} saved={:?}",
                            source.is_some(),
                            memory(&out)
                        ));
                    }
                }
                if memory(&saved(&m.clone(), None)).cloned() != expected(state) {
                    failures.push(format!("{id} cloned model lost memory presence/lifetime"));
                }
                if state.is_some() {
                    m.extra.counter = 41; // The live value changes; the raw compound still holds the old value.
                    let want = state.map(|(_, ttl)| (41, ttl));
                    if memory(&saved(&m, Some(&tag))).cloned() != expected(want) {
                        failures.push(format!("{id} stale raw value/TTL replaced modeled state"));
                    }
                }
                // A later valid empty memory map clears the class memory, not merely its value.
                from_nbt(&mut m, &raw(id, None, true));
                if memory(&saved(&m, Some(&tag))).is_some() {
                    failures.push(format!("{id} valid empty map retained stale memory"));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "saved nautilus cooldown state mismatches: {failures:?}"
        );
    }

    #[test]
    #[ignore = "daily"]
    fn actual_mob_ticks_retain_noai_lifetime_and_apply_active_memory_boundaries() {
        let root = std::path::PathBuf::from(
            std::env::var_os("HORIZON_TEST_RESOURCES").expect("actual prepared resources"),
        );
        let mut failures = Vec::new();
        let ran = crate::mobs::world::with_resource_test_world(
            &root,
            &[],
            true,
            true,
            |world, registry| {
                let water = registry.default_state("minecraft:water").unwrap();
                let stone = registry.default_state("minecraft:stone").unwrap();
                world
                    .chunks
                    .insert((0, 0), Arc::new(crate::chunk::Chunk::empty(0, 0)));
                for x in 0..16 {
                    for z in 0..16 {
                        world.set(x, 0, z, stone).unwrap();
                        for y in 1..6 {
                            world.set(x, y, z, water).unwrap();
                        }
                    }
                }
            },
            |world, types| {
                let types: std::collections::HashMap<_, _> = types
                    .types
                    .iter()
                    .map(|(id, kind)| (id.clone(), Arc::new(kind.clone())))
                    .collect();
                for id in ["minecraft:nautilus", "minecraft:zombie_nautilus"] {
                    for (no_ai, state) in [
                        (true, None),
                        (true, Some((0, None))),
                        (true, Some((37, None))),
                        (true, Some((37, Some(5)))),
                        (true, Some((37, Some(i64::MAX)))),
                        (false, Some((37, Some(5)))),
                        (false, Some((37, Some(1)))),
                        (false, Some((37, Some(0)))),
                    ] {
                        let tag = raw(id, state, no_ai);
                        let mut m = crate::entity_nbt::mob_from_nbt(&tag, &types, 7, None).unwrap();
                        m.attrs.speed = 0.0;
                        let mut test_level = crate::health::TestLevel::new(140);
                        let mut events = Vec::new();
                        for elapsed in 0..=6 {
                            if elapsed > 0 {
                                let mut w = world.clone();
                                w.game_time += elapsed;
                                assert!(m.tick(&w, &mut test_level.level(), &mut events));
                                assert!(m.fluids.in_water && m.fluids.eye_in_water);
                                assert!(m.ai.target.is_none(), "safe no-attack control");
                                assert!(
                                    !events.iter().any(|e| matches!(
                                        e,
                                        MobEvent::AttackPlayer(..) | MobEvent::AttackMob { .. }
                                    )),
                                    "no attack fixture"
                                );
                            }
                            assert_eq!(m.health, 10.0);
                            assert_eq!(m.no_ai, no_ai);
                            assert!(m.no_gravity);
                            let want = if no_ai || elapsed == 0 {
                                state
                            } else if state.unwrap().1 == Some(0) {
                                None
                            } else {
                                Some((37 - elapsed as i32, None))
                            };
                            if memory(&saved(&m, None)).cloned() != expected(want) {
                                failures.push(format!(
                                    "{id} NoAI={no_ai} {state:?} tick={elapsed}: {:?}",
                                    memory(&saved(&m, None))
                                ));
                            }
                        }
                    }
                }
            },
        );
        assert!(ran, "actual prepared world route must execute");
        assert!(
            failures.is_empty(),
            "nautilus cooldown tick/save mismatches: {failures:?}"
        );
    }
}

#[cfg(test)]
mod feed_heal_tests {
    use super::feed_heal;

    #[test]
    fn heals_twice_the_nutrition_of_vanilla_foods() {
        // Foods.java: cod 2, pufferfish 1, cooked cod 5, cooked salmon 6.
        assert_eq!(feed_heal(Some(2)), 4.0);
        assert_eq!(feed_heal(Some(1)), 2.0);
        assert_eq!(feed_heal(Some(5)), 10.0);
        assert_eq!(feed_heal(Some(6)), 12.0);
    }

    #[test]
    fn item_without_food_component_heals_one() {
        assert_eq!(feed_heal(None), 1.0);
    }
}
