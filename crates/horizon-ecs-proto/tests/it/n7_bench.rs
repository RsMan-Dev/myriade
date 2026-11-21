//! Cost of the N7 pilot modules (`-- --ignored --nocapture`, `N7_N=1000`, one species per process): ns per creature-tick, idle (state absent)
//! and active (celebrating / counting down to a conversion / pregnant frog), for N creatures spread on a grid.
use crate::common;
use common::*;
use horizon_ecs_proto::cow::*;
use std::time::Instant;

fn run(n: usize, active: impl Fn(&mut Herd, bevy_ecs::entity::Entity), has: impl Fn(&Herd, bevy_ecs::entity::Entity) -> bool, label: &str) {
    let side = (n as f64).sqrt().ceil() as usize;
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    h.tick();
    // A still player every 16 blocks over the grid, so that every creature is near one and awake.
    let half = 18.0;
    let mut id = 0;
    let mut gx = -18.0;
    while gx < half {
        let mut gz = -18.0;
        while gz < half {
            h.world.resource_mut::<horizon_ecs_proto::goals::Online>().set(horizon_ecs_proto::goals::Player { id, x: gx, y: 1.0, z: gz, ..Default::default() });
            id += 1;
            gz += 16.0;
        }
        gx += 16.0;
    }
    let mut es = Vec::new();
    for k in 0..n {
        let (x, z) = (-18.0 + (k % side) as f64 + 0.5, -18.0 + (k / side) as f64 + 0.5);
        let e = ecs_cow(&mut h, x, 1.0, z, 7000 + k as u64);
        horizon_ecs_proto::despawn::keep(&mut h.world, e);
        es.push(e);
    }
    for t in 0..300 {
        h.tick();
        if std::env::var_os("N7_DBG").is_some() && t % 20 == 0 {
            eprintln!("t {t} alive {}", es.iter().filter(|e| h.world.get_entity(**e).is_ok()).count());
        }
    }
    let t0 = Instant::now();
    for _ in 0..100 {
        h.tick();
    }
    let idle = t0.elapsed().as_nanos() as f64 / (100.0 * n as f64);
    es.retain(|e| h.world.get_entity(*e).is_ok());
    let alive = es.len();
    for &e in &es {
        active(&mut h, e);
    }
    for _ in 0..5 {
        h.tick();
    }
    let t1 = Instant::now();
    for _ in 0..150 {
        h.tick();
    }
    let act = t1.elapsed().as_nanos() as f64 / (150.0 * n as f64);
    let awake = es.iter().filter(|e| h.world.get_entity(**e).is_ok() && has(&h, **e)).count();
    eprintln!("N7 {label} n={n} idle {idle:.1} ns/creature-tick, active {act:.1} ns/creature-tick (alive {alive}, still in the state at the end: {awake})");
}

#[test]
#[ignore]
fn n7_bench() {
    if !have_data() {
        return;
    }
    let n: usize = std::env::var("N7_N").ok().and_then(|v| v.parse().ok()).unwrap_or(1000);
    match horizon_ecs_proto::cow::species_var().as_deref() {
        Ok("minecraft:piglin") => {
            run(n, |h, e| horizon_ecs_proto::behaviours::celebrate::start_party(&mut h.world, e, horizon_ecs_proto::behaviours::celebrate::Kill { victim: e, killer: None, species: 0, at: [0.0; 3] }), |h, e| h.world.get::<horizon_ecs_proto::behaviours::celebrate::Celebrating>(e).is_some(), "celebrate");
            run(
                n,
                |h, e| {
                    h.world.insert_resource(horizon_ecs_proto::goals::UnsafePlace(|_, _| true));
                    horizon_ecs_proto::goals::arm_module(&mut h.world, e, &horizon_ecs_proto::behaviours::convert_timer::MODULE, 1);
                },
                |h, e| h.world.get::<horizon_ecs_proto::behaviours::convert_timer::Converting>(e).is_some(),
                "convert_timer",
            );
        }
        Ok("minecraft:frog") => run(
            n,
            |h, e| {
                h.world.entity_mut(e).insert(horizon_ecs_proto::behaviours::breed::Carrying);
                horizon_ecs_proto::goals::arm_module(&mut h.world, e, &horizon_ecs_proto::behaviours::lay_spawn::MODULE, 1);
            },
            |h, e| h.world.get::<horizon_ecs_proto::behaviours::breed::Carrying>(e).is_some(),
            "lay_spawn",
        ),
        _ => eprintln!("N7 : HORIZON_SPECIES=minecraft:piglin or minecraft:frog"),
    }
}
