//! `GiveGiftToHero.getLootTableToThrow` (`mobs/villager.rs:480-495`): the pack's table of `neoforge:raid_hero_gifts` wins, else the vanilla
//! name of the profession. The expected values are the Mob's own map reader (`NeoForgeMobMaps::hero_gift`, what its `gift_table` calls).
use myriade_engine_lib::neoforge_mob_maps::NeoForgeMobMaps;
use horizon_ecs_proto::species::villager::gift_name;
use std::collections::BTreeMap;

fn pack(entries: &[(&str, &str)]) -> NeoForgeMobMaps {
    let mut maps = NeoForgeMobMaps::default();
    maps.set_gifts(entries.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>());
    maps
}

#[test]
fn a_pack_table_for_the_profession_overrides_the_vanilla_name() {
    let maps = pack(&[("minecraft:farmer", "test:gifts/farmer_pack")]);
    let got = gift_name(Some(&maps), "minecraft:farmer");
    assert_eq!(Some(got.as_str()), maps.hero_gift("minecraft:farmer").as_deref());
    assert_eq!(got, "test:gifts/farmer_pack");
}

#[test]
fn a_profession_missing_from_the_pack_falls_back_to_unemployed_as_the_mob_does() {
    let maps = pack(&[("minecraft:farmer", "test:gifts/farmer_pack")]);
    assert_eq!(gift_name(Some(&maps), "minecraft:librarian"), "minecraft:gameplay/hero_of_the_village/unemployed_gift");
}

#[test]
fn without_pack_maps_the_vanilla_name_is_used() {
    assert_eq!(gift_name(None, "minecraft:farmer"), "minecraft:gameplay/hero_of_the_village/farmer_gift");
    assert_eq!(gift_name(None, "minecraft:nitwit"), "minecraft:gameplay/hero_of_the_village/unemployed_gift");
    assert_eq!(gift_name(None, "minecraft:none"), "minecraft:gameplay/hero_of_the_village/unemployed_gift");
}
