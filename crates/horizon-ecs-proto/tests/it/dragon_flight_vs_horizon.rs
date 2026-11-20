//! The dragon's flight graph and flight against Horizon's `Dragon` (`game/mobs/dragon.rs`, `dragon_nodes`, `dragon_path`, `closest_node`,
//! `tick_dragon`), on flat ground at 64, with imposed draws (`LegacyRandom` at fixed seeds). The golden lines come from the reconciler,
//! which runs Horizon's side in a test of `horizon-client`; this file reads them from `tests/it/dragon_golden/flight.txt` and compares them
//! to the ECS side, to the bit. Lines: `node I X Y Z`, `path A B CRYSTALS X/Y/Z;...|none` (the node positions), `closest CRYSTALS X Y Z I`,
//! `tick T POS_BITS YAW_BITS`.
use horizon_core::geometry::Vec3;
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::dragon::flight::{self, Flight, FlightHistory, FlyOrder, LevelRandom, Params};
use horizon_ecs_proto::dragon::nodes::{self, FlightNodes, LAYOUT};
use horizon_ecs_proto::host_out::HostOut;

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

    // A fixed fly target above the far side, 300 ticks, from the spawn at the pillar's top.
    let order = FlyOrder { target: Some(Vec3::new(30.5, 80.0, 30.5)), speed: 0.6, landing: false, sitting: false };
    let mut f = Flight {
        cfg: Params::DEFAULT,
        rng: LegacyRandom::new(5),
        flap: 0.0,
        old_flap: 0.0,
        growl: 0,
        ambient: 0,
        in_wall: false,
        scan_due: false,
    };
    let mut hist = FlightHistory::new(80.0, 0.0);
    let mut host = HostOut::default();
    let mut level = LevelRandom(LegacyRandom::new(9));
    let (mut pos, mut vel, mut yaw, mut accel) = (Vec3::new(0.5, 80.0, 0.5), Vec3::new(0.0, 0.0, 0.0), 0.0f32, 0.0f32);
    for t in 0..300 {
        flight::pre(&mut f, &order, pos, vel, &mut yaw, &mut hist, &mut level, &mut host);
        flight::steer(&order, &f, &mut pos, &mut vel, &mut yaw, &mut accel);
        out.push(format!("tick {t} {} {:08x}", bits(pos), yaw.to_bits()));
    }
    out
}

#[test]
fn flight_matches_horizon_to_the_bit() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/dragon_golden/flight.txt");
    let golden = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("golden {path} missing (the reconciler writes it): {e}"));
    let ours = ours().join("\n");
    assert_eq!(ours, golden.trim_end(), "flight lines differ from Horizon's Dragon");
}
