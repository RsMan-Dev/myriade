use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_ecs_proto::rules::variant::{Place, draw_from};
use horizon_ecs_proto::rules::spawn_finalize::ageable_roll;
use horizon_ecs_proto::behaviours::spawn_draw::{run, Loner, When};
use horizon_ecs_proto::rules::spawn_draw::Ctx;
use horizon_ecs_proto::species::{parrot, rabbit, tropical_fish};
use bevy_ecs::world::World;

/// The place of a plain biome: no tag, no structure, no moon.
fn plain(_: &str) -> bool {
    false
}

/// `Rabbit.getRandomRabbitVariant`: brown 0, salt 5, black 2 by `nextInt(100)`, the first draw of the level's random.
#[test]
fn a_lone_rabbit_takes_the_level_randoms_first_draw() {
    let place = Place { biome: "", biome_has: &plain, in_structure: &plain, moon: 0.0 };
    let table = rabbit::DEF.variant.unwrap();
    for seed in 0..64u32 {
        let mut r = LegacyRandom::new(i64::from(seed) * 31 + 7);
        let roll = r.clone().next_int_bound(100).unwrap();
        let plain = if roll < 50 {
            0
        } else if roll < 90 {
            5
        } else {
            2
        };
        assert_eq!(i32::from(draw_from(table, None, &place, &mut r)), plain, "seed {seed}, roll {roll}");
    }
}

/// `Util.getRandom(Parrot.Variant.values())`: `nextInt(5)` on the level's random, the table being `uniform(5)` (`species/variants.rs`).
#[test]
fn a_lone_parrot_takes_the_level_randoms_first_draw() {
    let place = Place { biome: "", biome_has: &plain, in_structure: &plain, moon: 0.0 };
    let table = parrot::DEF.variant.unwrap();
    for seed in 0..64u32 {
        let mut r = LegacyRandom::new(i64::from(seed) * 31 + 7);
        let expected = r.clone().next_int_bound(5).unwrap();
        assert_eq!(i32::from(draw_from(table, None, &place, &mut r)), expected, "seed {seed}");
    }
}

/// `RabbitGroupData`: a pack's first member is no baby and draws nothing; each later one is a baby for certain (chance 1), the draw made anyway.
#[test]
fn a_rabbit_pack_has_certain_babies_after_its_first_member() {
    let mut group = rabbit::DEF.draws.unwrap().group;
    let mut r = LegacyRandom::new(7);
    let mut babies = Vec::new();
    for _ in 0..4 {
        let (baby, g) = ageable_roll(group, &mut r);
        babies.push(baby);
        group = Some(g);
    }
    assert_eq!(babies, [false, true, true, true]);
    let mut reference = LegacyRandom::new(7);
    for _ in 0..3 {
        reference.next_float();
    }
    assert_eq!(r.next_int_bound(1000), reference.next_int_bound(1000));
}

/// `TropicalFish.finalizeSpawn`: a draw of `nextFloat() < 0.9` joins the school, the others are loners (`isSchool` false, the school ends).
#[test]
fn a_tropical_fish_draw_past_nine_tenths_is_a_loner() {
    let mut w = World::new();
    let d = tropical_fish::DEF.draws.unwrap();
    let c = Ctx { biome: "", biome_has: &plain, special: 0.0, baby: false };
    for seed in 0..64i64 {
        let e = w.spawn_empty().id();
        let school = LegacyRandom::new(seed).next_float() < 0.9;
        run(&mut w, e, d, When::After, &mut None, &c, &mut LegacyRandom::new(seed));
        assert_eq!(w.get::<Loner>(e).is_none(), school, "seed {seed}");
    }
}
