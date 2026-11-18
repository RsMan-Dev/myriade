//! The in-process bench: N cows as `Mob`s or as ECS cows, in one walled flat world, ticked T times with no
//! client, no network tracking and no player (`BENCH=mob|ecs BENCH_N=2000 BENCH_TICKS=300`, under
//! `perf stat -e instructions:u`; the set-up is the same for both and cancels between two tick counts).
use super::{cowworld::CowWorld, plain::PlainWorld, tests::mob_pass};
use crate::{
    health::TestLevel,
    mobs::{self, Mob, PendingPushes},
};
use horizon_core::geometry::Vec3;
use std::sync::Arc;

fn env(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
#[ignore = "bench"]
fn bench_cows() {
    let which = std::env::var("BENCH").unwrap_or_else(|_| "mob".into());
    let (n, ticks) = (env("BENCH_N", 2000), env("BENCH_TICKS", 100) as u32);
    // A square of cows two blocks apart (the server bench's grid is as dense), walled in.
    let side = (n as f64).sqrt().ceil() as i32 + 2;
    let half = side + 2;
    let chunks = (half * 2 / 16) + 2;
    let build = move |world: &mut crate::world::World, registry: &horizon_world::registry::BlockRegistry| {
        for cx in -chunks..chunks {
            for cz in -chunks..chunks {
                world.chunks.insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
            }
        }
        let stone = registry.default_state("minecraft:stone").unwrap();
        let lim = chunks * 16;
        for x in -lim..lim {
            for z in -lim..lim {
                world.set(x, 0, z, stone).unwrap();
                if x == -lim || x == lim - 1 || z == -lim || z == lim - 1 {
                    world.set(x, 1, z, stone).unwrap();
                    world.set(x, 2, z, stone).unwrap();
                }
            }
        }
    };
    let ran = mobs::world::with_built_test_world(&[], build, |w, types| {
        let items = w.items.expect("items");
        let kind = Arc::new(types.types["minecraft:cow"].clone());
        let cows: Vec<Mob> = (0..n)
            .map(|i| {
                let (cx, cz) = ((i as i32 % side) as f64, (i as i32 / side) as f64);
                let at = Vec3::new(-f64::from(side) + cx * 2.0 + 0.5, 1.0, -f64::from(side) + cz * 2.0 + 0.5);
                let mut m = Mob::new(i as u32 + 1, kind.clone(), at, (i * 37 % 360) as f32);
                m.persistent = true;
                m
            })
            .collect();
        let mut level = TestLevel::new(7);
        match which.as_str() {
            "mob" | "mob_rest" => {
                let mut cows = cows;
                for m in &mut cows {
                    m.memo.0.enabled = which == "mob_rest";
                }
                let mut pushes = PendingPushes::default();
                for _ in 0..ticks {
                    let events = mob_pass(&mut cows, w, &mut level.level(), &mut pushes, items);
                    std::hint::black_box(&events);
                }
                eprintln!("bench mob {n} cows {ticks} ticks, {} left", cows.len());
            }
            "plain" | "plain_rest" => {
                let mut plain = PlainWorld::new();
                plain.rest = which == "plain_rest";
                for m in cows {
                    plain.spawn_from(m);
                }
                for _ in 0..ticks {
                    let events = plain.pass(w, &mut level.level());
                    std::hint::black_box(&events);
                }
                eprintln!("bench plain {n} cows {ticks} ticks, {} left", plain.len());
            }
            _ => {
                let mut ecs = CowWorld::new();
                ecs.rest = which == "ecs_rest";
                for m in cows {
                    ecs.spawn_from(m);
                }
                for _ in 0..ticks {
                    let events = ecs.pass(w, &mut level.level());
                    std::hint::black_box(&events);
                }
                eprintln!("bench ecs {n} cows {ticks} ticks, {} left", ecs.len());
            }
        }
    });
    assert!(ran, "no resources");
}
