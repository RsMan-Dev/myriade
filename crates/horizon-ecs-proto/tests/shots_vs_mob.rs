//! The projectile socle (hurting projectiles, a rate-of-fire shooting module) against the `Mob`'s own `Projectile::tick` and goals
//! (`HORIZON_TEST_RESOURCES` set): the flight of a small fireball, a large fireball and a spit tick by tick (same position and velocity,
//! gone at the same tick), then the shots of a ghast (one every 60 ticks, from the same place along the same line) and of a blaze (three per
//! 178 ticks) at a player standing in the open. `HORIZON_SPECIES` is not read: the species are taken by name.
mod common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::{
    arrows,
    mobs::{MobEvent, Who as MWho},
    projectiles::{Hit, Projectile, ProjectileKind, ProjectileWorld},
};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{
    behaviours::{burst_shot, charged_shot, Entry},
    cow::*,
    goals::*,
    projectile::{launch, Flight, Projectile as EProjectile},
    species::Config,
    targeting::{set_target, Target, Who},
};
use std::collections::HashMap;

struct Open<'a>(&'a MobRef);
impl ProjectileWorld for Open<'_> {
    fn clip(&self, from: Vec3, to: Vec3) -> Option<(Vec3, [i32; 3], [i32; 3])> {
        let mw = self.0.w;
        let movement = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z);
        arrows::clip_block_hit(mw.world, mw.facts, mw.collision, from, movement).map(|(at, p, n)| (at, [p.x, p.y, p.z], n))
    }
    fn in_water(&self, _: Aabb) -> bool {
        false
    }
    fn targets(&self) -> &[(MWho, Aabb)] {
        &[]
    }
}
fn close(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9 && (a.z - b.z).abs() < 1e-9
}

/// The same start and velocity: the same position every tick and the same tick of the end, on the ground 20 blocks away.
#[test]
fn the_hurting_and_thrown_kinds_fly_as_the_mob_s_do() {
    if !have_data() {
        return;
    }
    let kinds: [(&str, ProjectileKind, Flight, Vec3); 3] = [
        ("small fireball", ProjectileKind::SmallFireball, Flight::SMALL_FIREBALL, Vec3::new(0.02, -0.01, 0.1)),
        ("large fireball", ProjectileKind::LargeFireball { power: 1 }, Flight::large_fireball(1), Vec3::new(-0.03, -0.012, 0.1)),
        ("spit", ProjectileKind::LlamaSpit, Flight::SPIT, Vec3::new(0.2, 0.3, 0.5)),
    ];
    for (name, kind, flight, v) in kinds {
        let (r, mut h) = flat_pair();
        let at = Vec3::new(0.5, 3.0, 0.5);
        let mut m = Projectile::new(kind, at, None);
        m.velocity = v;
        let e = launch(&mut h.world, flight, at, v, None);
        let world = Open(&r);
        let mut gone = None;
        for t in 1..=400i64 {
            let (keep, hit) = m.tick(&world);
            h.tick();
            if !keep || !matches!(hit, Hit::None) {
                gone = Some(t);
                assert!(h.world.get_entity(e).is_err(), "{name}: gone together at tick {t}");
                break;
            }
            let p = h.world.get::<Pos>(e).unwrap().0;
            assert!(close(p, m.position), "{name} tick {t}: ecs {p:?} mob {:?}", m.position);
        }
        eprintln!("{name}: gone at tick {gone:?} in both");
        assert!(gone.is_some(), "{name} never ended");
    }
}

fn shots_of_mob(kind: &str, at: Vec3, player: Vec3, ticks: usize) -> Vec<(usize, Vec3, Vec3)> {
    let mut r = MobRef::new(24, false);
    let i = r.add_kind(at, mob_kind(kind));
    r.mobs[i].ai.target = Some(MWho::Player);
    let view = player_view(player, None);
    let mut shots = Vec::new();
    for t in 0..ticks {
        let before = r.events.len();
        r.step(Some(&view));
        for e in &r.events[before..] {
            if let MobEvent::Projectile(p) = e {
                shots.push((t, p.position, p.velocity));
            }
        }
        // The creature is held where it is: its own wanderings are not what is compared.
        r.mobs[i].position = at;
        r.mobs[i].velocity = Vec3::ZERO;
        r.mobs[i].yaw = 0.0;
        r.mobs[i].body_yaw = 0.0;
    }
    shots
}
fn shots_of_ecs(name: &str, entry: Entry, at: Vec3, player: Vec3, ticks: usize) -> Vec<(usize, Vec3, Vec3)> {
    let mut h = Herd::with_slab(24, false, false);
    set_lazy_look(&mut h.world, false);
    h.set_online(&[(player.x, player.z)]);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == name).unwrap();
    sp.goals = Config { entries: Box::leak(Box::new([entry])) };
    let e = spawn_species(&mut h.world, &sp, at.x, at.z, 99);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    h.tick();
    teleport(&mut h.world, e, at);
    h.world.get_mut::<Yaw>(e).unwrap().0 = 0.0;
    set_target(&mut h.world, e, Target::new(Who::Player(1), 200, u8::MAX));
    let mut first: HashMap<Entity, usize> = HashMap::new();
    let mut shots = Vec::new();
    for t in 0..ticks {
        h.tick();
        let found: Vec<(Entity, Vec3, Vec3)> = h.world.query::<(Entity, &Pos, &Vel, &EProjectile)>().iter(&h.world).map(|(e, p, v, _)| (e, p.0, v.0)).collect();
        for (id, p, v) in found {
            if first.insert(id, t).is_none() {
                shots.push((t, p, v));
            }
        }
        teleport(&mut h.world, e, at);
        h.world.get_mut::<Yaw>(e).unwrap().0 = 0.0;
    }
    shots
}
fn gaps(v: &[(usize, Vec3, Vec3)]) -> Vec<usize> {
    v.windows(2).map(|w| w[1].0 - w[0].0).collect()
}

/// A ghast 6 blocks up, a player 28 blocks away: a fireball every 60 ticks from 4 blocks ahead of it along the line to the player's
/// middle. The ECS one is seen a tick later or not (the system order): its start, moved by one tick of flight or not, is the Mob's.
#[test]
fn a_ghast_shoots_as_the_mob_s_does() {
    if !have_data() {
        return;
    }
    let (at, player) = (Vec3::new(0.5, 6.0, 0.5), Vec3::new(0.5, 1.0, 28.5));
    let mob = shots_of_mob("minecraft:ghast", at, player, 400);
    let ecs = shots_of_ecs("minecraft:ghast", Entry::new(&charged_shot::MODULE, 7, &charged_shot::Params::GHAST), at, player, 400);
    eprintln!("ghast: mob {:?} ecs {:?}", mob.iter().map(|s| s.0).collect::<Vec<_>>(), ecs.iter().map(|s| s.0).collect::<Vec<_>>());
    assert!(mob.len() >= 5 && ecs.len() >= 5);
    assert!(gaps(&mob).iter().all(|&g| g == 60) && gaps(&ecs).iter().all(|&g| g == 60), "mob {:?} ecs {:?}", gaps(&mob), gaps(&ecs));
    let (m, e) = (&mob[0], &ecs[0]);
    // The ECS projectile is read after its first tick of flight or before: compare with the Mob's start moved 0 or 1 tick.
    let mut p = Projectile::new(ProjectileKind::LargeFireball { power: 1 }, m.1, None);
    p.velocity = m.2;
    let r = MobRef::new(24, false);
    let w = Open(&r);
    let same_start = close(e.1, m.1) && close(e.2, m.2);
    let _ = p.tick(&w);
    let one_tick = close(e.1, p.position) && close(e.2, p.velocity);
    assert!(same_start || one_tick, "mob start {:?} v {:?}; ecs {:?} v {:?}", m.1, m.2, e.1, e.2);
    let offset = ecs[0].0 as i64 - mob[0].0 as i64;
    eprintln!("first shot: ecs - mob = {offset} ticks");
    assert!(offset.abs() <= 2, "{offset}");
}

/// A blaze 6 blocks up, a player 20 blocks away: bursts of three small fireballs 6 ticks apart, a burst every 178 ticks.
#[test]
fn a_blaze_shoots_in_bursts_as_the_mob_s_does() {
    if !have_data() {
        return;
    }
    let (at, player) = (Vec3::new(0.5, 1.0, 0.5), Vec3::new(0.5, 1.0, 20.5));
    let mob = shots_of_mob("minecraft:blaze", at, player, 5400);
    let ecs = shots_of_ecs("minecraft:blaze", Entry::new(&burst_shot::MODULE, 4, &burst_shot::Params::BLAZE), at, player, 5400);
    eprintln!("blaze: {} / {} shots, first ticks mob {:?} ecs {:?}", mob.len(), ecs.len(), mob.iter().take(4).map(|s| s.0).collect::<Vec<_>>(), ecs.iter().take(4).map(|s| s.0).collect::<Vec<_>>());
    assert!(mob.len() >= 60 && ecs.len() >= 60);
    assert_eq!(gaps(&mob)[..58], gaps(&ecs)[..58], "the same rhythm");
    // The aim: the mean direction is the line to the player's middle, the spread grows as the fourth root of the distance.
    let law = |s: &[(usize, Vec3, Vec3)]| {
        let n = s.len() as f64;
        let ux: Vec<f64> = s.iter().map(|x| x.2.x / (x.2.x.powi(2) + x.2.y.powi(2) + x.2.z.powi(2)).sqrt()).collect();
        let mean = ux.iter().sum::<f64>() / n;
        let sd = (ux.iter().map(|u| (u - mean).powi(2)).sum::<f64>() / n).sqrt();
        (mean, sd)
    };
    let (lm, le) = (law(&mob), law(&ecs));
    eprintln!("lateral direction (mean, sd): mob {lm:?} ecs {le:?}");
    assert!(lm.0.abs() < 0.05 && le.0.abs() < 0.05 && (lm.1 / le.1 - 1.0).abs() < 0.5, "{lm:?} {le:?}");
}
