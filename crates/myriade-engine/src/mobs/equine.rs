use horizon_core::geometry::Vec3;
use horizon_core::geometry::trig::Mth;
use horizon_core::random::RandomSource;
use horizon_world::native_load_context::LoopContext;
use std::f32::consts::PI;

pub const COLORS: [&str; 7] = [
    "white",
    "creamy",
    "chestnut",
    "brown",
    "black",
    "gray",
    "darkbrown",
];
/// `Horse.Variant.getSerializedName` in id order (WHITE 0 to DARK_BROWN 6):
/// the names a `minecraft:horse/variant` component and the save's `Variant`
/// byte carry. [`COLORS`] is the same order, but names the texture
/// (`horse_darkbrown`), which drops the underscore.
pub const VARIANT_NAMES: [&str; 7] = [
    "white",
    "creamy",
    "chestnut",
    "brown",
    "black",
    "gray",
    "dark_brown",
];
pub const MARKINGS: [&str; 5] = ["", "white", "whitefield", "whitedots", "blackdots"];

// AnimState slots.
pub const EAT: usize = 0;
pub const STAND: usize = 1;
pub const MOUTH: usize = 2;
/// `AbstractHorse.tailCounter`: client-side, it only drives the model.
pub const TAIL: usize = 3;

/// What a rider asks of its mount this tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rider {
    pub strafe: f32,
    pub forward: f32,
    pub yaw: f32,
    pub pitch: f32,
    /// `playerJumpPendingScale` from the jump bar.
    pub jump: f32,
    /// Holding the mount's steering stick (pigs, striders).
    pub steer: bool,
    /// Sprinting (camels run faster).
    pub sprint: bool,
    /// Holding the jump key (a happy ghast rises).
    pub jumping: bool,
    /// Holding the sneak key (`Player.isShiftKeyDown`): a mount whose `entity_mount` listener
    /// kept a sneaking rider on reads it here.
    pub sneak: bool,
    /// A server's player whose own client moves the mount
    /// (`Entity.isClientAuthoritative`): here it only turns (`tickRidden`).
    pub client: bool,
}

/// The coat and markings a horse's `Variant` int shows: `Horse.getVariant`
/// is `Variant.byId(typeVariant & 255)` and `getMarkings`
/// `Markings.byId((typeVariant & 0xFF00) >> 8)`, both
/// `OutOfBoundsStrategy.WRAP` over 7 coats and 5 markings.
pub fn horse_look(type_variant: i32) -> (u8, u8) {
    (
        ((type_variant & 0xFF) % 7) as u8,
        (((type_variant >> 8) & 0xFF) % 5) as u8,
    )
}
/// `Horse.addAdditionalSaveData` writes `typeVariant` as it is: the int the
/// horse was loaded with while it still shows this coat and these markings
/// (9 stays 9, not 2), otherwise the two ids packed.
pub fn saved_variant(loaded: Option<i32>, coat: u8, markings: u8) -> i32 {
    loaded
        .filter(|v| horse_look(*v) == (coat, markings))
        .unwrap_or(i32::from(coat) | i32::from(markings) << 8)
}

/// `PlayerRideableJumping.getPlayerJumpPendingScale`: the charge `data` out of
/// 100 as the scale a mount leaps or dashes with (a full bar is 1.0 and the
/// shortest is 0.4).
pub fn jump_pending_scale(data: i32) -> f32 {
    if data >= 90 {
        1.0
    } else {
        0.4 + 0.4 * data as f32 / 90.0
    }
}
/// `AbstractHorse.createOffspringAttribute`.
pub fn offspring(a: f64, b: f64, min: f64, max: f64, r: &mut impl RandomSource) -> f64 {
    let (a, b) = (a.clamp(min, max), b.clamp(min, max));
    let margin = 0.15 * (max - min);
    let range = (a - b).abs() + margin * 2.0;
    let quality = (r.next_double() + r.next_double() + r.next_double()) / 3.0 - 0.5;
    let v = (a + b) / 2.0 + range * quality;
    if v > max {
        max - (v - max)
    } else if v < min {
        min + (min - v)
    } else {
        v
    }
}
/// `AbstractHorse.setOffspringAttributes`, drawn before later `aiStep` rolls.
#[derive(Clone, Debug)]
pub struct OffspringAttributes {
    pub max_health: f32,
    pub jump: f64,
    pub speed: f64,
}
impl OffspringAttributes {
}
/// A horse, donkey, mule or llama's inheritance, decided by its breeding goal.
#[derive(Clone, Debug)]
pub struct Offspring {
    pub attributes: OffspringAttributes,
    pub variant: u8,
    /// Horse markings or llama strength; unused for donkeys and mules.
    pub variant2: u8,
}
/// `RunAroundLikeCrazyGoal`: an untamed horse with a rider bolts about until
/// it is tamed (temper) or throws the rider off.
#[derive(Clone, Debug)]
pub(super) struct RunAroundLikeCrazy {
    /// The spot `canUse` picked, which `start` walks to.
    pos: Option<[f64; 3]>,
}
impl RunAroundLikeCrazy {
    pub(super) fn new() -> Self {
        Self { pos: None }
    }
}


/// What `Horse`, `AbstractChestedHorse` (donkeys, mules, llamas) and
/// `ZombieHorse.mobInteract` do with a held item before
/// `AbstractHorse.mobInteract`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldItem {
    /// `isFood`: `fedFood`, whether it eats or not.
    Feed,
    /// Untamed: `makeMad` instead of taking the rider.
    Refused,
}
/// `RandomStandGoal`.
#[derive(Clone, Debug)]
struct RandomStand {
    next: i32,
}
/// `AbstractHorse.isWoodSoundType`: WOOD, NETHER_WOOD, STEM, CHERRY_WOOD and
/// BAMBOO_WOOD (not HARD_CROP, which only shares WOOD's step).
fn is_wood_sound_type(sounds: &myriade_types::BlockSoundType) -> bool {
    let step = sounds
        .step
        .strip_prefix("minecraft:")
        .unwrap_or(&sounds.step);
    let place = sounds
        .place
        .strip_prefix("minecraft:")
        .unwrap_or(&sounds.place);
    match step {
        "block.wood.step" => place == "block.wood.place",
        "block.nether_wood.step"
        | "block.stem.step"
        | "block.cherry_wood.step"
        | "block.bamboo_wood.step" => true,
        _ => false,
    }
}

/// How long a client counts a rear the server's flags say is on. A client
/// never runs `standCounter` (`standIfPossible` only sets it on the server);
/// `isStanding` is just the synched flag. The mount a client drives does run
/// `stand_tick`, so a counter of 1 would end the rear a tick after the flag
/// arrived, long before the jump lands. The next flags without the bit end it.
const CLIENT_STANDING: i32 = 1 << 20;

/// The `horse_body` texture of a horse armour item (its id without the
/// namespace): the item's `equippable.asset_id`, which names the
/// `equipment/<asset>.json` whose `horse_body` layer is drawn. Only the golden
/// armour's asset (`gold`) is not its item's material word.
pub fn horse_armor_asset(item: &str) -> Option<&str> {
    match item.strip_suffix("_horse_armor")? {
        "golden" => Some("gold"),
        material => Some(material),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::Damage;
    use crate::mobs::finalize::base_value;
    use crate::mobs::sensor_scan_tests::kind;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

    /// Every vanilla horse armour item draws the `horse_body` texture its
    /// `equippable.asset_id` equipment names (`items.json`, `equipment/*.json`).
    #[test]
    fn horse_armor_draws_its_equipment_asset_texture() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/myriade-engine/26.3");
        let Ok(items) = std::fs::read(root.join("items.json")) else {
            return;
        };
        let items: serde_json::Value = serde_json::from_slice(&items).unwrap();
        let mut seen = 0;
        for (id, components) in items.as_object().unwrap() {
            let Some(item) = id
                .strip_prefix("minecraft:")
                .filter(|i| i.ends_with("_horse_armor"))
            else {
                continue;
            };
            let asset = components["minecraft:equippable"]["asset_id"]
                .as_str()
                .unwrap();
            let asset = asset.strip_prefix("minecraft:").unwrap();
            let file = root.join(format!("assets/minecraft/equipment/{asset}.json"));
            let equipment: serde_json::Value =
                serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
            let texture = equipment["layers"]["horse_body"][0]["texture"]
                .as_str()
                .unwrap();
            assert_eq!(
                horse_armor_asset(item),
                texture.strip_prefix("minecraft:"),
                "{id}"
            );
            seen += 1;
        }
        assert!(seen >= 6, "the vanilla horse armours: {seen}");
    }


    /// `AbstractHorse` takes a saddled player rider's input; any other mob
    /// (`Mob.getControllingPassenger`) does not, even with a saddle set, as a
    /// mod mob on a wandering trader template may set it for its own saddle.
    #[test]
    fn only_an_abstract_horse_is_steered_by_its_saddled_player_rider() {
        for (species, expected) in [
            (Species::Horse, true),
            (Species::Donkey, true),
            (Species::Mule, true),
            (Species::SkeletonHorse, true),
            (Species::ZombieHorse, true),
            (Species::Camel, true),
            (Species::CamelHusk, true),
            (Species::Llama, true),
            (Species::TraderLlama, true),
            (Species::WanderingTrader, false),
            (Species::Villager, false),
        ] {
            let mut m = horse(species);
            m.rider = Some(Rider::default());
            assert!(!controlled(&m), "{species:?} unsaddled");
            m.extra.set_saddled(true);
            assert_eq!(controlled(&m), expected, "{species:?} saddled");
        }
    }

    #[test]
    #[ignore = "daily"]
    fn tame_listener_defers_the_successful_tame() {
        use horizon_core::random::{LegacyRandom, RandomSource};
        assert!(crate::mobs::world::with_test_world(&[], |world, _| {
            let seed = (0..1000)
                .find(|seed| LegacyRandom::new(*seed).next_int_bound(50) == Ok(0))
                .unwrap();
            for asks_animal_tame in [false, true] {
                let world = crate::mobs::MobWorld {
                    asks_animal_tame,
                    ..world.clone()
                };
                let mut m = horse(Species::Horse);
                m.extra.equine_temper = 100;
                m.random = horizon_core::random::EntityRandom::own(LegacyRandom::new(seed));
                let mut test_level = crate::health::TestLevel::new(0);
                let level = &mut test_level.level();
                let mut events = Vec::new();
                let mut cx = Cx {
                    w: &world,
                    level,
                    events: &mut events,
                };
                RunAroundLikeCrazy::new().tick(&mut m, &mut cx);
                assert_eq!(m.extra.tame, !asks_animal_tame);
                assert!(matches!(events.as_slice(), [MobEvent::Tamed { mob: 1 }]));
            }
        }));
    }

    /// A failed taming adds 5 to the temper, up to the species' most, and
    /// throws the rider off before the angry rear.
    #[test]
    fn a_refused_rider_raises_the_temper_and_is_thrown() {
        let mut m = horse(Species::Horse);
        m.extra.equine_temper = 20;
        let mut events = Vec::new();
        refuse_rider(&mut m, &mut events);
        assert_eq!(m.extra.equine_temper, 25);
        assert!(matches!(events[0], MobEvent::Buck { mob: 1 }));
        assert_eq!(m.extra.counter, 20, "rears");
        m.extra.equine_temper = 98;
        refuse_rider(&mut m, &mut Vec::new());
        assert_eq!(temper(&m), max_temper(Species::Horse));
    }

    /// The existing goal reads the loaded counter; only its failed attempt
    /// modifies it, with the gameplay clamp. The random continuation is kept.
    #[test]
    #[ignore = "daily"]
    fn temper_preservation_reaches_the_existing_taming_consumer() {
        use horizon_core::random::LegacyRandom;
        let ran = crate::mobs::world::with_test_world(&[], |w, types| {
            let kinds = types
                .types
                .iter()
                .map(|(n, k)| (n.clone(), Arc::new(k.clone())))
                .collect();
            for (name, max) in [("llama", 30), ("trader_llama", 30), ("horse", 100)] {
                for value in [1000, -7] {
                    let raw = horizon_core::nbt::Tag::compound([
                        (
                            "id",
                            horizon_core::nbt::Tag::String(
                                format!("minecraft:{name}").as_str().into(),
                            ),
                        ),
                        ("Temper", horizon_core::nbt::Tag::Int(value)),
                        ("Pos", crate::save::doubles(&[0.5, 200.0, 0.5])),
                    ]);
                    let mut m = crate::entity_nbt::mob_from_nbt(&raw, &kinds, 7, None).unwrap();
                    m.rider = Some(Rider::default());
                    let seed = (0..10_000)
                        .find(|&seed| LegacyRandom::new(seed).next_int_bound(50).unwrap() == 0)
                        .unwrap();
                    m.random.set_seed(seed);
                    let mut expected = m.random.clone();
                    assert_eq!(expected.next_int_bound(50).unwrap(), 0);
                    expected.next_int_bound(max).unwrap();
                    let mut test_level = crate::health::TestLevel::new(1);
                    let mut events = Vec::new();
                    RunAroundLikeCrazy::new().tick(
                        &mut m,
                        &mut Cx {
                            w,
                            level: &mut test_level.level(),
                            events: &mut events,
                        },
                    );
                    assert_eq!(m.extra.tame, value == 1000);
                    if value < 0 {
                        expected.next_float();
                        expected.next_float();
                        assert!(
                            events
                                .iter()
                                .any(|e| matches!(e, MobEvent::Buck { mob: 7 }))
                        );
                    } else {
                        assert!(
                            events
                                .iter()
                                .any(|e| matches!(e, MobEvent::Tamed { mob: 7 }))
                        );
                    }
                    assert_eq!(
                        m.random.next_long(),
                        expected.next_long(),
                        "{name}: RNG continuation"
                    );
                    let saved =
                        crate::entity_nbt::mob_to_nbt(&m, [93, 0, 0, 1], None, None, |_| None);
                    assert_eq!(
                        saved.get("Temper"),
                        Some(&horizon_core::nbt::Tag::Int(if value < 0 {
                            0
                        } else {
                            1000
                        })),
                        "{name}: actual goal uses the same modeled temper"
                    );
                }
            }
        });
        assert!(ran, "the consumer test must actually run");
    }

    #[test]
    fn temper_preservation_modification_keeps_limits_and_java_overflow() {
        for (species, name) in [
            (Species::Llama, "minecraft:llama"),
            (Species::TraderLlama, "minecraft:trader_llama"),
            (Species::Horse, "minecraft:horse"),
        ] {
            let mut m = equine(7, species, name);
            m.extra.variant3 = 217;
            let max = max_temper(species);
            for (value, amount, expected) in [
                (-7, 5, 0),
                (max - 1, 5, max),
                (1000, 5, max),
                (i32::MAX, 5, 0),
                (i32::MIN, -1, max),
            ] {
                m.extra.equine_temper = value;
                assert_eq!(temper(&m), value);
                assert_eq!(modify_temper(&mut m, amount), expected);
                assert_eq!(m.extra.equine_temper, expected);
                assert_eq!(m.extra.variant3, 217, "unrelated byte is untouched");
            }
        }
        // Camels inherit the same AbstractHorse counter: loaded unclamped,
        // clamped only by a modification.
        for (species, name) in [
            (Species::Camel, "minecraft:camel"),
            (Species::CamelHusk, "minecraft:camel_husk"),
        ] {
            let mut m = equine(7, species, name);
            let raw = horizon_core::nbt::Tag::compound([
                ("id", horizon_core::nbt::Tag::String(name.into())),
                ("Pos", crate::save::doubles(&[0.5, 200.0, 0.5])),
                ("Temper", horizon_core::nbt::Tag::Int(1000)),
            ]);
            let kinds = [(name.into(), m.kind.clone())].into();
            let decoded = crate::entity_nbt::mob_from_nbt(&raw, &kinds, 7, None).unwrap();
            assert_eq!(decoded.extra.equine_temper, 1000);
            assert_eq!(temper(&decoded), 1000);
            m.extra.variant3 = 97;
            m.extra.equine_temper = 98;
            assert_eq!(modify_temper(&mut m, 5), 100);
            assert_eq!(m.extra.equine_temper, 100);
            assert_eq!(m.extra.variant3, 97, "unrelated byte is untouched");
        }
    }

    /// The rolled health is the attribute's base, which the server syncs: a
    /// rider's client draws the horse's hearts from it, not the type's 53.
    #[test]
    fn rolled_health_and_speed_are_attribute_bases() {
        for species in [Species::Horse, Species::Donkey, Species::Mule] {
            let mut m = horse(species);
            finalize(&mut m, &mut horizon_core::random::LegacyRandom::new(0));
            assert!((15.0..=30.0).contains(&m.attrs.max_health));
            assert_eq!(
                base_value(&m, Attr::MaxHealth),
                f64::from(m.attrs.max_health)
            );
            assert_eq!(m.health, m.attrs.max_health);
        }
        let mut m = horse(Species::Horse);
        finalize(&mut m, &mut horizon_core::random::LegacyRandom::new(0));
        assert_eq!(base_value(&m, Attr::MovementSpeed), m.attrs.speed);
    }

    /// Hits `n` fresh mobs of a species once each; how many reared.
    fn rears(
        level: &mut crate::health::DamageLevel<'_>,
        species: Species,
        name: &str,
        n: u32,
    ) -> u32 {
        (1..=n)
            .filter(|&id| {
                let mut m = equine(id, species, name);
                assert!(m.damage(level, 1.0, Damage::player([0.0, 64.0, 0.0])));
                m.extra.counter == 20
            })
            .count() as u32
    }

    #[test]
    fn a_hurt_horse_rears_one_hit_in_three() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // AbstractHorse.hurtServer: nextInt(3) == 0 -> standIfPossible (#3432).
        let n = rears(level, Species::Horse, "minecraft:horse", 300);
        assert!((60..=140).contains(&n), "{n} of 300 hits reared");
    }

    #[test]
    fn a_synched_rear_lasts_until_the_flag_goes() {
        // The mount a client drives ticks `stand_tick` itself; the synched
        // standing flag, not that counter, decides when the rear is over.
        let mut m = equine(1, Species::Donkey, "minecraft:donkey");
        set_flags(&mut m, 32);
        for _ in 0..25 {
            stand_tick(&mut m);
            animate(&mut m);
        }
        assert!(m.extra.counter > 0, "still standing");
        assert_eq!(m.anim.v[STAND], 1.0);
        set_flags(&mut m, 4);
        assert_eq!(m.extra.counter, 0);
        stand_tick(&mut m);
        animate(&mut m);
        assert!(m.anim.v[STAND] < 1.0, "falls once the flag is gone");
    }

    #[test]
    fn the_rear_ends_after_twenty_ticks() {
        // AbstractHorse.tick: standCounter runs down, then clearStanding.
        let mut m = equine(1, Species::Horse, "minecraft:horse");
        stand_if_possible(&mut m);
        for _ in 0..19 {
            stand_tick(&mut m);
        }
        assert!(m.extra.counter > 0);
        stand_tick(&mut m);
        assert_eq!(m.extra.counter, 0);
    }

    #[test]
    fn llamas_and_camels_never_rear_when_hurt() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // Llama and Camel: canPerformRearing is false.
        assert_eq!(rears(level, Species::Llama, "minecraft:llama", 60), 0);
        assert_eq!(rears(level, Species::Camel, "minecraft:camel", 60), 0);
    }


    /// `SkeletonHorse.playSwimSound`: 0.3 on the ground, and no more than 0.1
    /// in the water, where every other animal keeps the volume
    /// `waterSwimSound` works out from its speed.
    #[test]
    fn a_skeleton_horses_splash_is_quiet_in_the_water() {
        let mut m = equine(1, Species::SkeletonHorse, "minecraft:skeleton_horse");
        m.on_ground = true;
        assert_eq!(swim_volume(&m, 0.35), 0.3);
        m.on_ground = false;
        assert_eq!(swim_volume(&m, 0.35), 0.1);
        assert_eq!(swim_volume(&m, 1.0), 0.1);
        assert!((swim_volume(&m, 0.001) - 0.025).abs() < 1.0e-6);
        let mut horse = equine(2, Species::Horse, "minecraft:horse");
        horse.on_ground = false;
        assert_eq!(swim_volume(&horse, 0.35), 0.35);
    }

    /// `RunAroundLikeCrazyGoal.canUse`: `isVehicle()` is any rider, so an
    /// untamed horse with a mob on its back bolts; a zombie horse with a mob
    /// rider is `isMobControlled()` and does not.
    #[test]
    #[ignore = "daily"]
    fn a_mob_riding_an_untamed_horse_starts_the_bolt() {
        use crate::mobs::world::with_built_test_world;
        let mut horse = horse(Species::Horse);
        horse.position = Vec3::new(0.5, 1.0, 0.5);
        horse.on_ground = true;
        horse.extra.rider = Some(9);
        let mut zombie = zombie_horse();
        zombie.extra.rider = Some(9);
        let mut outcomes = Vec::new();
        if !with_built_test_world(
            &[],
            |world, registry| {
                for (cx, cz) in [
                    (-1, -1),
                    (-1, 0),
                    (-1, 1),
                    (0, -1),
                    (0, 0),
                    (0, 1),
                    (1, -1),
                    (1, 0),
                    (1, 1),
                ] {
                    world
                        .chunks
                        .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
                }
                let stone = registry.default_state("minecraft:stone").unwrap();
                for x in -16..32 {
                    for z in -16..32 {
                        world.set(x, 0, z, stone).unwrap();
                    }
                }
            },
            |w, _| {
                let mut lr = crate::health::TestLevel::new(0);
                let mut events = Vec::new();
                let mut cx = Cx {
                    w,
                    level: &mut lr.level(),
                    events: &mut events,
                };
                for m in [&mut horse, &mut zombie] {
                    m.ai.goals.add(1, RunAroundLikeCrazy::new());
                    let mut goals = std::mem::take(&mut m.ai.goals);
                    goals.tick(m, &mut cx);
                    outcomes.push(goals.is_running("RunAroundLikeCrazy"));
                    m.ai.goals = goals;
                }
            },
        ) {
            return;
        }
        assert_eq!(
            outcomes,
            vec![true, false],
            "a mob-ridden horse bolts; a mob-ridden zombie horse is controlled"
        );
    }

}

#[cfg(test)]
mod follow_mommy_tests {
    use super::*;
    use crate::mobs::world::{with_built_test_world, with_test_world};
    use std::sync::Arc;


    /// `AbstractHorse.followMommy`: a bred foal that is not eating keeps a path
    /// to the nearest bred `AbstractHorse` within 16 blocks while it is more
    /// than two blocks away.
    #[test]
    #[ignore = "daily"]
    fn a_bred_foal_paths_to_the_nearest_bred_horse() {
        let Some(_) = target_pos((true, true, false), &[]) else {
            return;
        };
        assert_eq!(
            target_pos((true, true, false), &[("minecraft:horse", 6.5, true)]).unwrap(),
            Some([6, 1, 0]),
            "a bred foal walks to the grown horse"
        );
        // The nearest of two, whatever the kind.
        assert_eq!(
            target_pos(
                (true, true, false),
                &[
                    ("minecraft:horse", 12.5, true),
                    ("minecraft:donkey", 6.5, true)
                ]
            )
            .unwrap(),
            Some([6, 1, 0]),
            "the nearer of the herd"
        );
        // `isBred`, `isBaby` and `!isEating` on the foal itself.
        assert_eq!(
            target_pos((false, true, false), &[("minecraft:horse", 6.5, true)]).unwrap(),
            None,
            "a foal that is not bred does not follow"
        );
        assert_eq!(
            target_pos((true, false, false), &[("minecraft:horse", 6.5, true)]).unwrap(),
            None,
            "an adult does not follow"
        );
        assert_eq!(
            target_pos((true, true, true), &[("minecraft:horse", 6.5, true)]).unwrap(),
            None,
            "a foal busy eating does not follow"
        );
        // `MOMMY_TARGETING`: an AbstractHorse, and bred.
        assert_eq!(
            target_pos((true, true, false), &[("minecraft:horse", 6.5, false)]).unwrap(),
            None,
            "the grown horse must be bred too"
        );
        assert_eq!(
            target_pos((true, true, false), &[("minecraft:cow", 6.5, true)]).unwrap(),
            None,
            "another kind is no mommy"
        );
        // 16 blocks of `MOMMY_TARGETING`, and more than two away.
        assert_eq!(
            target_pos((true, true, false), &[("minecraft:horse", 20.5, true)]).unwrap(),
            None,
            "out of the 16-block range"
        );
        assert_eq!(
            target_pos((true, true, false), &[("minecraft:horse", 1.5, true)]).unwrap(),
            None,
            "already at its side"
        );
    }
}


#[cfg(test)]
mod goal_timing_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use horizon_core::random::{EntityRandom, LegacyRandom, RandomSource};

}
