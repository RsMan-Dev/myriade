//! The link between a creaking and its heart: the jar's `home_pos` (an int array) holds the heart's position, a creaking that is loaded gets
//! it back whatever order the keys come in, and a creature that is not a creaking still reads `home_pos` as its `Home`.
use bevy_ecs::prelude::*;
use horizon_core::{
    geometry::Vec3,
    nbt::{Compound, JavaString, Tag},
};
use horizon_ecs_proto::{
    behaviours::stare_freeze::{Params, StareFreeze},
    bridge::host_creaking::HeartLink,
    cow::{Herd, Pos, TypeName},
    nbt::{self, Saved},
    spots::Home,
};

fn patch(k: &str, t: Tag) -> Compound {
    let mut c = Compound::new();
    c.insert(JavaString::from(k), t);
    c
}

fn link(pos: [i32; 3]) -> HeartLink {
    HeartLink { pos, stuck: 0, tearing_down: false }
}

#[test]
fn a_creaking_saves_the_heart_position_as_an_int_array() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let c = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:creaking"), Saved, StareFreeze(&Params::DEFAULT), link([5, 64, -7]))).id();
    assert_eq!(nbt::get(&h.world, c, "home_pos"), Some(Tag::IntArray(vec![5, 64, -7])));
}

#[test]
fn a_loaded_creaking_gets_its_link_back_and_saves_the_same_nbt() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let c = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:creaking"), Saved, StareFreeze(&Params::DEFAULT), link([5, 64, -7]))).id();
    let saved = nbt::save(&h.world, c).expect("saved");

    let twin = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:creaking"), Saved, StareFreeze(&Params::DEFAULT))).id();
    assert_eq!(nbt::load(&mut h.world, twin, &saved), Some(0));
    assert_eq!(h.world.get::<HeartLink>(twin).map(|l| l.pos), Some([5, 64, -7]));
    assert!(h.world.get::<Home>(twin).is_none(), "a heart is not a home with a radius");
    assert_eq!(nbt::save(&h.world, twin), Some(saved));
}

#[test]
fn a_link_read_keeps_the_state_the_creaking_holds_and_refuses_a_wrong_type() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let c = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:creaking"), Saved, StareFreeze(&Params::DEFAULT), HeartLink { pos: [1, 2, 3], stuck: 3, tearing_down: true })).id();
    assert!(nbt::merge(&mut h.world, c, &patch("home_pos", Tag::IntArray(vec![9, 9, 9]))));
    assert_eq!(h.world.get::<HeartLink>(c).copied(), Some(HeartLink { pos: [9, 9, 9], stuck: 3, tearing_down: true }));

    assert!(!nbt::merge(&mut h.world, c, &patch("home_pos", Tag::Int(1))));
    assert!(!nbt::merge(&mut h.world, c, &patch("home_pos", Tag::IntArray(vec![1, 2]))));
    assert_eq!(h.world.get::<HeartLink>(c).map(|l| l.pos), Some([9, 9, 9]));
}

#[test]
fn a_creature_without_the_stare_still_reads_home_pos_as_its_home() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let twin = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:villager"), Saved)).id();
    assert_eq!(nbt::load(&mut h.world, twin, &patch("home_pos", Tag::IntArray(vec![4, 5, 6]))), Some(0));
    assert_eq!(h.world.get::<Home>(twin).map(|x| x.center), Some([4, 5, 6]));
    assert!(h.world.get::<HeartLink>(twin).is_none());
}
