//! The dragon's flight graph and flight against Horizon's `Dragon` (`game/mobs/dragon.rs`, `dragon_nodes`, `dragon_path`, `closest_node`,
//! `tick_dragon`), on flat ground at 64, with imposed draws (`LegacyRandom` at fixed seeds). The golden lines come from the reconciler,
//! which runs Horizon's side in a test of `horizon-client`; this file reads them from `tests/it/dragon_golden/flight.txt` and compares them
//! to the ECS side, to the bit. Lines: `node I X Y Z`, `path A B CRYSTALS X/Y/Z;...|none` (the node positions), `closest CRYSTALS X Y Z I`,
//! `tick T POS_BITS YAW_BITS PHASE`.
use horizon_core::geometry::Vec3;
use horizon_core::random::LegacyRandom;
use crate::common::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::dragon::flight::{DragonRng, FlightHistory};
use horizon_ecs_proto::dragon::nodes::{self, FlightNodes, LAYOUT};
use horizon_ecs_proto::dragon::phases::{self, PhaseId};

/// Ground height of every column (flat ground at 64).
const GROUND: i32 = 64;
/// The node pairs walked by `find_path`: with the inner ring (crystals alive) and without (the inner ring out).
const PAIRS: [(usize, usize, u16); 6] = [(0, 5, 1), (12, 20, 1), (3, 15, 1), (18, 22, 1), (11, 0, 0), (13, 23, 0)];
/// Points probed by `closest`: a column near the pillar, one beside a ring node, one far out.
const PROBES: [(f64, f64, f64, u16); 4] = [(0.5, 80.0, 0.5, 1), (58.0, 70.0, 6.0, 1), (-12.0, 66.0, 25.0, 0), (30.0, 90.0, -30.0, 1)];

fn bits(v: Vec3) -> String {
    format!("{:016x} {:016x} {:016x}", v.x.to_bits(), v.y.to_bits(), v.z.to_bits())
}

/// The ECS side, line by line, in the golden's order.
fn ours() -> Vec<String> {
    let mut out = Vec::new();
    let g = FlightNodes::build(&LAYOUT, |_, _| GROUND);
    for (i, n) in g.nodes.iter().enumerate() {
        out.push(format!("node {i} {} {} {}", n[0], n[1], n[2]));
    }
    for (a, b, c) in PAIRS {
        let line = match nodes::find_path(&g, c, a, b, None) {
            Some(p) => p.nodes.iter().map(|&i| format!("{}/{}/{}", g.nodes[i][0], g.nodes[i][1], g.nodes[i][2])).collect::<Vec<_>>().join(";"),
            None => "none".to_string(),
        };
        out.push(format!("path {a} {b} {c} {line}"));
    }
    for (x, y, z, c) in PROBES {
        out.push(format!("closest {c} {x} {y} {z} {}", g.closest(c, Vec3::new(x, y, z))));
    }

    // The Holding dragon from (0.5, 80, 0.5), seed 5, 300 ticks of the real schedule: position and yaw bits and the phase, per tick.
    let mut h = Herd::with_slab_awake(40, false, false);
    h.world.insert_resource(FlightNodes::build(&LAYOUT, |_, _| GROUND));
    crate::dragon_support::lay_ground(&mut h);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:ender_dragon").expect("the dragon's species");
    let e = spawn_armed(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.entity_mut(e).insert((Pos(Vec3::new(0.5, 80.0, 0.5)), Yaw(0.0), DragonRng(LegacyRandom::new(5), LegacyRandom::new(9)), FlightHistory::new(80.0, 0.0), horizon_ecs_proto::dragon::parts::Parts::default()));
    phases::set_phase(&mut h.world, e, phases::HOLDING);
    for t in 0..300 {
        h.tick();
        let (p, y, ph) = (h.world.get::<Pos>(e).unwrap().0, h.world.get::<Yaw>(e).unwrap().0, h.world.get::<PhaseId>(e).unwrap().0);
        out.push(format!("tick {t} {} {:08x} {ph}", bits(p), y.to_bits()));
    }
    out
}

#[test]
fn flight_matches_horizon_to_the_bit() {
    if !have_data() {
        return;
    }
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/dragon_golden/flight.txt");
    let golden = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("golden {path} missing (the reconciler writes it): {e}"));
    let ours = ours();
    if let Some(p) = std::env::var_os("HORIZON_DRAGON_DUMP") {
        std::fs::write(std::path::Path::new(&p).join("flight.txt"), ours.join("\n")).unwrap();
    }
    let want: Vec<&str> = golden.trim_end().lines().collect();
    for (i, (a, b)) in ours.iter().zip(&want).enumerate() {
        assert_eq!(a, b, "line {i}: the ECS side (left) differs from Horizon's Dragon (right)");
    }
    assert_eq!(ours.len(), want.len(), "line count");
}
