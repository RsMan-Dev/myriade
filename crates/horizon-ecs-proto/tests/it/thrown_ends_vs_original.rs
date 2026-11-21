//! The ends of the thrown items of `projectile_more.rs` (`OnHit::Hatch`, `Teleport`, `Orbs`) against the original: each item flies in step with
//! `Mob`'s `Projectile::tick` (which gives the tick it ends and the position its last tick started at), and what it does at its end is the
//! jar's `onHit` (read with javap: `ThrownEgg`, `ThrownEnderpearl`, `ThrownExperienceBottle`), replayed with the same random source in the same
//! draw order. `HORIZON_TEST_RESOURCES` set.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::projectiles::{Projectile, ProjectileKind, ProjectileWorld};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::{cow::*, projectile::*, projectile_more::*};

struct Open<'a>(&'a MobRef);
impl ProjectileWorld for Open<'_> {
    fn clip(&self, from: Vec3, to: Vec3) -> Option<(Vec3, [i32; 3], [i32; 3])> {
        let mw = self.0.w;
        let movement = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z);
        horizon_client_lib::arrows::clip_block_hit(mw.world, mw.facts, mw.collision, from, movement).map(|(at, p, n)| (at, [p.x, p.y, p.z], n))
    }
    fn in_water(&self, _: Aabb) -> bool {
        false
    }
    fn targets(&self) -> &[(horizon_client_lib::mobs::Who, Aabb)] {
        &[]
    }
}

const SEED: i64 = 4242;

/// Throws `flight` (`kind` for the Mob) from the same start; runs both to the end; returns the Mob's `previous` at its last tick.
/// `before_last` runs just before the world's last tick.
fn throw(h: &mut Herd, r: &MobRef, kind: ProjectileKind, flight: Flight, owner: Option<horizon_ecs_proto::targeting::Who>, mut before_last: impl FnMut(&mut Herd)) -> Vec3 {
    let (at, v) = (Vec3::new(0.5, 1.9, 0.5), Vec3::new(0.3, -0.25, 0.1));
    let mut m = Projectile::new(kind, at, None);
    m.velocity = v;
    let e = launch(&mut h.world, flight, at, v, owner);
    let world = Open(r);
    for _ in 1..=200 {
        let before = m.position;
        let (keep, _) = m.tick(&world);
        if !keep {
            before_last(h);
        }
        h.tick();
        if !keep {
            assert!(h.world.get_entity(e).is_err(), "gone together");
            return before;
        }
    }
    panic!("never ended");
}

fn count(h: &mut Herd) -> usize {
    h.world.query::<&Pos>().iter(&h.world).count()
}

#[test]
fn a_bottle_gives_three_plus_two_draws_of_five_as_experience() {
    if !have_data() {
        return;
    }
    let mut reference = LegacyRandom::new(SEED);
    for i in 0..30 {
        let (r, mut h) = flat_pair();
        h.world.insert_resource(ThrownRng(LegacyRandom::new(SEED)));
        // The same source, advanced to the i-th bottle.
        let mut want = LegacyRandom::new(SEED);
        let _ = &mut reference;
        for _ in 0..i * 2 {
            want.next_int_bound(5).unwrap();
        }
        let (a, b) = (want.next_int_bound(5).unwrap(), want.next_int_bound(5).unwrap());
        {
            let mut rng = h.world.resource_mut::<ThrownRng>();
            for _ in 0..i * 2 {
                rng.0.next_int_bound(5).unwrap();
            }
        }
        let _ = h.drain_host_out();
        throw(&mut h, &r, ProjectileKind::ExperienceBottle, EXPERIENCE_BOTTLE, None, |_| {});
        let xp: Vec<u32> = h.drain_host_out().into_iter().filter_map(|e| if let HostEvent::Xp { amount, .. } = e { Some(amount) } else { None }).collect();
        assert_eq!(xp, vec![(3 + a + b) as u32], "bottle {i}");
    }
}

#[test]
fn an_egg_hatches_one_in_eight_and_then_four_one_in_thirty_two() {
    if !have_data() {
        return;
    }
    let egg = Flight { hit: egg_hit("minecraft:chicken"), ..EGG };
    let mut want = LegacyRandom::new(SEED);
    let (r, mut h) = flat_pair();
    h.world.insert_resource(ThrownRng(LegacyRandom::new(SEED)));
    let (mut expected, mut hatched_any, mut four) = (0usize, false, false);
    for i in 0..300 {
        let before = count(&mut h);
        throw(&mut h, &r, ProjectileKind::Egg, egg, None, |_| {});
        let n = if want.next_int_bound(8).unwrap() == 0 { if want.next_int_bound(32).unwrap() == 0 { 4 } else { 1 } } else { 0 };
        hatched_any |= n > 0;
        four |= n == 4;
        expected += n;
        assert_eq!(count(&mut h) - before, n, "egg {i}");
    }
    assert!(hatched_any && expected > 0, "the seed hatches nothing in 300 eggs");
    eprintln!("hatched {expected}, a group of four seen: {four}");
}

#[test]
fn a_pearl_takes_its_creature_owner_where_it_started_its_last_tick_without_harm() {
    if !have_data() {
        return;
    }
    let (r, mut h) = flat_pair();
    h.world.insert_resource(ThrownRng(LegacyRandom::new(SEED)));
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").unwrap();
    let owner = spawn_species(&mut h.world, &sp, 20.5, 20.5, 1);
    let mut hp = 0.0;
    let previous = throw(&mut h, &r, ProjectileKind::EnderPearl, ENDER_PEARL, Some(horizon_ecs_proto::targeting::Who::Entity(owner)), |h| {
        h.world.get_mut::<Contact>(owner).unwrap().fall_distance = 2.0;
        hp = h.world.get::<Health>(owner).unwrap().0;
    });
    let p = h.world.get::<Pos>(owner).unwrap().0;
    assert!(p.x == previous.x && p.y == previous.y && p.z == previous.z, "owner at {p:?}, pearl started its last tick at {previous:?}");
    assert_eq!(h.world.get::<Contact>(owner).unwrap().fall_distance, 0.0);
    assert_eq!(h.world.get::<Health>(owner).unwrap().0, hp, "no fall damage for a creature");
    assert!(h.drain_host_out().iter().all(|e| !matches!(e, HostEvent::Pearl { .. })));
}

#[test]
fn a_player_s_pearl_asks_for_the_teleport_the_fall_and_a_one_in_twenty_endermite() {
    if !have_data() {
        return;
    }
    let (r, mut h) = flat_pair();
    h.world.insert_resource(ThrownRng(LegacyRandom::new(SEED)));
    let mut want = LegacyRandom::new(SEED);
    let mut seen = [false; 2];
    for i in 0..200 {
        let _ = h.drain_host_out();
        let previous = throw(&mut h, &r, ProjectileKind::EnderPearl, ENDER_PEARL, Some(horizon_ecs_proto::targeting::Who::Player(7)), |_| {});
        let mite = want.next_float() < 0.05;
        seen[usize::from(mite)] = true;
        let got: Vec<_> = h.drain_host_out().into_iter().filter(|e| matches!(e, HostEvent::Pearl { .. })).collect();
        assert_eq!(got, vec![HostEvent::Pearl { player: 7, to: [previous.x, previous.y, previous.z], damage: 5.0, endermite: mite }], "pearl {i}");
    }
    assert!(seen[0] && seen[1], "both outcomes of the 5% draw seen");
}
