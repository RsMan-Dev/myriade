//! The bed points of interest read by the ECS registry (`classify` of `poi.rs`, through the changed blocks): vanilla 26.3 bed states
//! (`VanillaBedPoiOracle`, `tools/test_bed_poi_parity.py`), a block a mod gave to `minecraft:home` (`PoiExtension`), and head against foot.
//! `HORIZON_TEST_RESOURCES` set.
use crate::common;
use common::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::poi::PoiRegistry;
use horizon_world::chunk::BlockState;
use horizon_world::poi::{PoiExtension, PoiType, saved_poi, with_extensions};

fn state(name: &str, props: &[(&str, &str)]) -> BlockState {
    BlockState {
        name: name.into(),
        properties: props.iter().map(|&(k, v)| (k.into(), v.into())).collect(),
    }
}

/// Every state of the oracle file, as a bed the registry of the ECS reads: a head is a home point, nothing else is.
#[test]
#[ignore = "requires the vanilla bed POI oracle output"]
fn vanilla_bed_poi_states() {
    if !have_data() {
        return;
    }
    let path = std::env::var("HORIZON_BED_POI_ORACLE").expect("oracle output path");
    let rows = std::fs::read_to_string(path).unwrap();
    assert_eq!(rows.lines().count(), 272);
    let mut h = Herd::with_slab_awake(40, true, false);
    h.tick();
    let mut checks = Vec::new();
    for (i, row) in rows.lines().enumerate() {
        let fields: Vec<_> = row.split_whitespace().collect();
        let [name, part, expected] = fields[..] else {
            panic!("invalid oracle row")
        };
        let p = [i as i32 % 17 - 8, 1, i as i32 / 17 - 8];
        let name: &'static str = Box::leak(name.to_string().into_boxed_str());
        let part: &'static str = Box::leak(part.to_string().into_boxed_str());
        h.set_block_state(p[0], p[1], p[2], name, Box::leak(Box::new([("part", part)])));
        checks.push((p, expected == "true", state(name, &[("part", part)]), row));
    }
    h.tick();
    let r = h.world.resource::<PoiRegistry>();
    for (p, expected, saved, row) in &checks {
        assert_eq!(r.get(*p).map(|x| x.kind) == Some(PoiType::Home), *expected, "{row}");
        assert_eq!(saved_poi(saved) == Some(PoiType::Home), *expected, "{row}");
    }
}

/// A block a mod added to `minecraft:home` is a bed for the registry; without the addition, vanilla gives hay bales no point.
#[test]
fn a_block_a_mod_added_to_home_is_a_bed_to_the_ecs() {
    if !have_data() {
        return;
    }
    let additions = ["x", "y", "z"]
        .into_iter()
        .map(|axis| PoiExtension {
            state: state("minecraft:hay_block", &[("axis", axis)]),
            poi: "minecraft:home".into(),
        })
        .collect();
    with_extensions(additions, |refusals| {
        assert!(refusals.is_empty(), "{refusals:?}");
        let mut h = Herd::with_slab_awake(40, true, false);
        h.tick();
        h.set_block(0, 1, 0, "minecraft:hay_block");
        h.tick();
        assert_eq!(h.world.resource::<PoiRegistry>().get([0, 1, 0]).map(|x| x.kind), Some(PoiType::Home));
    });
    with_extensions(Vec::new(), |_| {
        let mut h = Herd::with_slab_awake(40, true, false);
        h.tick();
        h.set_block(0, 1, 0, "minecraft:hay_block");
        h.tick();
        assert!(h.world.resource::<PoiRegistry>().get([0, 1, 0]).is_none());
    });
}

/// The bed heads the registry reads hot are the saved home points, the feet and the straw bed are neither.
#[test]
fn bed_heads_agree_between_the_registry_and_the_saved_points() {
    if !have_data() {
        return;
    }
    let rows: [(&'static str, &'static [(&'static str, &'static str)], bool); 4] = [
        ("minecraft:straw_bed", &[("part", "head")], false),
        ("minecraft:straw_bed", &[("part", "foot")], false),
        ("minecraft:red_bed", &[("part", "head")], true),
        ("minecraft:red_bed", &[("part", "foot")], false),
    ];
    let mut h = Herd::with_slab_awake(40, true, false);
    h.tick();
    for (i, &(name, props, _)) in rows.iter().enumerate() {
        h.set_block_state(i as i32 * 2, 1, 0, name, props);
    }
    h.tick();
    let r = h.world.resource::<PoiRegistry>();
    for (i, &(name, props, expected)) in rows.iter().enumerate() {
        let p = [i as i32 * 2, 1, 0];
        assert_eq!(r.get(p).map(|x| x.kind) == Some(PoiType::Home), expected, "{name} {props:?} (registry)");
        assert_eq!(saved_poi(&state(name, props)) == Some(PoiType::Home), expected, "{name} {props:?} (saved)");
    }
}
