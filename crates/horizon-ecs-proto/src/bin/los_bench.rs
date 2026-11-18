//! `los-bench [CALLS]`, `HORIZON_TEST_RESOURCES` set: what one `MobWorld::line_of_sight_for` (the `Mob`'s `sees`) costs, from a
//! zombie's eyes to a player's eyes 4 and 20 blocks away in the open, and 20 blocks away through a wall.
use horizon_core::geometry::Vec3;

fn main() {
    let calls: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(200_000);
    let (level, _s, _k) = horizon_ecs_proto::cow::build_level(40, false);
    let w = level.mw.0;
    let run = |label: &str, to: Vec3| {
        let t0 = std::time::Instant::now();
        let mut seen = 0usize;
        for i in 0..calls {
            let d = (i % 7) as f64 * 0.01;
            seen += usize::from(w.line_of_sight_for(Some(1.0), Vec3::new(-10.0 + d, 2.62, 0.5), Vec3::new(to.x + d, to.y, to.z)));
        }
        println!("{label}: {:.0} ns per call, seen {seen}/{calls}", t0.elapsed().as_secs_f64() * 1e9 / calls as f64);
    };
    run("4 blocks, open", Vec3::new(-6.0, 2.62, 0.5));
    run("20 blocks, open", Vec3::new(10.0, 2.62, 0.5));
    // The wall of the slab is at x = -40 and 40 (stone, 2 high): a target beyond it.
    run("through the wall", Vec3::new(-45.0, 2.62, 0.5));
}
