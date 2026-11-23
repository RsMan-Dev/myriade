//! The vanilla NBT of a dropped item on the ECS (`load_entity` then `entity_nbt`) and what it spills: the short narrowing of `Age` and `PickupDelay`,
//! the floor of `Health`, the numeric streams of `Owner` and `Thrower`, the round trip, the lifetime counters over ticks.
use crate::game::ecs::test_support::{ecs_game_with, ecs_roundtrip, item_compound};
use horizon_core::nbt::{Compound, JavaString, Tag};

fn at(c: &Compound, key: &str) -> Option<Tag> {
    c.get(&JavaString::from(key)).cloned()
}

fn int_stream(c: &Compound, key: &str) -> Option<Vec<i32>> {
    match at(c, key)? {
        Tag::IntArray(v) => Some(v),
        _ => None,
    }
}

#[test]
#[ignore = "daily"]
fn an_item_narrows_its_age_and_pickup_delay_to_a_short() {
    let (mut game, _) = ecs_game_with("minecraft:cow", [0.0, 64.0, 0.0]);
    let load = |game: &mut crate::game::Game<'_>, fields: &str| {
        let saved = ecs_roundtrip(game, &item_compound(fields)).expect("the item is made");
        (at(&saved, "Age"), at(&saved, "PickupDelay"))
    };
    assert_eq!(load(&mut game, "Age:40000,PickupDelay:65546"), (Some(Tag::Short(-25536)), Some(Tag::Short(10))));
    assert_eq!(load(&mut game, "Age:65536,PickupDelay:40000"), (Some(Tag::Short(0)), Some(Tag::Short(-25536))));
    assert_eq!(load(&mut game, "Age:-32768s,PickupDelay:32767s"), (Some(Tag::Short(-32768)), Some(Tag::Short(32767))));
    assert_eq!(load(&mut game, "Age:5999s,PickupDelay:10s"), (Some(Tag::Short(5999)), Some(Tag::Short(10))));
    assert_eq!(load(&mut game, "Age:-0.5f,PickupDelay:9.9d"), (Some(Tag::Short(-1)), Some(Tag::Short(9))));
    assert_eq!(load(&mut game, "Age:-32768,PickupDelay:32767"), (Some(Tag::Short(-32768)), Some(Tag::Short(32767))));
    assert_eq!(load(&mut game, "Age:32768,PickupDelay:98303"), (Some(Tag::Short(-32768)), Some(Tag::Short(32767))));
}

#[test]
#[ignore = "daily"]
fn item_health_uses_floor_then_short_narrowing() {
    let (mut game, _) = ecs_game_with("minecraft:cow", [0.0, 64.0, 0.0]);
    let cases = [
        (None, 5),
        (Some(Tag::Short(5)), 5),
        (Some(Tag::Int(5)), 5),
        (Some(Tag::Long(5)), 5),
        (Some(Tag::Byte(5)), 5),
        (Some(Tag::Float((-1.5_f32).to_bits())), -2),
        (Some(Tag::Double((-1.5_f64).to_bits())), -2),
        (Some(Tag::Float(1.5_f32.to_bits())), 1),
        (Some(Tag::Double(1.5_f64.to_bits())), 1),
        (Some(Tag::Double((-40000.5_f64).to_bits())), 25535),
        (Some(Tag::Int(32767)), 32767),
        (Some(Tag::Int(32768)), -32768),
        (Some(Tag::Int(-32768)), -32768),
        (Some(Tag::Int(-32769)), 32767),
        (Some(Tag::Long(65535)), -1),
        (Some(Tag::Long(65536)), 0),
        (Some(Tag::Long(-65537)), -1),
        (Some(Tag::Long(i64::MAX)), -1),
    ];
    for (input, expected) in cases {
        let mut c = item_compound("");
        if let Some(input) = input {
            c.insert("Health".into(), input.clone());
        }
        let saved = ecs_roundtrip(&mut game, &c).expect("the item is made");
        assert_eq!(at(&saved, "Health"), Some(Tag::Short(expected)), "input {c:?}");
    }
}

#[test]
#[ignore = "daily"]
fn item_numeric_uuid_codec_adopts_numeric_streams_before_save() {
    let (mut game, _) = ecs_game_with("minecraft:cow", [0.0, 64.0, 0.0]);
    let cases = [
        (Tag::IntArray(vec![54, 1, -2, 3]), vec![54, 1, -2, 3]),
        (Tag::LongArray(vec![54, 4_294_967_297, -4_294_967_298, 9_007_199_254_740_993]), vec![54, 1, -2, 1]),
        (Tag::List(vec![Tag::Float(54.9_f32.to_bits()), Tag::Float(1.75_f32.to_bits()), Tag::Float((-2.75_f32).to_bits()), Tag::Float((-0.5_f32).to_bits())]), vec![54, 1, -2, 0]),
        (Tag::List(vec![Tag::Double(54.9_f64.to_bits()), Tag::Double(1.75_f64.to_bits()), Tag::Double((-2.75_f64).to_bits()), Tag::Double((-0.5_f64).to_bits())]), vec![54, 1, -2, 0]),
        (Tag::List(vec![Tag::Long(54), Tag::Long(4_294_967_297), Tag::Long(-4_294_967_298), Tag::Long(9_007_199_254_740_993)]), vec![54, 1, -2, 1]),
        (Tag::ByteArray(vec![54, 1, (-2_i8) as u8, 3]), vec![54, 1, -2, 3]),
        // Util.fixedSize keeps the first four of an overlong stream.
        (Tag::IntArray(vec![54, 1, -2, 3, 4]), vec![54, 1, -2, 3]),
    ];
    for field in ["Owner", "Thrower"] {
        for (value, expected) in &cases {
            let mut c = item_compound("");
            c.insert(field.into(), value.clone());
            let saved = ecs_roundtrip(&mut game, &c).expect("the item is made");
            assert_eq!(int_stream(&saved, field), Some(expected.clone()), "{field}: {value:?}");
            assert_eq!(saved.get(&JavaString::from("Item")).and_then(|i| match i { Tag::Compound(i) => i.get(&JavaString::from("count")).cloned(), _ => None }), Some(Tag::Int(1)));
        }
    }
}

#[test]
#[ignore = "daily"]
fn item_numeric_uuid_codec_rejects_nonstreams_and_short_inputs() {
    let (mut game, _) = ecs_game_with("minecraft:cow", [0.0, 64.0, 0.0]);
    let cases = [
        None,
        Some(Tag::String("00000036-0000-0001-ffff-fffe00000003".into())),
        Some(Tag::IntArray(vec![])),
        Some(Tag::IntArray(vec![54, 1, -2])),
        Some(Tag::Int(7)),
        Some(Tag::compound([("value", Tag::Int(7))])),
        Some(Tag::List(vec![Tag::Int(54), Tag::Int(1), Tag::Int(-2), Tag::String("3".into())])),
    ];
    for field in ["Owner", "Thrower"] {
        for value in &cases {
            let mut c = item_compound("");
            if let Some(value) = value {
                c.insert(field.into(), value.clone());
            }
            let saved = ecs_roundtrip(&mut game, &c).expect("the item is made");
            assert!(saved.get(&JavaString::from(field)).is_none(), "{field}: {value:?}");
        }
    }
}

#[test]
#[ignore = "daily"]
fn item_entities_round_trip() {
    let (mut game, _) = ecs_game_with("minecraft:cow", [0.0, 64.0, 0.0]);
    let c = item_compound("Health:2s,Fire:73s,Age:120s,PickupDelay:10s,Thrower:[I;5,6,7,8],Item:{id:\"minecraft:diamond_pickaxe\",count:1,tag:{Damage:5}}");
    let saved = ecs_roundtrip(&mut game, &c).expect("the item is made");
    assert_eq!(at(&saved, "id"), Some(Tag::String("minecraft:item".into())));
    assert_eq!(at(&saved, "Health"), Some(Tag::Short(2)));
    assert_eq!(at(&saved, "Fire"), Some(Tag::Short(73)));
    assert_eq!((at(&saved, "Age"), at(&saved, "PickupDelay")), (Some(Tag::Short(120)), Some(Tag::Short(10))));
    assert_eq!(int_stream(&saved, "Thrower"), Some(vec![5, 6, 7, 8]));
    let again = ecs_roundtrip(&mut game, &saved).expect("the saved item is made again");
    assert_eq!(again.get(&JavaString::from("Thrower")), saved.get(&JavaString::from("Thrower")));
}

#[test]
#[ignore = "daily"]
fn loaded_item_lifetime_counters_tick_with_signed_sentinels() {
    // An item only ticks in a chunk that ticks (`ServerLevel.tick`): a player stands in the chunk, as the game's tick reads it.
    let mut resources = None;
    let mut game = crate::game::ecs::test_support::active_server_game(&mut resources);
    game.build_registries();
    game.world.chunks.insert((0, 0), std::sync::Arc::new(horizon_client::chunk::Chunk::empty(0, 0)));
    crate::game::native_guest_turn_tests::lan_guests_on(&mut game);
    game.update_chunk_tickets();
    for delay in [32767_i16, -1, 0, 1, 2] {
        for age in [-32768_i16, -1, 0, 5998, 5999, 6000, 32767] {
            let c = item_compound(&format!("NoGravity:1b,Pos:[4.5d,210d,4.5d],Age:{age}s,PickupDelay:{delay}s"));
            let e = game.ecs_with(|ecs, mw| ecs.load_entity(mw, &c)).expect("the item is made");
            let mut alive = true;
            for step in 0..=3 {
                if step > 0 && alive {
                    game.tick().unwrap();
                    alive = game.ecs.entity_nbt(e).is_some();
                }
                let expected_age = if age == i16::MIN { i32::from(age) } else { i32::from(age) + step };
                assert_eq!(alive, step == 0 || expected_age < 6000, "age={age}, delay={delay}, tick={step}");
                if !alive {
                    continue;
                }
                let saved = game.ecs.entity_nbt(e).unwrap();
                let expected_delay = if delay > 0 && delay != i16::MAX { (i32::from(delay) - step).max(0) } else { i32::from(delay) };
                assert_eq!(at(&saved, "Age"), Some(Tag::Short(expected_age as i16)), "age={age}, tick={step}");
                assert_eq!(at(&saved, "PickupDelay"), Some(Tag::Short(expected_delay as i16)), "age={age}, delay={delay}, tick={step}");
            }
        }
    }
}

#[test]
#[ignore = "daily"]
fn unknown_fields_survive() {
    let (mut game, _) = ecs_game_with("minecraft:cow", [0.0, 64.0, 0.0]);
    let c = item_compound("CustomName:\"x\"");
    let saved = ecs_roundtrip(&mut game, &c).expect("the item is made");
    assert_eq!(at(&saved, "CustomName"), Some(Tag::String("x".into())));
    assert_eq!(at(&saved, "Health"), Some(Tag::Short(5)));
}

#[test]
#[ignore = "daily"]
fn a_stars_negative_age_survives_item_nbt() {
    let (mut game, _) = ecs_game_with("minecraft:cow", [0.0, 64.0, 0.0]);
    let saved = ecs_roundtrip(&mut game, &item_compound("Age:-6000s,Item:{id:\"minecraft:nether_star\",count:1}")).expect("the item is made");
    assert_eq!(at(&saved, "Age"), Some(Tag::Short(-6000)));
    assert_eq!(saved.get(&JavaString::from("Item")).and_then(|i| match i { Tag::Compound(i) => i.get(&JavaString::from("id")).cloned(), _ => None }), Some(Tag::String("minecraft:nether_star".into())));
}

/// `ItemUtils.onContainerDestroyed` makes each stack with `new ItemEntity(level, x, y, z, stack)`: 0.2 up, ±0.1 sideways from the item's own random.
#[test]
#[ignore = "daily"]
fn spilled_stacks_rise_from_their_own_randoms() {
    let mut resources = None;
    let mut game = crate::game::ecs::test_support::active_server_game(&mut resources);
    let at = horizon_core::geometry::Vec3::new(8.5, 70.0, 8.5);
    use horizon_core::random::RandomSource;
    let mut level = game.world.random.clone();
    for id in ["minecraft:stone", "minecraft:dirt", "minecraft:glass"] {
        game.spawn_item_own(horizon_client::items::ItemStack::new(id, 1), [at.x, at.y, at.z], 0);
    }
    let motions: Vec<[f64; 3]> = game.ecs.item_views().iter().map(|item| [item.velocity.x, item.velocity.y, item.velocity.z]).collect();
    assert_eq!(motions.len(), 3);
    for motion in &motions {
        assert_eq!(motion[1], 0.2, "the four-argument constructor rises 0.2");
        assert!((-0.1..0.1).contains(&motion[0]), "{motion:?}");
        assert!((-0.1..0.1).contains(&motion[2]), "{motion:?}");
    }
    assert_ne!(motions[0], motions[1]);
    assert_ne!(motions[1], motions[2]);
    assert_ne!(motions[0], motions[2]);
    // The spill leaves the level's random alone.
    assert_eq!(game.world.random.next_double(), level.next_double());
    assert_eq!(game.world.random.next_double(), level.next_double());
}

/// The damage command is one of those callers: killing a shulker box item spills its contents with the same upward motion.
#[test]
#[ignore = "daily"]
fn a_damaged_shulker_box_spills_its_stacks_upward() {
    let mut resources = None;
    let mut game = crate::game::ecs::test_support::active_server_game(&mut resources);
    game.build_registries();
    game.set_allow_commands(true);
    game.world.chunks.insert((0, 0), std::sync::Arc::new(horizon_client::chunk::Chunk::empty(0, 0)));
    crate::game::native_guest_turn_tests::lan_guests_on(&mut game);
    game.update_chunk_tickets();
    let boxed = horizon_client::item_data::parse_stack(
        r#"{id:"minecraft:shulker_box",count:1,components:{"minecraft:container":[
            {slot:0,item:{id:"minecraft:stone",count:2}},
            {slot:4,item:{id:"minecraft:dirt",count:1}},
            {slot:8,item:{id:"minecraft:glass",count:3}}]}}"#,
    )
    .unwrap()
    .unwrap();
    game.spawn_item(boxed, [8.5, 70.0, 8.5], [0.0; 3], 0);
    assert_eq!(game.ecs.item_ids().len(), 1);
    assert!(game.run_command("damage @e[type=minecraft:item,limit=1] 100 minecraft:generic"));
    let mut spilled: Vec<_> = game.ecs.item_views().iter().map(|item| (item.stack.item.to_string(), item.stack.count)).collect();
    spilled.sort();
    assert_eq!(
        spilled,
        [("minecraft:dirt".to_owned(), 1), ("minecraft:glass".to_owned(), 3), ("minecraft:stone".to_owned(), 2)]
    );
    for item in game.ecs.item_views() {
        assert_eq!(item.velocity.y, 0.2, "{item:?}");
    }
}
