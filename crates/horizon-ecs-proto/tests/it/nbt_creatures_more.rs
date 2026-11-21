//! The keys of `nbt_fields::creatures_more`: a sheep's `Sheared` is the wool's state, a vex's `life_ticks` the ticks left of its `Lifespan`.
//! Each saves the type the jar writes, reads it back to the same NBT, and a wrong type is refused without any change.
use bevy_ecs::prelude::*;
use horizon_core::{
    geometry::Vec3,
    nbt::{Compound, JavaString, Tag},
};
use horizon_ecs_proto::{
    behaviours::{fox_sleep::Sleeping, fox_state::Crouch, limited_life::Lifespan, swim_to_treasure::GotFish},
    mount::Mount,
    raids::{Patrol, Raider},
    cow::{now_of, Herd, Pos, TypeName},
    farm::Wool,
    nbt::{self, Saved},
};

fn key(k: &str) -> JavaString {
    JavaString::from(k)
}

fn patch(k: &str, t: Tag) -> Compound {
    let mut c = Compound::new();
    c.insert(key(k), t);
    c
}

#[test]
fn a_sheep_saves_its_wool_as_a_byte_and_reads_it_back_and_refuses_an_int() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let sheep = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:sheep"), Saved, Wool { sheared: true, color: 0 })).id();
    assert_eq!(nbt::get(&h.world, sheep, "Sheared"), Some(Tag::Byte(1)));

    let c = nbt::save(&h.world, sheep).expect("saved");
    let twin = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:sheep"), Saved, Wool::default())).id();
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert_eq!(h.world.get::<Wool>(twin).map(|w| w.sheared), Some(true));
    assert_eq!(nbt::save(&h.world, twin), Some(c));

    assert!(!nbt::merge(&mut h.world, twin, &patch("Sheared", Tag::Int(0))));
    assert_eq!(h.world.get::<Wool>(twin).map(|w| w.sheared), Some(true));
}

#[test]
fn a_vex_saves_its_ticks_left_as_an_int_and_a_read_restores_its_span_and_refuses_a_short() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let vex = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:vex"), Saved, Lifespan(0))).id();
    let now = now_of(&h.world, vex);
    h.world.entity_mut(vex).insert(Lifespan(now + 40));
    assert_eq!(nbt::get(&h.world, vex, "life_ticks"), Some(Tag::Int(40)));

    let c = nbt::save(&h.world, vex).expect("saved");
    let twin = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:vex"), Saved)).id();
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert_eq!(h.world.get::<Lifespan>(twin).map(|l| l.0), Some(now + 40));
    assert_eq!(nbt::save(&h.world, twin), Some(c));

    assert!(!nbt::merge(&mut h.world, twin, &patch("life_ticks", Tag::Short(5))));
    assert_eq!(h.world.get::<Lifespan>(twin).map(|l| l.0), Some(now + 40));
}

#[test]
fn a_horse_saves_its_temper_as_an_int_and_refuses_a_byte() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let horse = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:horse"), Saved, Mount { temper: 17, max_temper: 100, ..Mount::default() })).id();
    assert_eq!(nbt::get(&h.world, horse, "Temper"), Some(Tag::Int(17)));

    let c = nbt::save(&h.world, horse).expect("saved");
    let twin = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:horse"), Saved, Mount { max_temper: 100, ..Mount::default() })).id();
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert_eq!(h.world.get::<Mount>(twin).map(|m| m.temper), Some(17));
    assert_eq!(nbt::save(&h.world, twin), Some(c));

    assert!(!nbt::merge(&mut h.world, twin, &patch("Temper", Tag::Byte(3))));
    assert_eq!(h.world.get::<Mount>(twin).map(|m| m.temper), Some(17));
}

#[test]
fn a_raider_saves_its_raid_and_wave_and_a_patrol_its_flags_and_target() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let r = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:pillager"), Saved, Raider { raid: Some(4), wave: 2, leader: false }, Patrol { leader: true, patrolling: true, target: Some([5, 6, 7]), cooldown_until: 0 })).id();
    assert_eq!(nbt::get(&h.world, r, "RaidId"), Some(Tag::Int(4)));
    assert_eq!(nbt::get(&h.world, r, "Wave"), Some(Tag::Int(2)));
    assert_eq!(nbt::get(&h.world, r, "PatrolLeader"), Some(Tag::Byte(1)));
    assert_eq!(nbt::get(&h.world, r, "Patrolling"), Some(Tag::Byte(1)));
    assert_eq!(nbt::get(&h.world, r, "patrol_target"), Some(Tag::IntArray(vec![5, 6, 7])));

    let c = nbt::save(&h.world, r).expect("saved");
    let twin = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:pillager"), Saved)).id();
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert_eq!(h.world.get::<Raider>(twin).map(|x| (x.raid, x.wave)), Some((Some(4), 2)));
    assert_eq!(h.world.get::<Patrol>(twin).map(|p| (p.leader, p.patrolling, p.target)), Some((true, true, Some([5, 6, 7]))));
    assert_eq!(nbt::save(&h.world, twin), Some(c));

    assert!(!nbt::merge(&mut h.world, twin, &patch("Wave", Tag::Byte(1))));
    assert_eq!(h.world.get::<Raider>(twin).map(|x| x.wave), Some(2));
}

#[test]
fn a_dolphin_that_ate_its_fish_saves_a_byte_and_a_zero_takes_no_marker() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let d = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:dolphin"), Saved, GotFish)).id();
    assert_eq!(nbt::get(&h.world, d, "GotFish"), Some(Tag::Byte(1)));

    let c = nbt::save(&h.world, d).expect("saved");
    let twin = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:dolphin"), Saved)).id();
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert!(h.world.get::<GotFish>(twin).is_some());
    assert_eq!(nbt::save(&h.world, twin), Some(c));

    assert!(!nbt::merge(&mut h.world, twin, &patch("GotFish", Tag::Short(1))));
}

#[test]
fn a_fox_saves_its_crouch_and_its_sleep_as_bytes_and_refuses_a_short() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let now = 500;
    let f = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:fox"), Saved, Crouch(Some(now)), Sleeping)).id();
    assert_eq!(nbt::get(&h.world, f, "Crouching"), Some(Tag::Byte(1)));
    assert_eq!(nbt::get(&h.world, f, "Sleeping"), Some(Tag::Byte(1)));

    let c = nbt::save(&h.world, f).expect("saved");
    let twin = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:fox"), Saved, Crouch(None))).id();
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert!(h.world.get::<Crouch>(twin).is_some_and(|x| x.0.is_some()));
    assert!(h.world.get::<Sleeping>(twin).is_some());
    assert_eq!(nbt::save(&h.world, twin), Some(c));

    assert!(!nbt::merge(&mut h.world, twin, &patch("Crouching", Tag::Int(1))));
    assert!(!nbt::merge(&mut h.world, twin, &patch("Sleeping", Tag::Int(1))));
}
