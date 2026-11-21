//! `AbstractNautilus.mobInteract` through the rules of the species (`HORIZON_SPECIES=minecraft:nautilus`, `HORIZON_TEST_RESOURCES` set):
//! a wild adult given a taming item is tamed one time in three (and never put in love), a tame one is boarded with an empty hand, a wild
//! one is not, and a tame adult takes the armor of its body.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::breed::Love;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::items;
use horizon_ecs_proto::mount::Mount;
use horizon_ecs_proto::tame::{self, Owner};
use horizon_ecs_proto::targeting::Who;

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:nautilus", "minecraft:zombie_nautilus"])
}
fn player(h: &mut Herd, main: u32) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main, off: 0, ..Default::default() });
}

#[test]
fn a_taming_item_tames_a_wild_adult_one_time_in_three_without_love() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let item = items::item_id("minecraft:pufferfish");
    let mut tamed = 0;
    for k in 0..300 {
        let e = spawn_species(&mut h.world, species(), 0.5 + (k % 50) as f64, 0.5 + (k / 50) as f64, 11 + k as u64);
        assert!(interact(&mut h.world, e, &Interaction { player: 1, item }));
        assert!(h.world.get::<Love>(e).is_none(), "the taming item of a wild adult is not love");
        if h.world.get::<Owner>(e).unwrap().0 == Some(Who::Player(1)) {
            tamed += 1;
        }
    }
    assert!((75..=125).contains(&tamed), "a third of 300: {tamed}");
}

#[test]
fn only_a_tame_nautilus_is_boarded_and_wears_armor() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let wild = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    let pet = spawn_species(&mut h.world, species(), 4.5, 0.5, 6);
    h.world.resource_mut::<Entered>().0.extend([wild, pet]);
    player(&mut h, 0);
    assert!(!interact(&mut h.world, wild, &Interaction { player: 1, item: 0 }), "a wild one is not boarded");
    assert_eq!(h.world.get::<Mount>(wild).unwrap().rider, None);
    tame::set_owner(&mut h.world, pet, Who::Player(1));
    let armor = items::item_id("minecraft:iron_nautilus_armor");
    player(&mut h, armor);
    assert!(interact(&mut h.world, pet, &Interaction { player: 1, item: armor }));
    assert_eq!(equipped(&h.world, pet, Slot::Body), armor);
    player(&mut h, 0);
    assert!(interact(&mut h.world, pet, &Interaction { player: 1, item: 0 }));
    assert_eq!(h.world.get::<Mount>(pet).unwrap().rider, Some(Who::Player(1)));
}

#[test]
fn food_heals_a_hurt_tame_nautilus_by_twice_its_nutrition_and_not_a_full_one() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let pet = spawn_species(&mut h.world, species(), 4.5, 0.5, 6);
    h.world.resource_mut::<Entered>().0.push(pet);
    tame::set_owner(&mut h.world, pet, Who::Player(1));
    let cod = items::item_id("minecraft:cod");
    player(&mut h, cod);
    let max = h.world.get::<MaxHealth>(pet).unwrap().0;
    h.world.get_mut::<Health>(pet).unwrap().0 = max;
    assert!(!interact(&mut h.world, pet, &Interaction { player: 1, item: cod }), "full health: refused");
    h.world.get_mut::<Health>(pet).unwrap().0 = 1.0;
    assert!(interact(&mut h.world, pet, &Interaction { player: 1, item: cod }));
    assert_eq!(h.world.get::<Health>(pet).unwrap().0, 1.0 + 2.0 * 2.0, "cod: nutrition 2, twice");
}
