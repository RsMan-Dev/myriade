//! Writes the golden files of the ECS port (`horizon-ecs-proto/tests/it/dragon_golden/`) from the real `Dragon`.
//! Test-only: it changes no behaviour. Run with `HORIZON_DRAGON_GOLDEN=<dir>` (otherwise it only checks it can run).
use super::*;
use crate::game::*;
use horizon_client::chunk::Chunk;
use std::fmt::Write as _;

const GROUND: i32 = 64;
const SEED: i64 = 5;
const TICKS: usize = 300;
const PAIRS: [(usize, usize, i32); 6] = [(0, 5, 1), (12, 20, 1), (3, 15, 1), (18, 22, 1), (11, 0, 0), (13, 23, 0)];
const PROBES: [(f64, f64, f64, i32); 4] = [(0.5, 80.0, 0.5, 1), (58.0, 70.0, 6.0, 1), (-12.0, 66.0, 25.0, 0), (30.0, 90.0, -30.0, 1)];

fn flat_game(test: impl FnOnce(&mut Game<'_>)) {
    let root = PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
    let settings = Settings::default();
    let loaded = load_resources(
        horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
        Vec::new(),
        None,
        &root,
        &settings,
        Instant::now(),
        false,
        execution_role::ExecutionRole::IntegratedServer,
        &Progress::new(),
        &mut |_, _| unreachable!(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.execution_role = execution_role::ExecutionRole::IntegratedServer;
    let mut g = Game::new(
        &loaded.resources,
        loaded.pack,
        loaded.gui.unwrap(),
        options,
        settings,
        Audio::silent(&horizon_assets::resource_source::ResourceSource::directory(root.clone())),
        None,
    )
    .unwrap();
    g.options.dedicated = Some(Vec::new());
    g.world = World::new(g.res.facts().air, 1);
    for x in -6..=6 {
        for z in -6..=6 {
            g.world.chunks.insert((x, z), Arc::new(Chunk::empty(x, z)));
        }
    }
    let stone = g.pack.assets.registry.default_state("minecraft:stone").unwrap();
    for x in -90..=90 {
        for z in -90..=90 {
            g.world.set(x, GROUND - 1, z, stone).unwrap();
        }
    }
    test(&mut g);
}

fn bits(v: Vec3) -> String {
    format!("{:016x} {:016x} {:016x}", v.x.to_bits(), v.y.to_bits(), v.z.to_bits())
}

fn lines_l1() -> String {
    let mut out = String::from("# index px py pz x0 y0 z0 x1 y1 z1 (Dragon::part_box)\n");
    let mut d = Dragon::new(Vec3::ZERO, 0.0, 1);
    for (k, p) in [(0.0, 0.0, 0.0), (3.25, 70.5, -8.125), (-40.0, 100.0, 17.75)].into_iter().enumerate() {
        for i in 0..8 {
            d.parts[i].0 = Vec3::new(p.0 + i as f64 * 0.5, p.1 + k as f64, p.2 - i as f64 * 0.25);
            let (pos, b) = d.part_geometry(i);
            let (lo, hi) = (b.min_position(), b.max_position());
            let _ = writeln!(out, "{i} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}", pos.x, pos.y, pos.z, lo.x, lo.y, lo.z, hi.x, hi.y, hi.z);
        }
    }
    out
}

fn lines_flight(g: &mut Game<'_>) -> String {
    let mut out = String::new();
    g.dragon.replace(Dragon::new(Vec3::new(0.5, 80.0, 0.5), 0.0, SEED));
    for (i, n) in g.dragon_nodes().iter().enumerate() {
        let _ = writeln!(out, "node {i} {} {} {}", n[0], n[1], n[2]);
    }
    for (a, b, c) in PAIRS {
        g.fight.alive_crystals = c;
        let line = match g.dragon_path(a, b, None) {
            Some(p) => p.nodes.iter().map(|n| format!("{}/{}/{}", n[0], n[1], n[2])).collect::<Vec<_>>().join(";"),
            None => "none".into(),
        };
        let _ = writeln!(out, "path {a} {b} {c} {line}");
    }
    for (x, y, z, c) in PROBES {
        g.fight.alive_crystals = c;
        let _ = writeln!(out, "closest {c} {x} {y} {z} {}", g.closest_node(x, y, z));
    }
    g.fight.alive_crystals = 0;
    let mut d = Dragon::new(Vec3::new(0.5, 80.0, 0.5), 0.0, SEED);
    d.phase = Phase::Holding { path: None, target: None, clockwise: false };
    d.id = 900;
    g.dragon.replace(d);
    for t in 0..TICKS {
        g.tick_dragon();
        let d = g.dragon.as_ref().unwrap();
        let _ = writeln!(out, "tick {t} {} {:08x} {}", bits(d.position), d.yaw.to_bits(), d.phase.id());
    }
    out
}

#[test]
#[ignore = "daily"]
fn write_dragon_goldens() {
    flat_game(|g| {
        let (l1, flight) = (lines_l1(), lines_flight(g));
        if let Some(dir) = std::env::var_os("HORIZON_DRAGON_GOLDEN") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("L1.txt"), l1).unwrap();
            std::fs::write(dir.join("flight.txt"), flight).unwrap();
        }
    });
}
