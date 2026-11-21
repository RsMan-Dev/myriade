//! `mob-zone-bench rings SPACING R [TICKS]`, `mob-zone-bench mixed SPACING R PLAYERS [TICKS]` (`HALF` the slab, `HORIZON_DAB=1`
//! turns on the upstream's dynamic activation of the brain): the `Mob` herd of the game's pass over a level with real tickets
//! (`ticket_levels`, view = simulation = R): every mob is in the snapshot and the grid, the ones whose chunk does not tick are
//! skipped by the `entity_ticking` test, the others get the nearest player (any distance) and are ticked.
//! Reports instructions (`instructions:u`, one thread) per mob of each ring: the near ring (a player within 32 blocks), the
//! rest of the zone, the skipped ones (their share of the snapshot, the grid and the test), and the tickets' update.
use horizon_client_lib::{
    health::TestLevel,
    mobs::{self, Mob, MobEvent, MobWorld, PendingPushes, PlayerView, Seen},
    ticket_levels::{ChunkTickets, Sources},
};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::profile::Counter;

fn half() -> i32 {
    std::env::var("HALF").ok().and_then(|v| v.parse().ok()).unwrap_or(200)
}
fn counter() -> &'static Counter {
    static C: std::sync::OnceLock<Counter> = std::sync::OnceLock::new();
    C.get_or_init(Counter::new)
}
fn grid(spacing: f64, keep: impl Fn(f64, f64) -> bool) -> Vec<(f64, f64)> {
    let side = (f64::from(2 * half() - 4) / spacing) as i32;
    (0..side * side)
        .map(|i| (-f64::from(half()) + 2.0 + f64::from(i % side) * spacing + 0.5, -f64::from(half()) + 2.0 + f64::from(i / side) * spacing + 0.5))
        .filter(|p| keep(p.0, p.1))
        .collect()
}

#[derive(Default, Clone, Copy)]
struct Acc {
    instr: u64,
    n: u64,
}
impl Acc {
    fn per(&self) -> f64 {
        self.instr as f64 / self.n.max(1) as f64
    }
}

fn run(label: &str, at: Vec<(f64, f64)>, players: Vec<(f64, f64)>, walk: bool, radius: i32, ticks: usize, warm: usize) {
    let (level, _s, kind) = horizon_ecs_proto::cow::build_level(half(), false);
    let w: &MobWorld = level.mw.0;
    let items = w.items.expect("items");
    let mut mobs: Vec<Mob> = at
        .iter()
        .enumerate()
        .map(|(i, &(x, z))| {
            let mut m = Mob::new(i as u32 + 1, kind.clone(), Vec3::new(x, 1.0, z), ((i * 37) % 360) as f32);
            m.persistent = true;
            m
        })
        .collect();
    let n = mobs.len();
    let mut test_level = TestLevel::new(7);
    let mut pushes = PendingPushes::default();
    let mut tickets = ChunkTickets::default();
    let mut cur = players;
    let mut seed = 0x9e37_79b9_u64;
    let dirs: Vec<(f64, f64)> = cur
        .iter()
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let a = (seed >> 11) as f64 / (1u64 << 53) as f64 * std::f64::consts::TAU;
            (a.cos() * 0.2, a.sin() * 0.2)
        })
        .collect();
    let chunk = |v: f64| (v / 16.0).floor() as i32;
    let c = counter();
    let (mut near, mut rest, mut test, mut snap, mut upd) = (Acc::default(), Acc::default(), Acc::default(), Acc::default(), Acc::default());
    let mut skipped = 0u64;
    for t in 0..(ticks + warm) {
        let measured = t >= warm;
        if walk {
            for (p, d) in cur.iter_mut().zip(&dirs) {
                p.0 = (p.0 + d.0).clamp(-f64::from(half()) + 3.0, f64::from(half()) - 3.0);
                p.1 = (p.1 + d.1).clamp(-f64::from(half()) + 3.0, f64::from(half()) - 3.0);
            }
        }
        // The tickets: updated when a player changes chunk (the game submits them every tick; the call returns early when nothing moved).
        let c0 = c.read();
        tickets.update(Some(Sources { players: cur.iter().map(|p| (chunk(p.0), chunk(p.1))).collect(), view_distance: radius, simulation_distance: radius, ..Default::default() }));
        let levels = tickets.levels();
        if measured {
            upd.instr += c.read() - c0;
            upd.n += 1;
        }
        // The snapshot and the grid of every mob.
        let c0 = c.read();
        let mut snapshot: Vec<Seen> = mobs.iter().map(|m| m.seen(None, items)).collect();
        let mut grid = mobs::seen_grid::SeenGrid::new(snapshot.iter().map(|s| s.bounds));
        if measured {
            snap.instr += c.read() - c0;
            snap.n += n as u64;
        }
        let mut events: Vec<MobEvent> = Vec::new();
        let mut level_t = test_level.level();
        // The test of each mob (as the game's pass does before its tick), as one block.
        let c0 = c.read();
        let ticks_now: Vec<bool> = mobs.iter().map(|m| levels.entity_ticking_at(m.position.x.floor() as i32, m.position.z.floor() as i32)).collect();
        if measured {
            test.instr += c.read() - c0;
            test.n += n as u64;
            skipped += ticks_now.iter().filter(|&&b| !b).count() as u64;
        }
        for slot in 0..mobs.len() {
            if !ticks_now[slot] {
                continue;
            }
            let c0 = c.read();
            mobs::apply_mob_pushes(&mut mobs[slot], &mut pushes);
            let from = events.len();
            let p = mobs[slot].position;
            let nearest = cur.iter().map(|q| ((p.x - q.0).powi(2) + (p.z - q.1).powi(2), q)).min_by(|a, b| a.0.total_cmp(&b.0));
            let (near_pos, is_near) = match nearest {
                Some((d2, q)) => (Some(Vec3::new(q.0, 1.0, q.1)), d2 < 32.0 * 32.0),
                None => (None, false),
            };
            let view = MobWorld {
                others: &snapshot,
                others_grid: Some(&grid),
                others_solid: Some(&[]),
                nearest_player: near_pos,
                player: near_pos.map(|q| player_view(q)),
                game_time: t as i64 + 1,
                ..w.clone()
            };
            let kept = mobs::ai::tick(&mut mobs[slot], &view, &mut level_t, &mut events);
            mobs::take_mob_pushes(&mut events, from, &mut pushes);
            let mut s = mobs[slot].seen(None, items);
            s.alive &= kept;
            s.removed |= !kept;
            grid.update(slot, s.bounds);
            snapshot[slot] = s;
            if measured {
                let d = c.read() - c0;
                let a = if is_near { &mut near } else { &mut rest };
                a.instr += d;
                a.n += 1;
            }
        }
        std::hint::black_box(&events);
    }
    let tk = ticks as f64;
    let dab = std::env::var_os("HORIZON_DAB").is_some();
    let ticked = near.n + rest.n;
    let total = (near.instr + rest.instr + test.instr + snap.instr + upd.instr) as f64 / tk;
    println!(
        "{label:<26}{} mobs={n:>6} ticked/tick={:>7.0} skipped/tick={:>7.0} | per mob: near {:>8.0} (x{:>5.0}/tick) rest-of-zone {:>8.0} (x{:>6.0}/tick) | all mobs: snapshot+grid {:>6.0}, test {:>4.1} | tickets {:>8.0}/tick | total {:>12.0}/tick",
        if dab { " [DAB]" } else { "" },
        ticked as f64 / tk,
        skipped as f64 / tk,
        near.per(),
        near.n as f64 / tk,
        rest.per(),
        rest.n as f64 / tk,
        snap.per(),
        test.per(),
        upd.instr as f64 / tk,
        total
    );
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mode = a.get(1).map_or("rings", String::as_str);
    let spacing: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(6.0);
    let r: i32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(6);
    match mode {
        "rings" => {
            let ticks: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(20);
            let p = (0.0, 0.0);
            let d = |x: f64, z: f64| ((x - p.0).powi(2) + (z - p.1).powi(2)).sqrt();
            let edge = f64::from(r) * 16.0;
            run("near (< 30 blocks)", grid(spacing, |x, z| d(x, z) < 30.0), vec![p], false, r, ticks, 60);
            run("mid (38 .. zone-20)", grid(spacing, |x, z| d(x, z) > 38.0 && x.abs().max(z.abs()) < edge - 20.0), vec![p], false, r, ticks, 60);
            run("far (outside the zone)", grid(spacing, |x, z| x.abs().max(z.abs()) > edge + 40.0), vec![p], false, r, ticks, 60);
        }
        "mixed" => {
            let players: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(4);
            let ticks: usize = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(15);
            let ps: Vec<(f64, f64)> = (0..players).map(|i| (-150.0 + 300.0 * ((i * 7 % 10) as f64 / 10.0), -150.0 + 300.0 * ((i * 3 % 10) as f64 / 10.0))).collect();
            run("mixed, walking players", grid(spacing, |_, _| true), ps, true, r, ticks, 40);
        }
        _ => eprintln!("rings | mixed"),
    }
}

fn player_view(at: Vec3) -> PlayerView {
    PlayerView {
        position: at,
        bounds: Aabb::new(Vec3::new(at.x - 0.3, at.y, at.z - 0.3), Vec3::new(at.x + 0.3, at.y + 1.8, at.z + 0.3)),
        eye: [at.x, at.y + 1.62, at.z],
        alive: true,
        invulnerable: false,
        creative: false,
        sneaking: false,
        swimming: false,
        look: [0.0; 3],
        main_hand: None,
        off_hand: None,
        head: None,
        chest: None,
        legs: None,
        feet: None,
        armor_cover: 0.0,
        invisible: false,
        spectator: false,
        gold_armor: false,
        hurt_by: None,
        last_hurt: None,
        velocity: Vec3::ZERO,
        yaw: 0.0,
        sprinting: false,
        riding: false,
        hero: false,
        effects: Vec::new(),
        uuid: [0; 4],
        allies: Default::default(),
        remote: false,
        sleeping: false,
        sleep_timer: 0,
        passenger: false,
        riding_vehicle: None,
        vehicle: None,
    }
}
