//! The variant draws by biome (`VariantUtils.selectVariantToSpawn`: highest priority holding, one drawn among ties) and by group
//! (the group's value overrides the draw, which is still made when the table says so).
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::rules::variant::{draw_from, inherited, Choice, Inherit, Need, Pick, Place, Row, Table};

const LIST: &[Choice] = &[
    Choice { id: 0, priority: 0, need: None },
    Choice { id: 1, priority: 1, need: Some(Need::Biomes(&["minecraft:snowy_plains"])) },
    Choice { id: 2, priority: 1, need: Some(Need::Biomes(&["#minecraft:is_taiga"])) },
];

fn table(redraw: bool) -> Table {
    Table {
        bound: 0,
        rows: &[],
        otherwise: Row { tag: None, cuts: &[], last: 0 },
        key: "variant",
        pick: Pick::Registry(LIST),
        redraw,
        ids: &[0, 1, 2],
        names: &[],
        traits: &[],
        group_baby: None,
        inherit: Inherit::Blend { total: 9, own: 4, other: 4 },
    }
}

fn at(biome: &str, group: Option<u8>, redraw: bool) -> u8 {
    let has = |t: &str| t == "minecraft:is_taiga" && biome == "minecraft:taiga";
    let place = Place { biome, biome_has: &has, in_structure: &|_| false, moon: 0.0 };
    draw_from(&table(redraw), group, &place, &mut LegacyRandom::new(7))
}

#[test]
fn biome_picks_the_highest_priority_entry() {
    assert_eq!(at("minecraft:snowy_plains", None, false), 1);
    assert_eq!(at("minecraft:taiga", None, false), 2);
    assert_eq!(at("minecraft:desert", None, false), 0);
}

#[test]
fn structure_and_moon_conditions_read_the_place() {
    const BLACK: &[Choice] = &[
        Choice { id: 0, priority: 0, need: None },
        Choice { id: 9, priority: 1, need: Some(Need::Structures(&["#minecraft:cats_spawn_as_black"])) },
        Choice { id: 8, priority: 0, need: Some(Need::MoonAtLeast(0.9)) },
    ];
    let t = Table { pick: Pick::Registry(BLACK), ids: &[0, 8, 9], ..table(false) };
    let draw = |in_s: bool, moon: f32| {
        let inside = move |s: &str| in_s && s == "minecraft:cats_spawn_as_black";
        let place = Place { biome: "minecraft:plains", biome_has: &|_| false, in_structure: &inside, moon };
        draw_from(&t, None, &place, &mut LegacyRandom::new(7))
    };
    assert_eq!(draw(true, 0.0), 9);
    assert_eq!(draw(true, 1.0), 9);
    assert_ne!(draw(false, 0.0), 9);
    assert_ne!(draw(false, 0.0), 8);
}

#[test]
fn breeding_blend_follows_its_weights() {
    let t = table(false);
    let mut r = LegacyRandom::new(11);
    let (mut own, mut other, mut fresh) = (0, 0, 0);
    for _ in 0..9000 {
        match inherited(&t, 1, 2, &mut r) {
            Some(1) => own += 1,
            Some(2) => other += 1,
            _ => fresh += 1,
        }
    }
    assert!((3600..4400).contains(&own), "{own}");
    assert!((3600..4400).contains(&other), "{other}");
    assert!((600..1400).contains(&fresh), "{fresh}");
    assert_eq!(inherited(&Table { inherit: Inherit::None, ..t }, 1, 2, &mut r), None);
}

#[test]
fn group_value_overrides_the_biome() {
    assert_eq!(at("minecraft:snowy_plains", Some(2), false), 2);
    assert_eq!(at("minecraft:snowy_plains", Some(2), true), 2);
}
