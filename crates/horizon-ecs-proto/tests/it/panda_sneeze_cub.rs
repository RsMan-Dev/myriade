//! A panda cub's sneeze (`PandaSneezeGoal`, `Panda.tick`, `Panda.afterSneeze`): the pre-sneeze sound, 20 ticks later the sneeze sound, the adult pandas
//! within 10 blocks jump, and the gift is the table `gameplay/panda_sneeze` (a slime ball, one chance in 700).
use crate::common::*;
use horizon_ecs_proto::{
    behaviours::panda_state::Gene,
    cow::*,
    host_out::HostEvent,
    loot::roll_table,
};

fn sounded(h: &mut Herd, name: &str) -> bool {
    h.drain_host_out().into_iter().any(|ev| matches!(ev, HostEvent::Sound { name: n, .. } if n == name))
}

#[test]
fn a_weak_cub_sneezes_with_both_sounds_20_ticks_apart_and_the_adult_beside_it_jumps() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:panda").expect("the panda");
    let cub = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
    let adult = spawn_species(&mut h.world, &sp, 4.5, 0.5, 62);
    // A weak cub sneezes about 12 times as often as a normal one (1 in 500 asks against 1 in 6000).
    h.world.entity_mut(cub).insert((Gene::Weak, GrowAt(u64::MAX)));
    horizon_ecs_proto::goals::arm_all(&mut h.world);
    h.drain_host_out();
    let mut pre = None;
    for t in 1..=8000_u64 {
        h.tick();
        if sounded(&mut h, "minecraft:entity.panda.pre_sneeze") {
            pre = Some(t);
            break;
        }
    }
    let pre = pre.expect("the weak cub never sneezed in 8000 ticks");
    // Counter 1 plays the first sound, the counter past 20 the sneeze itself: 20 ticks later.
    let mut jumped = false;
    let mut end = None;
    for t in 1..=40_u64 {
        h.tick();
        let up = h.world.get::<Vel>(adult).is_some_and(|v| v.0.y > 0.0);
        if sounded(&mut h, "minecraft:entity.panda.sneeze") {
            end = Some(t);
            jumped = up;
            break;
        }
    }
    let end = end.unwrap_or_else(|| panic!("no sneeze sound after the pre-sneeze at tick {pre}"));
    assert_eq!(end, 20, "the sneeze sounds 20 ticks after the pre-sneeze");
    assert!(jumped, "the adult within 10 blocks jumps when the cub sneezes");
}

#[test]
fn the_sneeze_gift_is_a_slime_ball_one_time_in_700() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let balls: usize = (0..70_000_i64)
        .map(|seed| roll_table(&mut h.world, "minecraft:gameplay/panda_sneeze", seed.wrapping_mul(0x9E37_79B9).wrapping_add(17)))
        .map(|stacks| stacks.iter().filter(|s| &*s.item == "minecraft:slime_ball").count())
        .sum();
    assert!((55..=160).contains(&balls), "about 100 slime balls expected in 70000 sneezes, got {balls}");
}
