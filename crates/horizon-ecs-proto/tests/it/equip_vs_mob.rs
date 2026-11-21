//! The equipment of a mob, by the rules the game applies (`finalize.rs` for the spawn, `doHurtEquipment` and `dropCustomDeathLoot` for the wear and
//! the death). Pure: the expected values come from the rules, not from the code under test. The side-by-side run against a `Mob` is not here yet.
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_ecs_proto::{
    equipment_drop::roll_piece,
    equipment_spawn::{populate, Spawn, ZOMBIE, SKELETON},
    rules::equipment_wear::{hurt, wear_amount},
    goals::Slot,
};

/// Consecutive seeds give a first draw that walks the unit interval in a nearly linear way (the generator's multiplier is small
/// next to its modulus): a rate measured over 0..n depends on where the walk starts. The seeds are spread by a hash.
fn scatter(i: i64) -> i64 {
    let mut z = (i as u64).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (z ^ (z >> 31)) as i64
}

fn rate(hits: usize, n: usize) -> f64 {
    hits as f64 / n as f64
}

#[test]
fn no_difficulty_no_armour() {
    for seed in 0..300 {
        let out = populate(&ZOMBIE, 0.0, false, |_| false, &mut LegacyRandom::new(seed));
        assert!(out.iter().all(|(slot, _)| matches!(slot, Slot::Main)), "seed {seed}: {out:?}");
    }
}

#[test]
fn armour_set_starts_with_the_helmet_and_its_rate_follows_difficulty() {
    let n = 20_000;
    let mut sets = 0;
    for seed in 0..n {
        let out = populate(&Spawn { armor: true, hand: None }, 1.0, false, |_| false, &mut LegacyRandom::new(scatter(seed)));
        if let Some((slot, name)) = out.first() {
            assert!(matches!(slot, Slot::Head), "seed {seed}: the first piece is the helmet");
            assert!(name.starts_with("minecraft:") && name.ends_with("_helmet"), "{name}");
            sets += 1;
        }
    }
    // 0.15 x special (1.0): a set about one time in 6.7.
    let r = rate(sets, n as usize);
    assert!((0.13..0.17).contains(&r), "rate {r}");
}

#[test]
fn a_worn_slot_keeps_its_piece() {
    for seed in 0..2_000 {
        let out = populate(&Spawn { armor: true, hand: None }, 1.0, false, |s| matches!(s, Slot::Head), &mut LegacyRandom::new(seed));
        assert!(out.iter().all(|(s, _)| !matches!(s, Slot::Head)), "seed {seed}: {out:?}");
    }
}

#[test]
fn a_zombie_holds_one_of_its_weapons_about_one_time_in_a_hundred_easy_and_twenty_hard() {
    let n = 20_000;
    let spawn = Spawn { armor: false, hand: ZOMBIE.hand };
    for (hard, expected) in [(false, 0.01), (true, 0.05)] {
        let mut hits = 0;
        for seed in 0..n {
            let out = populate(&spawn, 0.0, hard, |_| false, &mut LegacyRandom::new(seed as i64));
            if let Some((_, name)) = out.first() {
                assert!(["minecraft:iron_sword", "minecraft:iron_spear", "minecraft:iron_shovel"].contains(&name.as_str()));
                hits += 1;
            }
        }
        let r = rate(hits, n as usize);
        assert!((r - expected).abs() < expected * 0.35, "hard {hard}: rate {r}, expected {expected}");
    }
}

#[test]
fn a_single_weapon_draws_no_random_number() {
    let spawn = Spawn { armor: false, hand: SKELETON.hand };
    let mut r = LegacyRandom::new(42);
    let out = populate(&spawn, 0.0, false, |_| false, &mut r);
    assert_eq!(out, vec![(Slot::Main, "minecraft:bow".to_string())]);
    assert_eq!(r.next_float(), LegacyRandom::new(42).next_float());
}

#[test]
fn wear_is_a_quarter_of_the_hurt_and_at_least_one() {
    assert_eq!(wear_amount(0.0), 0);
    assert_eq!(wear_amount(-2.0), 0);
    assert_eq!(wear_amount(f32::NAN), 0);
    assert_eq!(wear_amount(0.5), 1);
    assert_eq!(wear_amount(3.0), 1);
    assert_eq!(wear_amount(8.0), 2);
    assert_eq!(wear_amount(21.0), 5);
}

#[test]
fn a_piece_breaks_when_its_damage_reaches_its_durability() {
    let mut damage = 0;
    assert!(!hurt(&mut damage, 4, 10));
    assert!(!hurt(&mut damage, 4, 10));
    assert!(hurt(&mut damage, 4, 10));
}

#[test]
fn a_preserved_piece_falls_with_its_own_damage() {
    let mut r = LegacyRandom::new(1);
    for _ in 0..200 {
        assert_eq!(roll_piece(2.0, 2.0, false, false, 100, 37, &mut r), Some(37));
    }
}

#[test]
fn a_piece_that_is_not_preserved_falls_only_to_a_player() {
    let mut r = LegacyRandom::new(2);
    for _ in 0..200 {
        assert_eq!(roll_piece(1.0, 1.0, false, false, 100, 0, &mut r), None);
        assert!(roll_piece(1.0, 1.0, true, false, 100, 0, &mut r).is_some());
    }
}

#[test]
fn a_zero_chance_or_a_prevented_piece_never_falls() {
    let mut r = LegacyRandom::new(3);
    for _ in 0..200 {
        assert_eq!(roll_piece(0.0, 0.0, true, false, 100, 0, &mut r), None);
        assert_eq!(roll_piece(1.0, 1.0, true, true, 100, 0, &mut r), None);
    }
}

#[test]
fn a_fallen_piece_comes_with_a_random_damage_within_its_durability() {
    let mut r = LegacyRandom::new(4);
    for _ in 0..500 {
        let d = roll_piece(1.0, 1.0, true, false, 250, 0, &mut r).unwrap();
        assert!((0..=250).contains(&d), "{d}");
    }
}
