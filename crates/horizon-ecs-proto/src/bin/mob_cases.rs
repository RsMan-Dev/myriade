//! The single-cow hazard cases of `cow::tests::falls_fire_lava_snow_and_cactus_hurt_a_cow`, run on Horizon's `Mob`
//! (`HORIZON_TEST_RESOURCES` set): the health (or "dead") of each cow at the ticks 40, 140 and 340.
use horizon_client_lib::{
    health::TestLevel,
    mobs::{self, Mob, MobEvent, MobWorld, PendingPushes, Seen},
};
use horizon_core::geometry::Vec3;

fn main() {
    let jump = std::env::var("JUMP").is_ok();
    let walk = std::env::var("WALK").is_ok();
    let half = 56;
    let (level, _species, kind) = horizon_ecs_proto::cow::build_level(half, true);
    let w: &MobWorld = level.mw.0;
    let items = w.items.expect("items");
    let cases: [(&str, Vec3); 8] = if jump {
        [("jump", Vec3::new(-19.0, 1.0, -19.0)); 8]
    } else if walk {
        // A cow walks east into the cactus patch at x = -40 (goals cleared: no panic).
        [("walk", Vec3::new(-53.0, 1.0, -20.0)); 8]
    } else {
        [
        ("fall 11", Vec3::new(5.5, 12.0, 5.5)),
        ("fall 39", Vec3::new(8.5, 40.0, 5.5)),
        ("lava", Vec3::new(-47.0, 1.0, -47.0)),
        ("fire", Vec3::new(-47.0, 1.0, -39.0)),
        ("powder snow", Vec3::new(-31.0, 1.0, -47.0)),
        ("cactus", Vec3::new(-39.0, 1.0, -39.0)),
        ("pond", Vec3::new(-39.0, 3.0, -47.0)),
        ("in wall", Vec3::new(20.5, -0.6, 20.5)),
        ]
    };
    let mut mobs: Vec<Mob> = cases
        .iter()
        .enumerate()
        .map(|(i, (_, at))| {
            let mut m = Mob::new(i as u32 + 1, kind.clone(), *at, 0.0);
            m.persistent = true;
            m.ai.no_action_time = 100;
            if walk || jump {
                m.ai.goals.clear();
            }
            if i == 6 {
                // The goals of a cow keep it afloat (FloatGoal); the ECS cow has none yet.
                m.ai.goals.clear();
            }
            m
        })
        .collect();
    if walk || jump {
        // One cow only: the others would stand in it and push it.
        mobs.truncate(1);
    }
    if jump {
        for y in 0..5 {
            println!("block (-19,{y},-19): {}", w.facts.get(w.world.block(-19, y, -19)).name);
        }
    }
    let mut test_level = TestLevel::new(7);
    let mut pushes = PendingPushes::default();
    let mut ys: Vec<String> = Vec::new();
    for t in 1..=340 {
        let mut snapshot: Vec<Seen> = mobs.iter().map(|m| m.seen(None, items)).collect();
        let mut grid = mobs::seen_grid::SeenGrid::new(snapshot.iter().map(|s| s.bounds));
        let mut events: Vec<MobEvent> = Vec::new();
        let mut lvl = test_level.level();
        if jump {
            mobs[0].ai.jump = true;
        }
        if walk {
            let speed: f64 = std::env::var("SPEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
            mobs[0].ai.mov.set_wanted(-5.0, 1.0, -20.0, speed);
        }
        for slot in 0..mobs.len() {
            mobs::apply_mob_pushes(&mut mobs[slot], &mut pushes);
            let from = events.len();
            let view = MobWorld { others: &snapshot, others_grid: Some(&grid), others_solid: Some(&[]), nearest_player: None, ..w.clone() };
            let kept = mobs::ai::tick(&mut mobs[slot], &view, &mut lvl, &mut events);
            mobs::take_mob_pushes(&mut events, from, &mut pushes);
            let mut s = mobs[slot].seen(None, items);
            s.alive &= kept;
            s.removed |= !kept;
            grid.update(slot, s.bounds);
            snapshot[slot] = s;
        }
        if walk && [10, 20, 30, 40, 60, 80, 100].contains(&t) {
            let c = &mobs[0];
            println!("mob walk t={t:3} x={:.4} step={:.4} vel.x={:.4}", c.position.x, c.position.x - c.previous.x, c.velocity.x);
        }
        if std::env::var("TRACE").is_ok() && t <= 40 {
            let c = &mobs[5];
            println!(
                "mob cactus t={t:3} pos=({:.2},{:.2},{:.2}) health={} goals={:?} path={}",
                c.position.x, c.position.y, c.position.z, c.health, c.ai.goals.running_names(), c.ai.nav.path.is_some()
            );
        }
        if jump {
            ys.push(format!("{:.2}@({:.2},{:.2})", mobs[0].position.y, mobs[0].position.x, mobs[0].position.z));
            if t == 40 {
                println!("mob jump y: {}", ys.join(" "));
            }
        }
        if !walk && !jump && [40, 140, 340].contains(&t) {
            let line: Vec<String> = cases
                .iter()
                .zip(&mobs)
                .map(|((name, _), m)| {
                    if m.health <= 0.0 || m.removed { format!("{name}: dead") } else { format!("{name}: {}", m.health) }
                })
                .collect();
            println!("tick {t}: {}", line.join(", "));
        }
    }
}
