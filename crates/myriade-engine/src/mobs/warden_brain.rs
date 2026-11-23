use crate::health::DamageLevel;
use horizon_core::{
    geometry::Vec3,
    random::{EntityRandom, LegacyRandom, RandomSource},
};

/// `WardenAi` timings.
pub const EMERGE_DURATION: i32 = 134;
pub const ROAR_DURATION: i32 = 84;
const SNIFFING_DURATION: i32 = 84;
const DIGGING_DURATION: i32 = 100;
pub const DIGGING_COOLDOWN: i64 = 1200;
const MELEE_ATTACK_COOLDOWN: i64 = 18;
const SONIC_DURATION: i32 = 60;
const SONIC_SOUND_DELAY: i64 = 34;
const ROAR_SOUND_DELAY: i64 = 25;
const DISTURBANCE_EXPIRY: i64 = 100;
/// `Warden.createAttributes`: the follow range the fight's look target uses.
const FOLLOW_RANGE: f64 = 24.0;

/// `Activity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Core,
    Emerge,
    Dig,
    Idle,
    Roar,
    Fight,
    Investigate,
    Sniff,
}
impl Act {
    pub fn name(self) -> &'static str {
        match self {
            Act::Core => "core",
            Act::Emerge => "emerge",
            Act::Dig => "dig",
            Act::Idle => "idle",
            Act::Roar => "roar",
            Act::Fight => "fight",
            Act::Investigate => "investigate",
            Act::Sniff => "sniff",
        }
    }
    /// The bucket of `HashMap<Activity, ...>` (16 buckets, the hash of the
    /// name); roar and investigate share one but never a priority.
    fn slot(self) -> u8 {
        match self {
            Act::Core => 1,
            Act::Dig => 3,
            Act::Idle => 5,
            Act::Sniff => 6,
            Act::Emerge => 10,
            Act::Roar | Act::Investigate => 11,
            Act::Fight => 14,
        }
    }
    /// Its place in `getActivities` (insertion order inside a bucket).
    fn order(self) -> u8 {
        self as u8
    }
}










// ---- Sensors ------------------------------------------------------------


// ---- Shared warden actions ------------------------------------------------

fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
fn dist_sqr(a: [i32; 3], b: [i32; 3]) -> i64 {
    (0..3).map(|i| i64::from(a[i] - b[i]).pow(2)).sum()
}

// ---- Behaviours -----------------------------------------------------------


/// `OneShot.tryStart`: running when the trigger fires.
fn one(running: &mut bool, f: impl FnOnce() -> bool) -> bool {
    if f() {
        *running = true;
        true
    } else {
        false
    }
}

// ---- Printing ---------------------------------------------------------------

fn pos(p: [i32; 3]) -> String {
    format!("{},{},{}", p[0], p[1], p[2])
}

#[cfg(test)]
mod warden_owned_memory_tests {
    use super::*;
    use crate::entity_nbt::{mob_from_nbt, mob_to_nbt};
    use crate::mobs::MobType;
    use crate::save::{doubles, put};
    use horizon_core::nbt::{Compound, Tag};
    use horizon_world::registry::BlockRegistry;
    use std::{collections::HashMap, sync::Arc};
    fn string(s: &str) -> Tag {
        Tag::String(s.into())
    }
    use crate::mobs::{Species, world::with_built_test_world};

    fn types() -> HashMap<String, Arc<MobType>> {
        let mut kind = crate::mobs::sensor_scan_tests::kind();
        let ty = Arc::make_mut(&mut kind);
        ty.id = "minecraft:warden".into();
        ty.species = Species::Warden;
        HashMap::from([("minecraft:warden".into(), kind)])
    }
    fn entry(ttl: Option<i64>) -> Tag {
        let mut entry = Tag::compound([("value", Tag::Compound(Compound::new()))]);
        if let Some(ttl) = ttl {
            put(&mut entry, "ttl", Tag::Long(ttl));
        }
        entry
    }
    fn input(no_ai: bool, form: u8) -> Tag {
        let mut raw = Tag::compound([
            ("id", string("minecraft:warden")),
            ("UUID", Tag::IntArray(vec![29, 0, 0, 1])),
            ("Pos", doubles(&[2.5, 3.0, 2.5])),
            ("Motion", doubles(&[0.0; 3])),
            ("Health", Tag::Float(10f32.to_bits())),
            ("NoAI", Tag::Byte(i8::from(no_ai))),
            ("NoGravity", Tag::Byte(1)),
            ("Tags", Tag::List(vec![string("keep")])),
            ("audit:keep", Tag::Int(42)),
        ]);
        if form != 0 {
            let memories = if form == 1 {
                Tag::Compound(Compound::new())
            } else {
                Tag::compound([
                    (
                        "minecraft:is_emerging",
                        entry(matches!(form, 2 | 4).then_some(2)),
                    ),
                    (
                        "minecraft:dig_cooldown",
                        entry(matches!(form, 2 | 4).then_some(if form == 4 { 17 } else { 1200 })),
                    ),
                    ("audit:unrelated", Tag::Int(7)),
                ])
            };
            put(
                &mut raw,
                "Brain",
                Tag::compound([("memories", memories), ("audit:brain", Tag::Int(19))]),
            );
        }
        raw
    }
    fn assert_saved(tag: &Tag, expected: [Option<i64>; 2]) {
        for (key, ttl) in ["minecraft:is_emerging", "minecraft:dig_cooldown"]
            .into_iter()
            .zip(expected)
        {
            let actual = tag
                .get("Brain")
                .and_then(|b| b.get("memories"))
                .and_then(|b| b.get(key));
            match ttl {
                None => assert!(actual.is_none(), "stale {key}: {actual:?}"),
                Some(ttl) => assert_eq!(actual, Some(&entry((ttl != i64::MAX).then_some(ttl)))),
            }
        }
    }
    #[test]
    fn decoded_owned_slots_and_no_raw_saves_are_actual_not_input_echoes() {
        let kinds = types();
        for no_ai in [false, true] {
            for (form, expected) in [
                (2, [Some(2), Some(1200)]),
                (4, [Some(2), Some(17)]),
                (0, [None, None]),
                (1, [None, None]),
                (3, [Some(i64::MAX), Some(i64::MAX)]),
            ] {
                let raw = input(no_ai, form);
                let m = mob_from_nbt(&raw, &kinds, 91, None).unwrap();
                assert_eq!(m.no_ai, no_ai);
                assert_eq!(slot_state(&m), expected, "form={form}");
                let mut stale = raw.clone();
                put(
                    &mut stale,
                    "Brain",
                    Tag::compound([
                        (
                            "memories",
                            Tag::compound([
                                ("minecraft:is_emerging", entry(Some(999))),
                                ("minecraft:dig_cooldown", entry(Some(999))),
                                ("audit:unrelated", Tag::Int(7)),
                            ]),
                        ),
                        ("audit:brain", Tag::Int(19)),
                    ]),
                );
                for source in [None, Some(&raw), Some(&stale)] {
                    let saved = mob_to_nbt(&m, [29, 0, 0, 1], source, None, |_| None);
                    assert_saved(&saved, expected);
                    if source.is_some() {
                        assert_eq!(saved.get("audit:keep"), Some(&Tag::Int(42)));
                        if form != 0 || source == Some(&stale) {
                            assert_eq!(
                                saved.get("Brain").and_then(|b| b.get("audit:brain")),
                                Some(&Tag::Int(19))
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn owned_memory_mutations_remove_stale_raw_without_replacing_the_brain() {
        let raw = input(true, 2);
        let mut m = mob_from_nbt(&raw, &types(), 92, None).unwrap();
        let listener = m.extra.warden.as_ref().unwrap().vibration.to_nbt();
        let mem = &mut m.extra.warden.as_mut().unwrap().brain.mem;
        mem.is_emerging.clear();
        mem.dig_cooldown.set_ttl((), 7);
        mem.sniff_cooldown.set_ttl((), 17);
        let saved = mob_to_nbt(&m, [29, 0, 0, 1], Some(&raw), None, |_| None);
        assert_saved(&saved, [None, Some(7)]);
        assert_eq!(
            saved.get("Brain").and_then(|b| b.get("audit:brain")),
            Some(&Tag::Int(19))
        );
        assert_eq!(
            saved
                .get("Brain")
                .and_then(|b| b.get("memories"))
                .and_then(|b| b.get("audit:unrelated")),
            Some(&Tag::Int(7))
        );
        assert_eq!(
            m.extra
                .warden
                .as_ref()
                .unwrap()
                .brain
                .mem
                .sniff_cooldown
                .ttl(),
            17
        );
        assert_eq!(
            m.extra.warden.as_ref().unwrap().vibration.to_nbt(),
            listener
        );
        let mem = &mut m.extra.warden.as_mut().unwrap().brain.mem;
        mem.is_emerging.set(());
        mem.dig_cooldown.clear();
        assert_saved(
            &mob_to_nbt(&m, [29, 0, 0, 1], Some(&raw), None, |_| None),
            [Some(i64::MAX), None],
        );
    }
    #[test]
    #[ignore = "daily"]
    fn actual_mob_ticks_expire_owned_memories_and_noai_retains_them() {
        let build = |world: &mut crate::world::World, registry: &BlockRegistry| {
            world
                .chunks
                .insert((0, 0), Arc::new(crate::chunk::Chunk::empty(0, 0)));
            let stone = registry.default_state("minecraft:stone").unwrap();
            for x in 0..=4 {
                for z in 0..=4 {
                    world.set(x, 0, z, stone).unwrap();
                }
            }
        };
        let ran = with_built_test_world(&[], build, |w, kinds| {
            let types = HashMap::from([(
                "minecraft:warden".into(),
                Arc::new(kinds.types["minecraft:warden"].clone()),
            )]);
            for no_ai in [false, true] {
                let mut raw = input(no_ai, 4);
                put(&mut raw, "Health", Tag::Float(500f32.to_bits()));
                let mut m = mob_from_nbt(&raw, &types, 93, None).unwrap();
                let loaded = slot_state(&m);
                let mut fixture = crate::health::TestLevel::new(0);
                let mut events = Vec::new();
                for tick in 1..=5 {
                    assert!(m.tick(w, &mut fixture.level(), &mut events));
                    assert_eq!(m.age, tick as u32);
                    let expected = if no_ai {
                        [Some(2), Some(17)]
                    } else {
                        [(tick <= 2).then_some(2 - tick), Some(17 - tick)]
                    };
                    assert_eq!(slot_state(&m), expected, "actual tick={tick},NoAI={no_ai}");
                    assert_saved(
                        &mob_to_nbt(&m, [29, 0, 0, 1], None, None, |_| None),
                        expected,
                    );
                    assert_eq!(m.health, 500.0);
                }
                assert_eq!(loaded, [Some(2), Some(17)]);
            }
        });
        assert!(ran, "prepared actual MobWorld prerequisite must run");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

}
