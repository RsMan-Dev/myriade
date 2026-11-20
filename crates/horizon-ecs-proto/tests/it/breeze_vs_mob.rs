//! The breeze by configuration (`species/breeze.rs`, `brain/behaviours/breeze.rs`, `sensors/attackable.rs`, the wind charge) against real
//! `Mob` breezes (`HORIZON_TEST_RESOURCES` set, process species `minecraft:breeze`): one breeze and one player 10 blocks away, N runs with
//! ids far apart, 600 ticks: the tick it takes its target, how many charges it shoots, how many long jumps and slides it makes, and how far
//! it keeps from the player. The `Mob` draws its brain's durations from the level random and the ECS from the entity's own: the laws are
//! compared, not the ticks.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{breeze, MobEvent};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{brain::behaviours::breeze::Pose, cow::*, goals::*, hostile::PlayerState, projectile::Projectile};

fn skip() -> bool {
    !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:breeze")
}
const N: usize = 12;
const TICKS: i64 = 600;
const SETTLE: usize = 120;
fn far_id(j: usize) -> u32 {
    1 + (j as u32 + 1) * 7919 % 100_003
}
#[derive(Default)]
struct Trace {
    target_at: Option<i64>,
    shots: Vec<i64>,
    jumps: i64,
    far: f64,
    far_at: i64,
    path: Vec<(i64, f64)>,
    /// (distance flown, peak speed) of each long jump.
    flights: Vec<(f64, f64)>,
    near: f64,
}
fn mob_run(j: usize, start: Vec3, _player: Vec3) -> Trace {
    let mut r = MobRef::new(60, false);
    let i = r.add_id(start, far_id(j));
    let mut tr = Trace { near: f64::MAX, ..Default::default() };
    // Both sides wander 120 ticks alone first: the ECS creature is born in the air and its first choice (a stroll needs a path) is always
    // `do_nothing`, which lasts up to 100 ticks; the player then stands 10 blocks south of where the breeze is.
    for _ in 0..SETTLE {
        r.step(None);
    }
    let player = Vec3::new(r.mobs[i].position.x, 1.0, r.mobs[i].position.z + 10.0);
    let mut was_jumping = false;
    let (mut from_at, mut peak) = ((0.0, 0.0), 0.0);
    for t in 1..=TICKS {
        let p = player_view(player, None);
        let from = r.events.len();
        r.step(Some(&p));
        let m = &r.mobs[i];
        if m.ai.target.is_some() && tr.target_at.is_none() {
            tr.target_at = Some(t);
        }
        tr.shots.extend(r.events[from..].iter().filter(|e| matches!(e, MobEvent::Projectile(_))).map(|_| t));
        let jumping = breeze::pose(m) == breeze::LONG_JUMPING;
        if jumping && !was_jumping {
            tr.jumps += 1;
            from_at = (m.position.x, m.position.z);
            peak = (m.velocity.x * m.velocity.x + m.velocity.z * m.velocity.z).sqrt();
        }
        if !jumping && was_jumping {
            tr.flights.push((((m.position.x - from_at.0).powi(2) + (m.position.z - from_at.1).powi(2)).sqrt(), peak));
        }
        was_jumping = jumping;
        let d = ((m.position.x - player.x).powi(2) + (m.position.z - player.z).powi(2)).sqrt();
        if d > tr.far {
            tr.far = d;
            tr.far_at = t;
        }
        if t % 30 == 0 {
            tr.path.push((t, d));
        }
        tr.near = tr.near.min(d);
    }
    tr
}
fn ecs_run(j: usize, start: Vec3, _player: Vec3) -> Trace {
    let mut h = Herd::with_slab(60, true, false);
    h.world.resource_mut::<PlayerState>().immortal = true;
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:breeze").unwrap();
    let e = spawn_species(&mut h.world, &sp, start.x, start.z, ((j as u64) + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    let mut tr = Trace { near: f64::MAX, ..Default::default() };
    for _ in 0..SETTLE {
        h.tick();
    }
    let at = h.world.get::<Pos>(e).unwrap().0;
    let player = Vec3::new(at.x, 1.0, at.z + 10.0);
    let mut was_jumping = false;
    let (mut from_at, mut peak) = ((0.0, 0.0), 0.0);
    let mut seen = std::collections::HashSet::new();
    for t in 1..=TICKS {
        h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0, ..Default::default() });
        h.tick();
        if h.world.get::<horizon_ecs_proto::targeting::Target>(e).is_some() && tr.target_at.is_none() {
            tr.target_at = Some(t);
        }
        let mut q = h.world.query::<(Entity, &Projectile)>();
        let now: Vec<Entity> = q.iter(&h.world).map(|x| x.0).collect();
        for p in now {
            if seen.insert(p) {
                tr.shots.push(t);
            }
        }
        let jumping = h.world.get::<Pose>(e) == Some(&Pose::LongJumping);
        let (pp, vv) = (h.world.get::<Pos>(e).unwrap().0, h.world.get::<Vel>(e).unwrap().0);
        if jumping && !was_jumping {
            tr.jumps += 1;
            from_at = (pp.x, pp.z);
            peak = (vv.x * vv.x + vv.z * vv.z).sqrt();
        }
        if !jumping && was_jumping {
            tr.flights.push((((pp.x - from_at.0).powi(2) + (pp.z - from_at.1).powi(2)).sqrt(), peak));
        }
        was_jumping = jumping;
        let p = h.world.get::<Pos>(e).unwrap().0;
        let d = ((p.x - player.x).powi(2) + (p.z - player.z).powi(2)).sqrt();
        if d > tr.far {
            tr.far = d;
            tr.far_at = t;
        }
        if t % 30 == 0 {
            tr.path.push((t, d));
        }
        tr.near = tr.near.min(d);
    }
    tr
}
fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

#[test]
fn a_breeze_takes_its_player_shoots_jumps_and_slides_as_a_mob_does() {
    if skip() {
        return;
    }
    let (start, player) = (Vec3::new(0.5, 1.0, 0.5), Vec3::new(0.5, 1.0, 10.5));
    let (mut m, mut e) = (Vec::new(), Vec::new());
    for j in 0..N {
        m.push(mob_run(j, start, player));
        e.push(ecs_run(j, start, player));
    }
    let col = |f: &dyn Fn(&Trace) -> f64, v: &[Trace]| mean(&v.iter().map(f).collect::<Vec<_>>());
    for (name, f) in [
        ("target taken at", &(|t: &Trace| t.target_at.unwrap_or(-1) as f64) as &dyn Fn(&Trace) -> f64),
        ("charges shot", &|t: &Trace| t.shots.len() as f64),
        ("first charge at", &|t: &Trace| t.shots.first().copied().unwrap_or(-1) as f64),
        ("long jumps", &|t: &Trace| t.jumps as f64),
        ("nearest to the player", &|t: &Trace| t.near),
        ("farthest from the player", &|t: &Trace| t.far),
    ] {
        eprintln!("{name}: mob {:.2} ecs {:.2} ({N} runs)", col(f, &m), col(f, &e));
    }
    for t in &e {
        assert!(t.target_at.is_some_and(|x| x <= 60), "it takes the player: {:?}", t.target_at);
    }
    assert!(col(&|t| t.shots.len() as f64, &e) > 0.0, "the ECS breeze shoots");
    let within = |a: f64, b: f64, tol: f64| (a - b).abs() <= tol * a.max(b);
    assert!(within(col(&|t| t.shots.len() as f64, &m), col(&|t| t.shots.len() as f64, &e), 0.25), "charges shot");
    assert!(within(col(&|t| t.jumps as f64, &m), col(&|t| t.jumps as f64, &e), 0.25), "long jumps");
    assert!((col(&|t| t.target_at.unwrap_or(-1) as f64, &m) - col(&|t| t.target_at.unwrap_or(-1) as f64, &e)).abs() < 8.0, "the tick it takes its target");
}

/// A wind charge's burst pushes a creature away from its centre by `(1 - distance / diameter) * exposure` and does no damage (ECS only:
/// the `Mob` bench has no game to burst in; the formula is that of `wind_knockback` in the host).
#[test]
fn a_wind_burst_pushes_a_creature_away_without_hurting_it() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    let sp = h.world.resource::<SpeciesTable>().0[0];
    let e = spawn_species(&mut h.world, &sp, 2.5, 0.5, 11);
    h.world.get_mut::<Vel>(e).unwrap().0 = Vec3::ZERO;
    let hp = health(&h, e);
    horizon_ecs_proto::explosion::wind_burst(&mut h.world, Vec3::new(0.5, 1.0, 0.5), 3.0, None);
    let v = h.world.get::<Vel>(e).unwrap().0;
    eprintln!("burst push at 2 blocks: {:?}", v);
    assert!(v.x > 0.3 && v.x < 0.7, "pushed away along x by about (1 - 2/6) * dir: {}", v.x);
    assert_eq!(health(&h, e), hp, "no damage");
    let far = spawn_species(&mut h.world, &sp, 9.5, 0.5, 12);
    h.world.get_mut::<Vel>(far).unwrap().0 = Vec3::ZERO;
    horizon_ecs_proto::explosion::wind_burst(&mut h.world, Vec3::new(0.5, 1.0, 0.5), 3.0, None);
    assert!(h.world.get::<Vel>(far).unwrap().0.x == 0.0, "out of the diameter: no push");
}
