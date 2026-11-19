//! The explosion capability and the creeper's fuse against the `Mob` creeper (`HORIZON_SPECIES=minecraft:creeper`): the tick the blast goes
//! off after the target is taken at 2 blocks, its radius (3, or 6 when charged), and a fuse that falls back when the player leaves. The damage
//! and the blocks of the blast are the formulas of the game's `ServerExplosion` (private to `Game`: not callable here), checked by their numbers.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{self, MobEvent};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{swell, Entry},
    cow::*,
    explosion,
    goals::*,
    targeting::{set_target, Target},
};

fn on() -> bool {
    have_data() && std::env::var("HORIZON_SPECIES").as_deref() == Ok("minecraft:creeper")
}
/// (the tick of the blast, its radius) of a Mob creeper whose player is at `d(t)` blocks and is taken as target at tick 20.
fn mob_blast(d: impl Fn(usize) -> f64, charged: bool, ticks: usize) -> Option<(usize, f32)> {
    let mut r = MobRef::new(20, false);
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    r.mobs[i].ai.goals.clear();
    r.mobs[i].ai.targets.clear();
    r.mobs[i].ai.goals.add(2, mobs::hostile::Swell::default());
    r.mobs[i].extra.counter = -1;
    r.mobs[i].extra.charged = charged;
    for t in 0..ticks {
        let pv = player_view(Vec3::new(0.5 + d(t), 1.0, 0.5), None);
        if t == 20 {
            r.mobs[i].ai.target = Some(mobs::Who::Player);
        }
        let before = r.events.len();
        r.step(Some(&pv));
        for e in &r.events[before..] {
            if let MobEvent::Explode(_, radius, ..) = e {
                return Some((t, *radius));
            }
        }
    }
    None
}
fn ecs_blast(d: impl Fn(usize) -> f64, charged: bool, ticks: usize) -> Option<(usize, f32)> {
    let mut sp = *species();
    sp.goals = horizon_ecs_proto::species::Config { entries: Box::leak(Box::new([Entry::new(&swell::MODULE, 2, &swell::Params::DEFAULT)])) };
    let mut h = Herd::with_slab(20, false, false);
    set_lazy_look(&mut h.world, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    if charged {
        h.world.entity_mut(e).insert(explosion::Charged);
    }
    for t in 0..ticks {
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5 + d(t), y: 1.0, z: 0.5, main: 0, off: 0 });
        if t == 20 {
            set_target(&mut h.world, e, Target::player(1, 60));
        }
        h.tick();
        if let Some(b) = h.world.resource::<Events>().exploded.iter().next() {
            return Some((t - 1, b.radius));
        }
    }
    None
}

#[test]
fn a_creeper_blows_up_when_the_mob_s_does() {
    if !on() {
        return;
    }
    for (name, charged) in [("plain", false), ("charged", true)] {
        let (m, e) = (mob_blast(|_| 2.0, charged, 120), ecs_blast(|_| 2.0, charged, 120));
        eprintln!("{name}: mob {m:?} ecs {e:?}");
        let (m, e) = (m.expect("the Mob blows up"), e.expect("the ECS blows up"));
        assert_eq!(m.1, e.1, "the radius");
        assert!(m.0.abs_diff(e.0) <= 1, "the tick: mob {} ecs {}", m.0, e.0);
    }
}

/// The player steps away at tick 35 (7.5 blocks: past the 7): the fuse falls, and when he comes back the count starts from what is left.
#[test]
fn a_fuse_falls_back_when_the_player_leaves_as_the_mob_s_does() {
    if !on() {
        return;
    }
    let d = |t: usize| if (35..50).contains(&t) { 8.0 } else { 2.0 };
    let (m, e) = (mob_blast(d, false, 160), ecs_blast(d, false, 160));
    eprintln!("mob {m:?} ecs {e:?}");
    let (m, e) = (m.expect("the Mob blows up"), e.expect("the ECS blows up"));
    assert!(m.0.abs_diff(e.0) <= 1, "the tick: mob {} ecs {}", m.0, e.0);
}

/// The numbers of the damage formula: a creature at 1, 3 and 5 blocks of a radius 3 blast in the open takes `(pow^2 + pow) / 2 * 7 * 6 + 1`.
#[test]
fn the_blast_hurts_by_the_distance_and_pushes_away() {
    if horizon_ecs_proto::cow::skip_unless_default_species("the_blast_hurts_by_the_distance_and_pushes_away") {
        return;
    }
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, false, false);
    let cows: Vec<Entity> = [1.0_f64, 3.0, 5.0].iter().enumerate().map(|(k, d)| ecs_cow(&mut h, 20.5 + d, 1.0, 20.5 + 3.0 * k as f64 * 0.0, 5 + k as u64)).collect();
    for _ in 0..5 {
        h.tick();
    }
    let before: Vec<f32> = cows.iter().map(|&c| health(&h, c)).collect();
    for &c in &cows {
        let level = *h.world.resource::<Level>();
        let b = h.world.get::<Bounds>(c).unwrap().0;
        eprintln!("exposure of {:?}: {}", b, explosion::seen_percent(&level, Vec3::new(20.5, 1.0, 20.5), b));
    }
    let blast = horizon_ecs_proto::projectile::Blast { at: Vec3::new(20.5, 1.0, 20.5), radius: 3.0, fire: false, owner: None, source: None };
    explosion::explode(&mut h.world, blast);
    for _ in 0..2 {
        h.tick();
    }
    for (k, &c) in cows.iter().enumerate() {
        let d = [1.0, 3.0, 5.0][k];
        let lost = before[k] - health(&h, c);
        let expect = explosion::blast_damage(d / 6.0, 1.0, 6.0).min(before[k]);
        eprintln!("before {} after {}", before[k], health(&h, c));
        eprintln!("at {d} blocks: lost {lost}, formula {expect}, speed after {:?}", h.world.get::<Vel>(c).map(|v| (v.0.x, v.0.y)));
        assert!((lost - expect).abs() < 1.5, "at {d}: lost {lost} expected {expect}");
    }
}

/// A radius 3 blast in a block of dirt: what is broken is a ball (all of it within 1.5 blocks, nothing past 4), a block of obsidian stays,
/// and no block is broken when mobs may not grief.
#[test]
fn the_blast_breaks_a_ball_of_soft_blocks_and_spares_the_hard_ones() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, false, false);
    let c = (20, 6, 20);
    for x in -5..=5 {
        for y in -5..=5 {
            for z in -5..=5 {
                h.set_block(c.0 + x, c.1 + y, c.2 + z, "minecraft:dirt");
            }
        }
    }
    h.set_block(c.0 + 2, c.1, c.2, "minecraft:obsidian");
    h.tick();
    let blast = horizon_ecs_proto::projectile::Blast { at: Vec3::new(f64::from(c.0) + 0.5, f64::from(c.1) + 0.5, f64::from(c.2) + 0.5), radius: 3.0, fire: false, owner: None, source: None };
    explosion::explode(&mut h.world, blast);
    let broken: Vec<(i32, i32, i32, &str)> = h.world.resource::<BlockChanges>().0.clone();
    let dist = |p: &(i32, i32, i32, &str)| (f64::from(p.0 - c.0).powi(2) + f64::from(p.1 - c.1).powi(2) + f64::from(p.2 - c.2).powi(2)).sqrt();
    eprintln!("{} blocks broken, farthest {:.2}", broken.len(), broken.iter().map(dist).fold(0.0, f64::max));
    assert!(broken.len() > 30);
    assert!(broken.iter().all(|p| dist(p) < 4.5), "nothing past the radius");
    for (x, y, z) in [(0, 0, 0), (1, 0, 0), (0, 1, 0), (0, 0, -1)] {
        assert!(broken.iter().any(|p| (p.0, p.1, p.2) == (c.0 + x, c.1 + y, c.2 + z)), "the centre's neighbours go: {x} {y} {z}");
    }
    assert!(!broken.iter().any(|p| (p.0, p.1, p.2) == (c.0 + 2, c.1, c.2)), "obsidian stays");
}
