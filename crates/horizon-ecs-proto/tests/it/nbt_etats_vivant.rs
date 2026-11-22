//! The saved states of a living creature (lot 86): each one round-trips through the NBT with the keys of vanilla, a creature without state
//! writes none of them, a refused tag changes nothing, and `noActionTime` is read on the creature's own clock. Needs the vanilla data:
//! the tests return early without it, as the other NBT tests do.
use horizon_client_lib::items::ItemStack;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_ecs_proto::{
    absorption::Absorption,
    cow::Herd,
    despawn::{self, DespawnDef, Removable},
    nbt,
    no_gravity::{self, NoGravity},
};
use horizon_items::item_nbt::item_to_nbt;

fn cow(extra: &[(&str, Tag)]) -> Compound {
    let pos = Tag::List(vec![Tag::Double(0.5f64.to_bits()), Tag::Double(1.0f64.to_bits()), Tag::Double(0.5f64.to_bits())]);
    let mut c = Compound::default();
    c.insert("id".into(), Tag::String("minecraft:cow".into()));
    c.insert("Pos".into(), pos);
    for (k, v) in extra {
        c.insert((*k).into(), v.clone());
    }
    c
}

/// An enchanted sword with `damage` points of wear: its enchantments are a stand-in tag, the piece keeps its components whole.
fn sword(damage: u32) -> ItemStack {
    let mut s = ItemStack::new("minecraft:diamond_sword", 1);
    s.damage = damage;
    s.data_mut().set_other("minecraft:enchantments", Some(Tag::String("minecraft:sharpness=5".into())));
    s
}

/// One state of each key: absorption, a modifier, the sword in the main hand, a drop chance, a visible name, no gravity, a glide, a left hand.
fn states() -> Vec<(&'static str, Tag)> {
    let modifier = Tag::compound([
        ("id", Tag::String("test:boost".into())),
        ("amount", Tag::Double(0.25f64.to_bits())),
        ("operation", Tag::String("add_multiplied_base".into())),
    ]);
    let attribute = Tag::compound([("id", Tag::String("minecraft:movement_speed".into())), ("modifiers", Tag::List(vec![modifier]))]);
    let equipment = Tag::compound([("mainhand", item_to_nbt(&sword(7)))]);
    let drops = Tag::compound([("mainhand", Tag::Float(0.5f32.to_bits()))]);
    vec![
        ("AbsorptionAmount", Tag::Float(4.0f32.to_bits())),
        ("attributes", Tag::List(vec![attribute])),
        ("equipment", equipment),
        ("drop_chances", drops),
        ("CustomName", Tag::String("\"Rex\"".into())),
        ("CustomNameVisible", Tag::Byte(1)),
        ("NoGravity", Tag::Byte(1)),
        ("FallFlying", Tag::Byte(1)),
        ("LeftHanded", Tag::Byte(1)),
    ]
}

#[test]
fn every_state_round_trips_with_its_vanilla_key() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, false, false);
    h.tick();
    let original = states();
    let e = nbt::restore(&mut h.world, &cow(&original)).expect("a cow");
    let saved = nbt::save(&h.world, e).expect("a saved cow");
    for (k, v) in &original {
        assert_eq!(saved.get(&JavaString::from(*k)), Some(v), "{k} is not written back as it was read");
    }
    // The saved compound, restored in a second creature, is written the same.
    let again = nbt::restore(&mut h.world, &saved).expect("a second cow");
    let resaved = nbt::save(&h.world, again).expect("a second saved cow");
    for (k, v) in &original {
        assert_eq!(resaved.get(&JavaString::from(*k)), Some(v), "{k} changed by a second round trip");
    }
    assert!(h.world.get::<Absorption>(again).is_some_and(|a| a.0 == 4.0));
    assert!(no_gravity::get(&h.world, again));
}

#[test]
fn a_creature_without_state_writes_none_of_them() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, false, false);
    h.tick();
    let e = nbt::restore(&mut h.world, &cow(&[])).expect("a cow");
    let saved = nbt::save(&h.world, e).expect("a saved cow");
    for k in ["CustomName", "CustomNameVisible", "NoGravity", "equipment", "drop_chances"] {
        assert!(saved.get(&JavaString::from(k)).is_none(), "{k} written without its component");
    }
    // The living defaults of `common` (what vanilla writes at rest) are the only ones allowed, and they must be the defaults.
    assert!(saved.get(&JavaString::from("AbsorptionAmount")).is_none_or(|t| *t == Tag::Float(0)));
    assert!(saved.get(&JavaString::from("attributes")).is_none_or(|t| *t == Tag::List(Vec::new())));
    assert!(saved.get(&JavaString::from("FallFlying")).is_none_or(|t| *t == Tag::Byte(0)));
}

#[test]
fn a_refused_tag_gives_no_state() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, false, false);
    h.tick();
    let wrong = cow(&[("AbsorptionAmount", Tag::Int(4)), ("NoGravity", Tag::Int(1))]);
    let e = nbt::restore(&mut h.world, &wrong).expect("a cow");
    assert!(h.world.get::<Absorption>(e).is_none());
    assert!(!no_gravity::get(&h.world, e));
}

#[test]
fn no_action_time_is_read_on_the_own_clock() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, false, false);
    let e = nbt::restore(&mut h.world, &cow(&[])).expect("a cow");
    let def = DespawnDef { distance: 128.0, removable: Removable::Never, near: 32.0, idle: 600, chance: 800 };
    despawn::admit(&mut h.world, e, def, f64::INFINITY);
    for _ in 0..100 {
        h.tick();
    }
    despawn::set_idle(&mut h.world, e, 100);
    assert_eq!(despawn::idle_for(&h.world, e), Some(100));
    for _ in 0..20 {
        h.tick();
    }
    assert_eq!(despawn::idle_for(&h.world, e), Some(120));
    despawn::touch(&mut h.world, e);
    assert_eq!(despawn::idle_for(&h.world, e), Some(0));
}
