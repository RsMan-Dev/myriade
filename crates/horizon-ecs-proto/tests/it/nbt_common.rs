//! The common NBT keys: each takes the vanilla type, its component reads it back (a load into an entity with the component sets it), a
//! wrong type is refused, and a key whose component is absent stays a free key.
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use horizon_ecs_proto::{
    behaviours::{breed::Love, elder_curse::Silent, neutral_anger::{Anger, Params}},
    cow::{now_of, spawn_species, species_table, Burning, Frozen, HurtAt, Herd, LastHurtBy, Yaw},
    despawn::Persistent,
    effects::{self, Effects, Kind, AMBIENT, HIDDEN},
    nbt::{self, NbtData},
    nbt_fields::common,
    spots::Home,
    targeting::Who,
};

fn comp(entries: impl IntoIterator<Item = (&'static str, Tag)>) -> Compound {
    match Tag::compound(entries) {
        Tag::Compound(c) => c,
        _ => unreachable!("Tag::compound makes a compound"),
    }
}

fn a_cow(w: &mut World) -> Entity {
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    spawn_species(w, &sp, 3.0, 4.0, 1)
}

#[test]
fn fire_is_the_ticks_left_as_a_short_and_a_load_sets_it() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = a_cow(&mut a.world);
    let now = now_of(&a.world, cow);
    a.world.entity_mut(cow).insert(Burning { until: now + 40 });
    assert_eq!(nbt::get(&a.world, cow, "Fire"), Some(Tag::Short(40)));

    let other = a_cow(&mut a.world);
    a.world.entity_mut(other).insert(Burning { until: 0 });
    assert_eq!(nbt::load(&mut a.world, other, &comp([("Fire", Tag::Short(7))])), Some(0));
    let until = a.world.get::<Burning>(other).expect("still on fire").until;
    assert_eq!(until, now_of(&a.world, other) + 7);
    assert_eq!(nbt::load(&mut a.world, other, &comp([("Fire", Tag::Short(0))])), Some(0));
    assert!(a.world.get::<Burning>(other).is_none(), "zero ticks is not on fire");
    assert_eq!(nbt::load(&mut a.world, other, &comp([("Fire", Tag::Int(3))])), Some(1), "a wrong type is refused");
}

#[test]
fn ticks_frozen_is_an_int_and_a_key_without_its_component_stays_free() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = a_cow(&mut a.world);
    a.world.entity_mut(cow).insert(Frozen(5));
    assert_eq!(nbt::get(&a.world, cow, "TicksFrozen"), Some(Tag::Int(5)));

    let other = a_cow(&mut a.world);
    a.world.entity_mut(other).remove::<Frozen>();
    assert_eq!(nbt::load(&mut a.world, other, &comp([("TicksFrozen", Tag::Int(9))])), Some(0));
    assert_eq!(nbt::get(&a.world, other, "TicksFrozen"), Some(Tag::Int(9)));
    assert_eq!(a.world.get::<Frozen>(other).map(|f| f.0), Some(9), "the key poses its component (a creator)");
    assert!(a.world.get::<NbtData>(other).is_none(), "so it is not kept free");

    a.world.entity_mut(other).insert(Frozen(0));
    assert_eq!(nbt::load(&mut a.world, other, &comp([("TicksFrozen", Tag::Int(9))])), Some(0));
    assert_eq!(a.world.get::<Frozen>(other).map(|f| f.0), Some(9));
}

#[test]
fn in_love_is_the_ticks_left_as_an_int() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = a_cow(&mut a.world);
    let now = now_of(&a.world, cow);
    a.world.entity_mut(cow).insert(Love(now + 30));
    assert_eq!(nbt::get(&a.world, cow, "InLove"), Some(Tag::Int(30)));

    let other = a_cow(&mut a.world);
    a.world.entity_mut(other).insert(Love(0));
    assert_eq!(nbt::load(&mut a.world, other, &comp([("InLove", Tag::Int(12))])), Some(0));
    assert_eq!(a.world.get::<Love>(other).map(|l| l.0), Some(now_of(&a.world, other) + 12));
    assert_eq!(nbt::load(&mut a.world, other, &comp([("InLove", Tag::Long(1))])), Some(1), "a wrong type is refused");
}

#[test]
fn persistence_required_is_a_byte_that_sets_and_takes_the_marker() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = a_cow(&mut a.world);
    a.world.entity_mut(cow).remove::<Persistent>();
    assert_eq!(nbt::get(&a.world, cow, "PersistenceRequired"), None, "no marker, no key");

    a.world.entity_mut(cow).insert(Persistent);
    assert_eq!(nbt::get(&a.world, cow, "PersistenceRequired"), Some(Tag::Byte(1)));

    assert_eq!(nbt::load(&mut a.world, cow, &comp([("PersistenceRequired", Tag::Byte(0))])), Some(0));
    assert!(a.world.get::<Persistent>(cow).is_none());
    assert_eq!(nbt::load(&mut a.world, cow, &comp([("PersistenceRequired", Tag::Byte(1))])), Some(0));
    assert!(a.world.get::<Persistent>(cow).is_some(), "1b poses the marker");
    assert_eq!(nbt::load(&mut a.world, cow, &comp([("PersistenceRequired", Tag::Int(1))])), Some(1), "a wrong type is refused");
}

#[test]
fn rotation_is_the_yaw_with_a_zero_pitch_and_a_load_sets_it() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = a_cow(&mut a.world);
    a.world.entity_mut(cow).insert(Yaw(1.5));
    assert_eq!(nbt::get(&a.world, cow, "Rotation"), Some(Tag::List(vec![Tag::Float(1.5f32.to_bits()), Tag::Float(0)])));

    let other = a_cow(&mut a.world);
    a.world.entity_mut(other).remove::<Yaw>();
    let rot = Tag::List(vec![Tag::Float(2.0f32.to_bits()), Tag::Float(0)]);
    assert_eq!(nbt::load(&mut a.world, other, &comp([("Rotation", rot)])), Some(0));
    assert_eq!(a.world.get::<Yaw>(other).map(|y| y.0), Some(2.0), "the load poses the yaw");
    assert_eq!(nbt::load(&mut a.world, other, &comp([("Rotation", Tag::Int(1))])), Some(1), "a wrong type is refused");
}

#[test]
fn silent_is_a_byte_that_sets_and_takes_the_marker() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = a_cow(&mut a.world);
    a.world.entity_mut(cow).insert(Silent);
    assert_eq!(nbt::get(&a.world, cow, "Silent"), Some(Tag::Byte(1)));
    assert_eq!(nbt::load(&mut a.world, cow, &comp([("Silent", Tag::Byte(0))])), Some(0));
    assert!(a.world.get::<Silent>(cow).is_none());
    assert_eq!(nbt::load(&mut a.world, cow, &comp([("Silent", Tag::Short(1))])), Some(1), "a wrong type is refused");
}

#[test]
fn home_pos_and_home_radius_are_the_centre_and_the_radius_of_home() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = a_cow(&mut a.world);
    a.world.entity_mut(cow).insert(Home { center: [1, 2, 3], radius: 16 });
    assert_eq!(nbt::get(&a.world, cow, "home_pos"), Some(Tag::IntArray(vec![1, 2, 3])));
    assert_eq!(nbt::get(&a.world, cow, "home_radius"), Some(Tag::Int(16)));

    let other = a_cow(&mut a.world);
    a.world.entity_mut(other).remove::<Home>();
    let saved = comp([("home_pos", Tag::IntArray(vec![4, 5, 6])), ("home_radius", Tag::Int(8))]);
    assert_eq!(nbt::load(&mut a.world, other, &saved), Some(0));
    let home = a.world.get::<Home>(other).expect("the load poses Home");
    assert_eq!((home.center, home.radius), ([4, 5, 6], 8));
    assert_eq!(nbt::load(&mut a.world, other, &comp([("home_radius", Tag::Long(1))])), Some(1), "a wrong type is refused");
}

#[test]
fn hurt_time_is_the_invulnerability_left_as_a_short() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = a_cow(&mut a.world);
    let now = now_of(&a.world, cow);
    a.world.entity_mut(cow).insert(HurtAt { at: now, last: 2.0 });
    assert_eq!(nbt::get(&a.world, cow, "HurtTime"), Some(Tag::Short(10)));

    let other = a_cow(&mut a.world);
    a.world.entity_mut(other).insert(HurtAt { at: 0, last: 0.0 });
    assert_eq!(nbt::load(&mut a.world, other, &comp([("HurtTime", Tag::Short(7))])), Some(0));
    assert_eq!(a.world.get::<HurtAt>(other).map(|h| h.at), Some(now_of(&a.world, other) + 3));
    assert_eq!(nbt::load(&mut a.world, other, &comp([("HurtTime", Tag::Int(7))])), Some(1), "a wrong type is refused");
}

#[test]
fn hurt_by_timestamp_is_the_tick_of_the_last_hurt_by_a_mob() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = a_cow(&mut a.world);
    a.world.entity_mut(cow).insert(LastHurtBy { who: Who::Player(1), at: 55 });
    assert_eq!(nbt::get(&a.world, cow, "HurtByTimestamp"), Some(Tag::Int(55)));

    let other = a_cow(&mut a.world);
    a.world.entity_mut(other).insert(LastHurtBy { who: Who::Player(2), at: 0 });
    assert_eq!(nbt::load(&mut a.world, other, &comp([("HurtByTimestamp", Tag::Int(70))])), Some(0));
    assert_eq!(a.world.get::<LastHurtBy>(other).map(|l| l.at), Some(70));
    assert_eq!(nbt::load(&mut a.world, other, &comp([("HurtByTimestamp", Tag::Long(70))])), Some(1), "a wrong type is refused");
}

#[test]
fn anger_end_time_is_the_end_of_the_countdown_as_a_long() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = a_cow(&mut a.world);
    let angry = Anger { def: &Params::DEFAULT, slot: 0, holding: false, until: 300, at: None, at_player: false, boosted: false };
    a.world.entity_mut(cow).insert(angry);
    assert_eq!(nbt::get(&a.world, cow, "anger_end_time"), Some(Tag::Long(300)));

    assert_eq!(nbt::load(&mut a.world, cow, &comp([("anger_end_time", Tag::Long(120))])), Some(0));
    assert_eq!(a.world.get::<Anger>(cow).map(|x| x.until), Some(120));
    assert_eq!(nbt::load(&mut a.world, cow, &comp([("anger_end_time", Tag::Int(1))])), Some(1), "a wrong type is refused");
}

#[test]
fn active_effects_is_the_list_of_effects_with_their_ticks_left() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = a_cow(&mut a.world);
    effects::add_flagged(&mut a.world, cow, Kind::Speed, 1, 100, HIDDEN | AMBIENT);
    let shown = Tag::compound([
        ("id", Tag::String("minecraft:speed".into())),
        ("amplifier", Tag::Byte(1)),
        ("duration", Tag::Int(100)),
        ("ambient", Tag::Byte(1)),
        ("show_particles", Tag::Byte(0)),
        ("show_icon", Tag::Byte(1)),
    ]);
    assert_eq!(nbt::get(&a.world, cow, "active_effects"), Some(Tag::List(vec![shown.clone()])));

    let other = a_cow(&mut a.world);
    assert_eq!(nbt::load(&mut a.world, other, &comp([("active_effects", Tag::List(vec![shown.clone()]))])), Some(0));
    assert!(effects::has(&a.world, other, Kind::Speed));
    let e = a.world.get::<Effects>(other).expect("the effects");
    assert_eq!((e.list[0].amp, e.list[0].ends), (1, now_of(&a.world, other) + 100));
    assert_eq!(nbt::get(&a.world, other, "active_effects"), Some(Tag::List(vec![shown])));
    assert_eq!(nbt::load(&mut a.world, other, &comp([("active_effects", Tag::Int(1))])), Some(1), "a wrong type is refused");
}

#[test]
fn a_default_free_key_is_dropped_and_another_value_is_kept() {
    assert!(common::drop_default("OnGround", &Tag::Byte(0)));
    assert!(!common::drop_default("OnGround", &Tag::Byte(1)));
    assert!(common::drop_default("fall_distance", &Tag::Double(0)));
    assert!(!common::drop_default("Foo", &Tag::Byte(0)), "a key with no default is kept");
}
