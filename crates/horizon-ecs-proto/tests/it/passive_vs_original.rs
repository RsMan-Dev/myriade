//! Passive entities against Horizon's rules for them: the saved compound of a display (`Display.addAdditionalSaveData`), the saved
//! settings of an interaction (`Interaction.readAdditionalSaveData`, `addAdditionalSaveData`) and its box (`refreshDimensions`,
//! the values of the unit test `the_box_is_centred_on_the_feet_and_grows_with_the_data`). The expected values are written from
//! Horizon's code, not from the ECS: Horizon's own functions are `pub(super)` and out of reach from here.
use bevy_ecs::world::World;
use horizon_core::{geometry::Vec3, nbt::Tag};
use horizon_ecs_proto::{cow::{Moving, OwnTime, Pos}, nbt, passive::{self, Interact}};

fn at() -> Vec3 {
    Vec3::new(10.0, 64.0, -3.0)
}

/// A world with the declared NBT keys of the common fields and of the passive kinds, as the herd's.
fn world() -> World {
    let mut w = World::new();
    nbt::register(&mut w, nbt::COMMON);
    nbt::register(&mut w, passive::FIELDS);
    for id in passive::kinds().chain(["custom:thing"]) {
        horizon_ecs_proto::nbt::declare(&mut w, id, horizon_ecs_proto::nbt::Recipe { saved: true, build: horizon_ecs_proto::nbt::Build::Passive });
    }
    w
}

#[test]
fn a_block_display_saves_its_settings_as_horizon_does() {
    let mut w = world();
    let src = Tag::compound([
        ("view_range", Tag::Float(2.0f32.to_bits())),
        ("billboard", Tag::String("bogus".into())),
        ("start_interpolation", Tag::Int(3)),
        ("teleport_duration", Tag::Int(99)),
        ("shadow_radius", Tag::Float((-0.0f32).to_bits())),
    ]);
    let e = passive::spawn(&mut w, "minecraft:block_display", at(), src);
    let saved = passive::save(&w, e);
    let want = Tag::compound([
        ("id", Tag::String("minecraft:block_display".into())),
        ("view_range", Tag::Float(2.0f32.to_bits())),
        ("billboard", Tag::String("fixed".into())),
        ("interpolation_duration", Tag::Int(0)),
        ("teleport_duration", Tag::Int(59)),
        ("shadow_radius", Tag::Float(0)),
        ("shadow_strength", Tag::Float(1.0f32.to_bits())),
        ("width", Tag::Float(0)),
        ("height", Tag::Float(0)),
        ("glow_color_override", Tag::Int(-1)),
        ("block_state", Tag::String("minecraft:air".into())),
    ]);
    let Tag::Compound(got) = saved else { panic!("not a compound") };
    let Tag::Compound(want) = want else { unreachable!() };
    for (key, value) in &want {
        assert_eq!(got.get(key), Some(value), "{:?}", key);
    }
    assert!(!got.contains_key(&"start_interpolation".into()));
    let pos = w.get::<Pos>(e).unwrap().0;
    assert_eq!((pos.x, pos.y, pos.z), (10.0, 64.0, -3.0));
}

#[test]
fn an_interaction_box_is_centred_on_the_feet_and_grows_with_the_data() {
    let mut w = world();
    let e = passive::spawn(&mut w, "minecraft:interaction", at(), Tag::compound([("width", Tag::Float(2.2f32.to_bits())), ("height", Tag::Float(2.2f32.to_bits())), ("response", Tag::Byte(1))]));
    let i = w.get::<Interact>(e).unwrap().clone();
    let b = i.bounds(at());
    let half = f64::from(2.2f32) / 2.0;
    let lo = b.min_position().components();
    let hi = b.max_position().components();
    for (got, want) in lo.iter().zip([10.0 - half, 64.0, -3.0 - half]) {
        assert!((got - want).abs() < 1e-6, "{lo:?}");
    }
    for (got, want) in hi.iter().zip([10.0 + half, 64.0 + f64::from(2.2f32), -3.0 + half]) {
        assert!((got - want).abs() < 1e-6, "{hi:?}");
    }
    assert!(i.response);
    // The box's keys leave the kept compound: `Interact` is their one copy.
    assert!(w.get::<nbt::NbtData>(e).is_none_or(|d| d.0.get(&"width".into()).is_none()));
}

#[test]
fn an_interaction_without_size_is_one_block_and_keeps_its_last_actions() {
    let mut w = world();
    let player = Tag::IntArray(vec![1, 2, 3, 4]);
    let act = Tag::compound([("player", player), ("timestamp", Tag::Long(77))]);
    let e = passive::spawn(&mut w, "minecraft:interaction", at(), Tag::compound([("attack", act.clone())]));
    let i = w.get::<Interact>(e).unwrap().clone();
    assert_eq!((i.width, i.height, i.response), (1.0, 1.0, false));
    assert_eq!(i.attack.as_deref().map(|a| (a.player, a.timestamp)), Some(([1, 2, 3, 4], 77)));
    let saved = passive::save(&w, e);
    assert_eq!(saved.get("attack"), Some(&act));
    assert_eq!(saved.get("response"), Some(&Tag::Byte(0)));
}

#[test]
fn a_passive_entity_is_never_visited_by_a_tick() {
    let mut w = world();
    let e = passive::spawn(&mut w, "minecraft:marker", at(), Tag::compound([("data", Tag::compound([("k", Tag::Int(1))]))]));
    assert!(!w.entity(e).contains::<Moving>());
    assert!(!w.entity(e).contains::<OwnTime>());
}
