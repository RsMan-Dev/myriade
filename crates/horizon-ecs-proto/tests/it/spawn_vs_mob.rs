//! The spawn-time finalization (`spawn_finalize.rs`) and the weighted pick of the spawners (`spawn_spawner::weighted`) against the rates
//! of Horizon's `finalize.rs` and `WeightedRandom`: the baby of a group (5 %, never the first), the baby zombie and its chicken jockey
//! (5 % then 5 %), the left hand (5 %), the spread of the follow bonus. Pure draws: no world, no resources, no HORIZON_TEST_RESOURCES.
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_ecs_proto::spawn_finalize::{ageable_roll, mob_roll, zombie_roll, Group, Jockey, BABY_CHANCE, FOLLOW_DEVIATION};
use horizon_ecs_proto::spawn_spawner::weighted;

const N: usize = 200_000;
const TOL: f64 = 0.005;

fn near(got: f64, want: f64) -> bool {
    (got - want).abs() <= TOL
}

#[test]
fn the_first_of_a_group_is_never_a_baby() {
    let mut r = LegacyRandom::new(7);
    let (baby, g) = ageable_roll(None, &mut r);
    assert!(!baby);
    assert_eq!(g, Group::Ageable { should_baby: true, size: 1, chance: BABY_CHANCE });
}

#[test]
fn a_later_member_of_an_ageable_group_is_a_baby_one_time_in_twenty() {
    let mut r = LegacyRandom::new(11);
    let mut g = Some(Group::AGEABLE);
    let (mut babies, mut seen) = (0usize, 0usize);
    for _ in 0..N {
        let (baby, next) = ageable_roll(g, &mut r);
        babies += usize::from(baby);
        seen += 1;
        g = Some(next);
    }
    let rate = babies as f64 / seen as f64;
    assert!(near(rate, 0.05), "baby rate {rate}");
}

#[test]
fn a_new_zombie_group_is_a_baby_one_time_in_twenty_then_jockey_by_chicken_draws() {
    let mut r = LegacyRandom::new(23);
    let (mut babies, mut free, mut create) = (0usize, 0usize, 0usize);
    for _ in 0..N {
        let (g, j) = zombie_roll(None, &mut r);
        if let Group::Zombie { baby: true, .. } = g {
            babies += 1;
            match j {
                Jockey::Free => free += 1,
                Jockey::Create => create += 1,
                Jockey::None => {}
            }
        }
    }
    let baby = babies as f64 / N as f64;
    assert!(near(baby, 0.05), "zombie baby rate {baby}");
    // The free chicken draw is 5 %; the new chicken only when that one fails (5 % of the rest).
    let free_rate = free as f64 / babies as f64;
    let create_rate = create as f64 / babies as f64;
    assert!(near(free_rate, 0.05), "free chicken rate {free_rate}");
    assert!(near(create_rate, 0.95 * 0.05), "new chicken rate {create_rate}");
}

#[test]
fn a_zombie_that_is_not_a_baby_draws_no_chicken() {
    let mut r = LegacyRandom::new(31);
    for _ in 0..1_000 {
        let (_, j) = zombie_roll(Some(Group::Zombie { baby: false, jockey: true }), &mut r);
        assert_eq!(j, Jockey::None);
    }
}

#[test]
fn the_left_hand_is_one_time_in_twenty_and_the_follow_bonus_is_centred() {
    let mut r = LegacyRandom::new(41);
    let (mut left, mut sum, mut max) = (0usize, 0.0f64, 0.0f64);
    for _ in 0..N {
        let m = mob_roll(&mut r);
        left += usize::from(m.left_handed);
        sum += m.follow_bonus;
        max = max.max(m.follow_bonus.abs());
    }
    assert!(near(left as f64 / N as f64, 0.05));
    assert!((sum / N as f64).abs() < 0.001, "mean follow bonus {}", sum / N as f64);
    // A triangle of deviation d reaches at most 3 d in the source's draw.
    assert!(max <= 3.0 * FOLLOW_DEVIATION + 1e-9, "follow bonus {max}");
}

#[test]
fn the_weights_of_a_spawner_pick_in_proportion() {
    let mut r = LegacyRandom::new(53);
    let pot = [(1u8, 3), (2u8, 1)];
    let (mut ones, mut twos) = (0usize, 0usize);
    for _ in 0..N {
        match weighted(&pot, &mut r) {
            Some(1) => ones += 1,
            Some(2) => twos += 1,
            other => panic!("unexpected pick {other:?}"),
        }
    }
    let share = ones as f64 / N as f64;
    assert!((share - 0.75).abs() <= TOL, "share of 1: {share}");
    assert_eq!(ones + twos, N);
}
